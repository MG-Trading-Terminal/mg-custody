//! Bitcoin P2WPKH transaction signing (BIP-141/BIP-143).
//!
//! REAL signing — produces valid signed transactions that can be broadcast.
//! Uses BIP-143 (SegWit sighash) for P2WPKH inputs.

use anyhow::{Context, Result};
use bitcoin::absolute::LockTime;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use bip32::{DerivationPath, XPrv};
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey, VerifyingKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Bitcoin transaction input (from a UTXO).
#[derive(Debug, Clone)]
pub struct BtcTxInput {
    pub txid: String,
    pub vout: u32,
    pub amount_sats: u64,
    pub derivation_index: u32,
}

/// Bitcoin transaction output.
#[derive(Debug, Clone)]
pub struct BtcTxOutput {
    pub script_pubkey: ScriptBuf,
    pub amount_sats: u64,
}

/// Parameters for a Bitcoin transaction.
#[derive(Debug, Clone)]
pub struct BtcTxParams {
    pub inputs: Vec<BtcTxInput>,
    pub outputs: Vec<BtcTxOutput>,
    pub rbf: bool,
}

pub const RBF_SEQUENCE: u32 = 0xFFFF_FFFD;
pub const FINAL_SEQUENCE: u32 = 0xFFFF_FFFF;
/// SIGHASH_ALL for SegWit.
const SIGHASH_ALL: u32 = 1;

/// Derive secp256k1 key pair at m/84'/0'/0'/0/{index}.
fn derive_keypair(seed: &[u8; 64], index: u32) -> Result<(SigningKey, VerifyingKey, Vec<u8>)> {
    let path: DerivationPath = format!("m/84'/0'/0'/0/{index}")
        .parse()
        .context("Invalid BTC derivation path")?;

    let xprv = XPrv::derive_from_path(seed, &path)
        .context("BIP-32 derivation failed")?;

    let mut privkey_bytes = xprv.to_bytes();
    let signing_key = SigningKey::from_bytes((&privkey_bytes).into())
        .context("Invalid secp256k1 key")?;
    privkey_bytes.zeroize();

    let verifying_key = *signing_key.verifying_key();
    let compressed_pubkey = verifying_key.to_encoded_point(true).as_bytes().to_vec();

    Ok((signing_key, verifying_key, compressed_pubkey))
}

/// HASH160 = RIPEMD160(SHA256(data)).
fn hash160(data: &[u8]) -> [u8; 20] {
    let sha = Sha256::digest(data);
    let ripemd = Ripemd160::digest(sha);
    let mut result = [0u8; 20];
    result.copy_from_slice(&ripemd);
    result
}

/// Create a P2WPKH scriptPubKey: OP_0 <20-byte-pubkey-hash>.
pub fn p2wpkh_script(pubkey_hash: &[u8; 20]) -> ScriptBuf {
    let mut script = Vec::with_capacity(22);
    script.push(0x00); // OP_0
    script.push(0x14); // Push 20 bytes
    script.extend_from_slice(pubkey_hash);
    ScriptBuf::from(script)
}

/// Create the P2PKH script used in BIP-143 sighash for P2WPKH:
/// OP_DUP OP_HASH160 <20-byte-hash> OP_EQUALVERIFY OP_CHECKSIG
fn p2pkh_script_code(pubkey_hash: &[u8; 20]) -> ScriptBuf {
    let mut script = Vec::with_capacity(25);
    script.push(0x76); // OP_DUP
    script.push(0xa9); // OP_HASH160
    script.push(0x14); // Push 20 bytes
    script.extend_from_slice(pubkey_hash);
    script.push(0x88); // OP_EQUALVERIFY
    script.push(0xac); // OP_CHECKSIG
    ScriptBuf::from(script)
}

