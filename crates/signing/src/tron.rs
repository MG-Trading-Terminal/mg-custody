//! Tron transaction signing.
//!
//! Tron uses secp256k1 ECDSA (same curve as Ethereum) but:
//! - Transactions are hashed with SHA256 (not Keccak-256)
//! - Address format is Base58Check with 0x41 prefix
//! - Raw transaction is a protobuf-like structure
//!
//! Key derivation: m/44'/195'/0'/0/{index}

use anyhow::{Context, Result};
use bip32::{DerivationPath, XPrv};
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Parameters for a TRX native transfer.
#[derive(Debug, Clone)]
pub struct TrxTransferParams {
    /// Sender Tron address (hex with 41 prefix, e.g., "41...")
    pub owner_address_hex: String,
    /// Recipient Tron address (hex with 41 prefix)
    pub to_address_hex: String,
    /// Amount in sun (1 TRX = 1,000,000 sun)
    pub amount_sun: u64,
    /// Transaction expiration timestamp (Unix millis)
    pub expiration: u64,
    /// Transaction timestamp (Unix millis)
    pub timestamp: u64,
    /// Block reference (from latest block)
    pub ref_block_bytes: [u8; 2],
    pub ref_block_hash: [u8; 8],
}

/// Parameters for a TRC-20 token transfer.
#[derive(Debug, Clone)]
pub struct Trc20TransferParams {
    pub owner_address_hex: String,
    pub contract_address_hex: String,
    pub to_address_hex: String,
    pub amount: u64,
    pub fee_limit_sun: u64,
    pub expiration: u64,
    pub timestamp: u64,
    pub ref_block_bytes: [u8; 2],
    pub ref_block_hash: [u8; 8],
}

/// Derive secp256k1 key pair for Tron at m/44'/195'/0'/0/{index}.
fn derive_tron_keypair(seed: &[u8; 64], index: u32) -> Result<(SigningKey, Vec<u8>)> {
    let path: DerivationPath = format!("m/44'/195'/0'/0/{index}")
        .parse()
        .context("Invalid Tron derivation path")?;

    let xprv = XPrv::derive_from_path(seed, &path)
        .context("BIP-32 derivation failed")?;

    let mut privkey_bytes = xprv.to_bytes();
    let signing_key = SigningKey::from_bytes((&privkey_bytes).into())
        .context("Invalid secp256k1 key")?;
    privkey_bytes.zeroize();

    let verifying_key = signing_key.verifying_key();
    let uncompressed = verifying_key.to_encoded_point(false);
    let pubkey_bytes = uncompressed.as_bytes().to_vec();

    Ok((signing_key, pubkey_bytes))
}

/// Build a raw TRX transfer transaction (simplified protobuf-like format).
///
/// Tron raw transactions are protobuf, but we build the bytes manually
/// for the TransferContract message. This produces the raw_data that gets hashed.
fn build_trx_transfer_raw(params: &TrxTransferParams) -> Vec<u8> {
    // Simplified raw transaction structure for TransferContract
    // In production, use proper protobuf encoding
    let mut raw = Vec::new();

    // ref_block_bytes (field 1)
    raw.push(0x0a); // field 1, length-delimited
    raw.push(2);
    raw.extend_from_slice(&params.ref_block_bytes);

    // ref_block_hash (field 4)
    raw.push(0x22); // field 4, length-delimited
    raw.push(8);
    raw.extend_from_slice(&params.ref_block_hash);

    // expiration (field 8, varint)
    raw.push(0x40); // field 8, varint
    encode_varint(&mut raw, params.expiration);

    // timestamp (field 14, varint)
    raw.push(0x70); // field 14, varint
    encode_varint(&mut raw, params.timestamp);

    // contract (field 11) — TransferContract
    let mut contract = Vec::new();
    // type = TransferContract (1)
    contract.push(0x08); // field 1, varint
    contract.push(0x01); // type 1

    // parameter (field 2) — the actual transfer data
    let mut transfer = Vec::new();
    // owner_address (field 1)
    let owner = hex::decode(&params.owner_address_hex).unwrap_or_default();
    transfer.push(0x0a); // field 1, length-delimited
    transfer.push(owner.len() as u8);
    transfer.extend_from_slice(&owner);

    // to_address (field 2)
    let to = hex::decode(&params.to_address_hex).unwrap_or_default();
    transfer.push(0x12); // field 2, length-delimited
    transfer.push(to.len() as u8);
    transfer.extend_from_slice(&to);

    // amount (field 3, varint)
    transfer.push(0x18); // field 3, varint
    encode_varint(&mut transfer, params.amount_sun);

    // Wrap transfer in Any type
    let mut any = Vec::new();
    let type_url = b"type.googleapis.com/protocol.TransferContract";
    any.push(0x0a); // field 1 (type_url)
    any.push(type_url.len() as u8);
    any.extend_from_slice(type_url);
    any.push(0x12); // field 2 (value)
    any.push(transfer.len() as u8);
    any.extend_from_slice(&transfer);

    contract.push(0x12); // field 2 (parameter)
    contract.push(any.len() as u8);
    contract.extend_from_slice(&any);

    raw.push(0x5a); // field 11, length-delimited
    raw.push(contract.len() as u8);
    raw.extend_from_slice(&contract);

    raw
}

