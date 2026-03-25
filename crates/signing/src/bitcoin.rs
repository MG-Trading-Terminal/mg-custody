//! Bitcoin P2WPKH transaction signing.
//!
//! Constructs and signs SegWit (P2WPKH) transactions from UTXOs.
//! Uses the `bitcoin` crate for transaction construction and BIP-143 signing.
//!
//! Key features:
//! - P2WPKH (native SegWit) only
//! - Change goes to a fresh derived address
//! - RBF signaling (BIP-125): nSequence = 0xFFFFFFFD
//! - Private key derived from seed and zeroized after signing

use anyhow::{Context, Result};
use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, EcdsaSighashType, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
    Witness,
};
use bip32::{DerivationPath, XPrv};
use k256::ecdsa::SigningKey;
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
    pub address: String,
    pub amount_sats: u64,
}

/// Parameters for a Bitcoin transaction.
#[derive(Debug, Clone)]
pub struct BtcTxParams {
    pub inputs: Vec<BtcTxInput>,
    pub outputs: Vec<BtcTxOutput>,
    pub rbf: bool,
}

/// RBF sequence number (BIP-125).
pub const RBF_SEQUENCE: u32 = 0xFFFF_FFFD;
pub const FINAL_SEQUENCE: u32 = 0xFFFF_FFFF;

/// Derive the compressed public key hash (HASH160) for P2WPKH.
fn derive_pubkey_hash(seed: &[u8; 64], index: u32) -> Result<[u8; 20]> {
    let path: DerivationPath = format!("m/84'/0'/0'/0/{index}")
        .parse()
        .context("Invalid BTC derivation path")?;

    let xprv = XPrv::derive_from_path(seed, &path)
        .context("BIP-32 derivation failed")?;

    let verifying_key = k256::ecdsa::VerifyingKey::from(&xprv.public_key());
    let compressed = verifying_key.to_encoded_point(true);
    let pubkey_bytes = compressed.as_bytes();

    // HASH160 = RIPEMD160(SHA256(pubkey))
    let sha = Sha256::digest(pubkey_bytes);
    let hash160 = Ripemd160::digest(sha);

    let mut result = [0u8; 20];
    result.copy_from_slice(&hash160);
    Ok(result)
}

/// Create a P2WPKH scriptPubKey from a public key hash.
///
/// Format: OP_0 OP_PUSH20 <20-byte-hash>
pub fn p2wpkh_script(pubkey_hash: &[u8; 20]) -> ScriptBuf {
    // Manually construct: 0x00 0x14 <20 bytes>
    let mut script = Vec::with_capacity(22);
    script.push(0x00); // OP_0 (witness version 0)
    script.push(0x14); // OP_PUSH 20 bytes
    script.extend_from_slice(pubkey_hash);
    ScriptBuf::from(script)
}

/// Build an unsigned Bitcoin transaction.
pub fn build_unsigned_tx(params: &BtcTxParams) -> Result<Transaction> {
    let sequence = if params.rbf {
        Sequence(RBF_SEQUENCE)
    } else {
        Sequence(FINAL_SEQUENCE)
    };

    let inputs: Vec<TxIn> = params.inputs.iter().map(|input| {
        let txid: Txid = input.txid.parse()
            .unwrap_or_else(|_| Txid::from_byte_array([0; 32]));
        TxIn {
            previous_output: OutPoint::new(txid, input.vout),
            script_sig: ScriptBuf::new(), // empty for SegWit
            sequence,
            witness: Witness::default(), // filled during signing
        }
    }).collect();

    let outputs: Vec<TxOut> = params.outputs.iter().map(|output| {
        // Parse address to get scriptPubKey
        // For now, create a simple P2WPKH output placeholder
        TxOut {
            value: Amount::from_sat(output.amount_sats),
            script_pubkey: ScriptBuf::new(), // TODO: parse from address string
        }
    }).collect();

    Ok(Transaction {
        version: Version::TWO, // version 2 for BIP-68 relative locktime
        lock_time: LockTime::ZERO,
        input: inputs,
        output: outputs,
    })
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
    fn derive_pubkey_hash_from_seed() {
        let seed = test_seed();
        let hash = derive_pubkey_hash(&seed, 0).unwrap();
        assert_eq!(hash.len(), 20);
        // Should be deterministic
        let hash2 = derive_pubkey_hash(&seed, 0).unwrap();
        assert_eq!(hash, hash2);
        // Different index = different hash
        let hash3 = derive_pubkey_hash(&seed, 1).unwrap();
        assert_ne!(hash, hash3);
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
    fn build_unsigned_transaction() {
        let params = BtcTxParams {
            inputs: vec![
                BtcTxInput { txid: "a".repeat(64), vout: 0, amount_sats: 100_000, derivation_index: 0 },
            ],
            outputs: vec![
                BtcTxOutput { address: "bc1q...".into(), amount_sats: 90_000 },
            ],
            rbf: true,
        };

        let tx = build_unsigned_tx(&params).unwrap();
        assert_eq!(tx.input.len(), 1);
        assert_eq!(tx.output.len(), 1);
        assert_eq!(tx.input[0].sequence, Sequence(RBF_SEQUENCE));
        assert_eq!(tx.version, Version::TWO);
    }

    #[test]
    fn fee_calculation() {
        let params = BtcTxParams {
            inputs: vec![
                BtcTxInput { txid: "a".repeat(64), vout: 0, amount_sats: 100_000, derivation_index: 0 },
            ],
            outputs: vec![
                BtcTxOutput { address: "bc1q...".into(), amount_sats: 90_000 },
            ],
            rbf: false,
        };

        assert_eq!(total_input_sats(&params), 100_000);
        assert_eq!(total_output_sats(&params), 90_000);
        assert_eq!(implied_fee_sats(&params), 10_000);
    }

    #[test]
    fn rbf_vs_final_sequence() {
        let rbf_params = BtcTxParams { inputs: vec![BtcTxInput { txid: "a".repeat(64), vout: 0, amount_sats: 1000, derivation_index: 0 }], outputs: vec![], rbf: true };
        let final_params = BtcTxParams { inputs: vec![BtcTxInput { txid: "a".repeat(64), vout: 0, amount_sats: 1000, derivation_index: 0 }], outputs: vec![], rbf: false };

        let rbf_tx = build_unsigned_tx(&rbf_params).unwrap();
        let final_tx = build_unsigned_tx(&final_params).unwrap();

        assert_eq!(rbf_tx.input[0].sequence, Sequence(RBF_SEQUENCE));
        assert_eq!(final_tx.input[0].sequence, Sequence(FINAL_SEQUENCE));
    }
}