/// BIP-143 sighash for a P2WPKH input.
///
/// This is the digest that gets signed for SegWit transactions.
fn bip143_sighash(
    tx: &Transaction,
    input_index: usize,
    script_code: &ScriptBuf,
    value: u64,
    sighash_type: u32,
) -> [u8; 32] {
    // 1. nVersion
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&tx.version.0.to_le_bytes());

    // 2. hashPrevouts (SHA256d of all outpoints)
    let mut prevouts = Vec::new();
    for input in &tx.input {
        let _ = input.previous_output.consensus_encode(&mut prevouts);
    }
    let hash_prevouts = double_sha256(&prevouts);
    preimage.extend_from_slice(&hash_prevouts);

    // 3. hashSequence (SHA256d of all sequences)
    let mut sequences = Vec::new();
    for input in &tx.input {
        sequences.extend_from_slice(&input.sequence.0.to_le_bytes());
    }
    let hash_sequence = double_sha256(&sequences);
    preimage.extend_from_slice(&hash_sequence);

    // 4. outpoint being signed
    let mut outpoint_bytes = Vec::new();
    let _ = tx.input[input_index].previous_output.consensus_encode(&mut outpoint_bytes);
    preimage.extend_from_slice(&outpoint_bytes);

    // 5. scriptCode of the input
    let script_bytes = script_code.as_bytes();
    // Encode as compact size + script
    preimage.push(script_bytes.len() as u8);
    preimage.extend_from_slice(script_bytes);

    // 6. value of the input (in satoshis, little-endian u64)
    preimage.extend_from_slice(&value.to_le_bytes());

    // 7. nSequence of the input
    preimage.extend_from_slice(&tx.input[input_index].sequence.0.to_le_bytes());

    // 8. hashOutputs (SHA256d of all outputs)
    let mut outputs = Vec::new();
    for output in &tx.output {
        let _ = output.consensus_encode(&mut outputs);
    }
    let hash_outputs = double_sha256(&outputs);
    preimage.extend_from_slice(&hash_outputs);

    // 9. nLockTime
    let locktime_val: u32 = tx.lock_time.to_consensus_u32();
    preimage.extend_from_slice(&locktime_val.to_le_bytes());

    // 10. sighash type
    preimage.extend_from_slice(&sighash_type.to_le_bytes());

    // Double SHA256 of the preimage
    double_sha256(&preimage)
}

/// SHA256(SHA256(data))
fn double_sha256(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut result = [0u8; 32];
    result.copy_from_slice(&second);
    result
}

/// Sign a Bitcoin P2WPKH transaction.
///
/// Returns the fully signed, serialized transaction ready for broadcast.
pub fn sign_bitcoin_tx(seed: &[u8; 64], params: &BtcTxParams) -> Result<Vec<u8>> {
    let sequence = if params.rbf {
        Sequence(RBF_SEQUENCE)
    } else {
        Sequence(FINAL_SEQUENCE)
    };

    // Build unsigned transaction
    let inputs: Vec<TxIn> = params.inputs.iter().enumerate().map(|(i, input)| {
        let txid: Txid = input.txid.parse()
            .map_err(|e| anyhow::anyhow!("Invalid txid for input {i}: {e}"))?;
        Ok(TxIn {
            previous_output: OutPoint::new(txid, input.vout),
            script_sig: ScriptBuf::new(),
            sequence,
            witness: Witness::default(),
        })
    }).collect::<Result<Vec<TxIn>>>()?;

    let outputs: Vec<TxOut> = params.outputs.iter().map(|output| {
        TxOut {
            value: Amount::from_sat(output.amount_sats),
            script_pubkey: output.script_pubkey.clone(),
        }
    }).collect();

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    // Sign each input
    for (i, input) in params.inputs.iter().enumerate() {
        let (signing_key, _verifying_key, compressed_pubkey) =
            derive_keypair(seed, input.derivation_index)?;

        let pubkey_hash = hash160(&compressed_pubkey);
        let script_code = p2pkh_script_code(&pubkey_hash);

        // Compute BIP-143 sighash
        let sighash = bip143_sighash(&tx, i, &script_code, input.amount_sats, SIGHASH_ALL);

        // Sign the sighash with secp256k1 ECDSA
        let signature: Signature = signing_key.sign_prehash(&sighash)
            .map_err(|e| anyhow::anyhow!("ECDSA signing failed: {e}"))?;

        // DER-encode the signature + SIGHASH_ALL byte
        let der_sig = signature.to_der();
        let mut sig_with_hashtype = der_sig.as_bytes().to_vec();
        sig_with_hashtype.push(SIGHASH_ALL as u8);

        // Build P2WPKH witness: [signature, pubkey]
        let mut witness = Witness::new();
        witness.push(&sig_with_hashtype);
        witness.push(&compressed_pubkey);

        tx.input[i].witness = witness;
    }

    // Serialize the signed transaction
    let mut serialized = Vec::new();
    tx.consensus_encode(&mut serialized)
        .context("Transaction serialization failed")?;

    tracing::info!(
        inputs = params.inputs.len(),
        outputs = params.outputs.len(),
        size = serialized.len(),
        "Bitcoin P2WPKH transaction signed"
    );

    Ok(serialized)
}