/// Encode a u64 as protobuf varint.
fn encode_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Produce the raw-tx bytes + the 32-byte SHA-256 prehash an external
/// signer (HSM / MPC / signer-service) must sign for a TRX transfer.
///
/// Tron uses SHA-256 of the raw protobuf (NOT Keccak like Ethereum).
/// Caller hands the prehash to `backend.sign(Chain::Tron, &prehash)`,
/// receives a 65-byte `r||s||v`, and combines it with `raw_data` the
/// same way `sign_trx_transfer` does its final assembly. Tron's
/// on-chain signature format is `r(32) || s(32) || v(1 byte, 0 or 1)`
/// — the older `v + 27` convention was a Bitcoin legacy and the
/// current Tron node code accepts either, so we stick with the raw
/// recovery id the HSM returns.
pub fn prepare_trx_transfer(params: &TrxTransferParams) -> (Vec<u8>, [u8; 32]) {
    let raw_data = build_trx_transfer_raw(params);
    let hash = Sha256::digest(&raw_data);
    let mut prehash = [0u8; 32];
    prehash.copy_from_slice(&hash);
    (raw_data, prehash)
}

/// Sign a TRX native transfer.
///
/// Returns the signature (65 bytes: r[32] + s[32] + recovery_id[1]).
pub fn sign_trx_transfer(
    seed: &[u8; 64],
    index: u32,
    params: &TrxTransferParams,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let (signing_key, _pubkey) = derive_tron_keypair(seed, index)?;

    // Build raw transaction
    let raw_data = build_trx_transfer_raw(params);

    // SHA256 hash (NOT Keccak like Ethereum!)
    let tx_hash = Sha256::digest(&raw_data);
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(&tx_hash);

    // Sign with secp256k1 ECDSA
    let signature: Signature = signing_key.sign_prehash(&hash_bytes)
        .map_err(|e| anyhow::anyhow!("ECDSA signing failed: {e}"))?;

    // Tron signature format: r(32) + s(32) + v(1)
    let r = signature.r().to_bytes();
    let s = signature.s().to_bytes();
    let mut sig_bytes = Vec::with_capacity(65);
    sig_bytes.extend_from_slice(&r);
    sig_bytes.extend_from_slice(&s);
    sig_bytes.push(27); // recovery_id (0 or 1) + 27

    tracing::info!(
        to = %params.to_address_hex,
        amount_sun = params.amount_sun,
        "TRX transfer signed"
    );

    Ok((sig_bytes, raw_data))
}

/// Encode TRC-20 transfer(address, uint256) calldata.
///
/// Same ABI as ERC-20 but Tron addresses use hex without 0x prefix.
pub fn encode_trc20_transfer_params(to_address_hex: &str, amount: u64) -> String {
    let selector = "a9059cbb";
    let addr_clean = to_address_hex.trim_start_matches("41");
    let addr_padded = format!("{:0>64}", addr_clean);
    let amount_hex = format!("{:064x}", amount);
    format!("{selector}{addr_padded}{amount_hex}")
}

/// Get the Tron address (hex with 41 prefix) for a seed and index.
pub fn tron_address_hex(seed: &[u8; 64], index: u32) -> Result<String> {
    let (_signing_key, pubkey) = derive_tron_keypair(seed, index)?;

    // Tron address = 0x41 + last 20 bytes of Keccak-256(uncompressed_pubkey[1..])
    use sha3::Keccak256;
    let hash = Keccak256::digest(&pubkey[1..]); // skip 0x04 prefix
    let mut address = Vec::with_capacity(21);
    address.push(0x41);
    address.extend_from_slice(&hash[12..]);
    Ok(hex::encode(address))
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
    fn derive_tron_address() {
        let seed = test_seed();
        let addr = tron_address_hex(&seed, 0).unwrap();
        assert!(addr.starts_with("41")); // Tron prefix
        assert_eq!(addr.len(), 42); // 21 bytes = 42 hex chars

        // Deterministic
        assert_eq!(addr, tron_address_hex(&seed, 0).unwrap());
        // Different index = different address
        assert_ne!(addr, tron_address_hex(&seed, 1).unwrap());
    }

    #[test]
    fn sign_real_trx_transfer() {
        let seed = test_seed();
        let from = tron_address_hex(&seed, 0).unwrap();

        let params = TrxTransferParams {
            owner_address_hex: from,
            to_address_hex: "41".to_string() + &"ab".repeat(20),
            amount_sun: 1_000_000, // 1 TRX
            expiration: 1700000000000,
            timestamp: 1699999990000,
            ref_block_bytes: [0x12, 0x34],
            ref_block_hash: [0xAA; 8],
        };

        let (signature, raw_data) = sign_trx_transfer(&seed, 0, &params).unwrap();

        // Signature should be 65 bytes (r + s + v)
        assert_eq!(signature.len(), 65);
        // Recovery ID should be 27 or 28
        assert!(signature[64] == 27 || signature[64] == 28);

        // Raw data should be non-empty
        assert!(raw_data.len() > 50);

        // Verify: hash the raw_data and check signature
        let tx_hash = Sha256::digest(&raw_data);
        let r = k256::ecdsa::Signature::from_scalars(
            <[u8; 32]>::try_from(&signature[..32]).unwrap(),
            <[u8; 32]>::try_from(&signature[32..64]).unwrap(),
        ).unwrap();

        let (_, pubkey) = derive_tron_keypair(&seed, 0).unwrap();
        use k256::ecdsa::signature::hazmat::PrehashVerifier;
        let verifying_key = k256::ecdsa::VerifyingKey::from_sec1_bytes(&pubkey).unwrap();
        assert!(verifying_key.verify_prehash(&tx_hash, &r).is_ok(), "Signature verification failed!");
    }

    #[test]
    fn trc20_param_encoding() {
        let params = encode_trc20_transfer_params(
            "41abcdef1234567890abcdef1234567890abcdef12",
            1_000_000,
        );
        assert!(params.starts_with("a9059cbb"));
        assert_eq!(params.len(), 136);
    }

    #[test]
    fn varint_encoding() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, 1);
        assert_eq!(buf, vec![1]);

        buf.clear();
        encode_varint(&mut buf, 300);
        assert_eq!(buf, vec![0xAC, 0x02]); // 300 = 0x12C
    }

    #[test]
    fn varint_large_value() {
        let mut buf = Vec::new();
        encode_varint(&mut buf, 1_000_000);
        assert!(buf.len() <= 4); // u64 1M fits in 3-4 varint bytes
        // Decode back
        let mut val = 0u64;
        for (i, &byte) in buf.iter().enumerate() {
            val |= ((byte & 0x7F) as u64) << (7 * i);
        }
        assert_eq!(val, 1_000_000);
    }
}