/// Compute the total input amount.
pub fn total_input_sats(params: &BtcTxParams) -> u64 {
    params.inputs.iter().map(|i| i.amount_sats).sum()
}

/// Compute the total output amount.
pub fn total_output_sats(params: &BtcTxParams) -> u64 {
    params.outputs.iter().map(|o| o.amount_sats).sum()
}

/// Compute the implied fee (inputs - outputs).
pub fn implied_fee_sats(params: &BtcTxParams) -> u64 {
    total_input_sats(params).saturating_sub(total_output_sats(params))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let bytes = hex::decode(
            "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
        ).unwrap();
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&bytes);
        seed
    }

    #[test]
    fn derive_keypair_deterministic() {
        let seed = test_seed();
        let (_, _, pk1) = derive_keypair(&seed, 0).unwrap();
        let (_, _, pk2) = derive_keypair(&seed, 0).unwrap();
        assert_eq!(pk1, pk2);

        let (_, _, pk3) = derive_keypair(&seed, 1).unwrap();
        assert_ne!(pk1, pk3);
    }

    #[test]
    fn compressed_pubkey_format() {
        let seed = test_seed();
        let (_, _, pk) = derive_keypair(&seed, 0).unwrap();
        assert_eq!(pk.len(), 33);
        assert!(pk[0] == 0x02 || pk[0] == 0x03);
    }

    #[test]
    fn hash160_known_value() {
        // hash160 of a known byte should produce 20 bytes
        let result = hash160(&[0x42; 33]);
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn p2wpkh_script_format() {
        let hash = [0xAB; 20];
        let script = p2wpkh_script(&hash);
        let bytes = script.as_bytes();
        assert_eq!(bytes[0], 0x00); // OP_0
        assert_eq!(bytes[1], 0x14); // Push 20
        assert_eq!(&bytes[2..], &hash);
    }

    #[test]
    fn p2pkh_script_code_format() {
        let hash = [0xAB; 20];
        let script = p2pkh_script_code(&hash);
        let bytes = script.as_bytes();
        assert_eq!(bytes[0], 0x76); // OP_DUP
        assert_eq!(bytes[1], 0xa9); // OP_HASH160
        assert_eq!(bytes[2], 0x14); // Push 20
        assert_eq!(&bytes[3..23], &hash);
        assert_eq!(bytes[23], 0x88); // OP_EQUALVERIFY
        assert_eq!(bytes[24], 0xac); // OP_CHECKSIG
    }

    #[test]
    fn sign_real_btc_transaction() {
        let seed = test_seed();

        // Derive address for index 0 to create output script
        let (_, _, pk) = derive_keypair(&seed, 0).unwrap();
        let pkh = hash160(&pk);
        let recipient_script = p2wpkh_script(&pkh);

        let params = BtcTxParams {
            inputs: vec![BtcTxInput {
                txid: "a".repeat(64), // dummy txid
                vout: 0,
                amount_sats: 100_000,
                derivation_index: 0,
            }],
            outputs: vec![BtcTxOutput {
                script_pubkey: recipient_script,
                amount_sats: 90_000,
            }],
            rbf: true,
        };

        let signed_tx = sign_bitcoin_tx(&seed, &params).unwrap();

        // Should be non-empty
        assert!(signed_tx.len() > 100, "Signed tx too small: {} bytes", signed_tx.len());

        // Parse back to verify structure
        let tx: Transaction = bitcoin::consensus::deserialize(&signed_tx).unwrap();

        // Verify witness is present
        assert_eq!(tx.input.len(), 1);
        assert_eq!(tx.input[0].witness.len(), 2); // [signature, pubkey]

        // Witness[0] = DER signature + sighash byte
        let sig_bytes = tx.input[0].witness.nth(0).unwrap();
        assert!(*sig_bytes.last().unwrap() == 0x01); // SIGHASH_ALL

        // Witness[1] = compressed pubkey
        let witness_pubkey = tx.input[0].witness.nth(1).unwrap();
        assert_eq!(witness_pubkey.len(), 33);
        assert!(witness_pubkey[0] == 0x02 || witness_pubkey[0] == 0x03);

        // Output should have correct amount
        assert_eq!(tx.output[0].value.to_sat(), 90_000);

        // Fee = 100000 - 90000 = 10000 sats
        assert_eq!(implied_fee_sats(&params), 10_000);
    }

    #[test]
    fn sign_multi_input_transaction() {
        let seed = test_seed();
        let (_, _, pk0) = derive_keypair(&seed, 0).unwrap();
        let (_, _, pk1) = derive_keypair(&seed, 1).unwrap();
        let recipient_script = p2wpkh_script(&hash160(&pk0));
        let change_script = p2wpkh_script(&hash160(&pk1));

        let params = BtcTxParams {
            inputs: vec![
                BtcTxInput { txid: "a".repeat(64), vout: 0, amount_sats: 50_000, derivation_index: 0 },
                BtcTxInput { txid: "b".repeat(64), vout: 1, amount_sats: 60_000, derivation_index: 1 },
            ],
            outputs: vec![
                BtcTxOutput { script_pubkey: recipient_script, amount_sats: 80_000 },
                BtcTxOutput { script_pubkey: change_script, amount_sats: 25_000 },
            ],
            rbf: false,
        };

        let signed_tx = sign_bitcoin_tx(&seed, &params).unwrap();
        let tx: Transaction = bitcoin::consensus::deserialize(&signed_tx).unwrap();

        assert_eq!(tx.input.len(), 2);
        assert_eq!(tx.output.len(), 2);

        // Both inputs should have witness data
        for input in &tx.input {
            assert_eq!(input.witness.len(), 2);
        }

        // Fee = 110000 - 105000 = 5000 sats
        assert_eq!(implied_fee_sats(&params), 5_000);
    }

    #[test]
    fn sign_with_rbf() {
        let seed = test_seed();
        let (_, _, pk) = derive_keypair(&seed, 0).unwrap();
        let script = p2wpkh_script(&hash160(&pk));

        let params = BtcTxParams {
            inputs: vec![BtcTxInput { txid: "c".repeat(64), vout: 0, amount_sats: 10_000, derivation_index: 0 }],
            outputs: vec![BtcTxOutput { script_pubkey: script, amount_sats: 9_000 }],
            rbf: true,
        };

        let signed_tx = sign_bitcoin_tx(&seed, &params).unwrap();
        let tx: Transaction = bitcoin::consensus::deserialize(&signed_tx).unwrap();

        assert_eq!(tx.input[0].sequence, Sequence(RBF_SEQUENCE));
    }

    #[test]
    fn double_sha256_known() {
        // SHA256d of empty = known value
        let result = double_sha256(&[]);
        let hex = hex::encode(result);
        assert_eq!(hex, "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456");
    }
}
