//! TON (The Open Network) chain support.
//!
//! Architecture: TON uses Ed25519 keys, workchain-based addressing,
//! and a unique cell-based transaction format (BoC — Bag of Cells).
//!
//! TON USDT (jUSDT Jetton) is one of the highest-volume stablecoin transfers.
//!
//! References:
//! - TON Addresses: https://docs.ton.org/learn/overviews/addresses
//! - TON HTTP API: https://toncenter.com/api/v2/
//! - Jettons (TRC-20 equivalent): https://docs.ton.org/develop/dapps/asset-processing/jettons

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

/// TON wallet version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TonWalletVersion {
    V3R2,
    V4R2,
    V5,
}

impl Default for TonWalletVersion {
    fn default() -> Self { Self::V4R2 }
}

/// TON address in raw format (workchain:hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TonAddress {
    pub workchain: i8,
    pub hash: [u8; 32],
    /// User-friendly Base64url encoded address
    pub friendly: String,
    /// Raw hex format: workchain:hash_hex
    pub raw: String,
}

/// TON scanner config.
#[derive(Debug, Clone)]
pub struct TonScannerConfig {
    pub api_url: String,
    pub api_key: Option<String>,
}

impl TonScannerConfig {
    pub fn mainnet() -> Self {
        Self {
            api_url: "https://toncenter.com/api/v2".into(),
            api_key: None,
        }
    }

    pub fn testnet() -> Self {
        Self {
            api_url: "https://testnet.toncenter.com/api/v2".into(),
            api_key: None,
        }
    }
}

/// TON transaction from API.
#[derive(Debug, Clone, Deserialize)]
pub struct TonTransaction {
    #[serde(rename = "@type")]
    pub tx_type: Option<String>,
    pub utime: u64,
    pub data: Option<String>,
    pub transaction_id: Option<TonTxId>,
    pub fee: Option<String>,
    pub in_msg: Option<TonMessage>,
    pub out_msgs: Option<Vec<TonMessage>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TonTxId {
    pub lt: Option<String>,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TonMessage {
    pub source: Option<String>,
    pub destination: Option<String>,
    pub value: Option<String>,
    pub msg_data: Option<TonMsgData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TonMsgData {
    #[serde(rename = "@type")]
    pub data_type: Option<String>,
    pub body: Option<String>,
    pub text: Option<String>,
}

/// TON Jetton (fungible token) transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JettonTransfer {
    pub jetton_master: String,
    pub sender: String,
    pub recipient: String,
    pub amount: Decimal,
    pub decimals: u8,
    pub symbol: String,
}

/// Known Jetton master contracts.
pub const JETTON_USDT: &str = "EQCxE6mUtQJKFnGfaROTKOt1lZbDiiX1kCixRv7Nw2Id_sDs";
pub const JETTON_USDC: &str = "EQB-MPwrd1G6WKNkLz_VnV6WCE2kMxO-LE1cg-y3HsnODGS";

/// Derive TON address from Ed25519 public key.
///
/// TON uses a state init hash approach:
/// address = SHA256(wallet_code || initial_data(pubkey))
/// For wallet v4r2, the standard encoding.
pub fn derive_ton_address(public_key: &[u8; 32], workchain: i8) -> TonAddress {
    // Simplified: SHA256 of pubkey as wallet state init
    // Real implementation would construct the actual StateInit BoC
    let mut hasher = Sha256::new();
    hasher.update(&[0xff]); // wallet v4r2 marker
    hasher.update(&[workchain as u8]);
    hasher.update(public_key);
    hasher.update(b"ton-wallet-v4r2");
    let hash_result = hasher.finalize();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_result);

    let raw = format!("{}:{}", workchain, hex::encode(hash));

    // User-friendly address: base64url(tag + workchain + hash + crc16)
    let friendly = encode_friendly_address(workchain, &hash, false);

    TonAddress {
        workchain,
        hash,
        friendly,
        raw,
    }
}

/// Encode a user-friendly TON address (Base64url with CRC16).
fn encode_friendly_address(workchain: i8, hash: &[u8; 32], bounceable: bool) -> String {
    let tag: u8 = if bounceable { 0x11 } else { 0x51 };
    let mut data = Vec::with_capacity(36);
    data.push(tag);
    data.push(workchain as u8);
    data.extend_from_slice(hash);

    // CRC16-CCITT
    let crc = crc16_ccitt(&data);
    data.push((crc >> 8) as u8);
    data.push((crc & 0xFF) as u8);

    base64_url_encode(&data)
}

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// Convert nanotons to TON. 1 TON = 1_000_000_000 nanotons.
pub fn nanoton_to_ton(nanoton: u64) -> Decimal {
    Decimal::new(nanoton as i64, 9)
}

/// Convert TON to nanotons.
pub fn ton_to_nanoton(ton: Decimal) -> u64 {
    use rust_decimal::prelude::ToPrimitive;
    (ton * Decimal::new(1_000_000_000, 0)).trunc().to_u64().unwrap_or(0)
}

/// Fetch recent transactions for an address via TON HTTP API.
pub async fn get_transactions(
    client: &reqwest::Client,
    config: &TonScannerConfig,
    address: &str,
    limit: u32,
) -> Result<Vec<TonTransaction>> {
    let mut url = format!(
        "{}/getTransactions?address={}&limit={}",
        config.api_url, address, limit
    );

    if let Some(ref key) = config.api_key {
        url.push_str(&format!("&api_key={}", key));
    }

    let resp: serde_json::Value = client
        .get(&url)
        .send().await.context("TON getTransactions")?
        .json().await.context("TON parse")?;

    if !resp["ok"].as_bool().unwrap_or(false) {
        anyhow::bail!("TON API error: {}", resp["error"].as_str().unwrap_or("unknown"));
    }

    let txs: Vec<TonTransaction> = serde_json::from_value(
        resp["result"].clone()
    ).unwrap_or_default();

    Ok(txs)
}

/// Get account balance via TON HTTP API.
pub async fn get_balance(
    client: &reqwest::Client,
    config: &TonScannerConfig,
    address: &str,
) -> Result<Decimal> {
    let url = format!("{}/getAddressBalance?address={}", config.api_url, address);

    let resp: serde_json::Value = client
        .get(&url)
        .send().await.context("TON getAddressBalance")?
        .json().await.context("TON balance parse")?;

    let balance_str = resp["result"].as_str().unwrap_or("0");
    let nanotons: u64 = balance_str.parse().unwrap_or(0);
    Ok(nanoton_to_ton(nanotons))
}

/// Sign a TON transfer (simplified).
///
/// Real implementation would construct a BoC (Bag of Cells) with:
/// - External message to wallet contract
/// - Internal message (transfer) as payload
/// - Ed25519 signature over the message hash
pub fn sign_ton_transfer(
    signing_key: &ed25519_dalek::SigningKey,
    to_address: &str,
    amount_nanotons: u64,
    seqno: u32,
) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer;

    // Construct transfer message (simplified — real impl needs BoC encoding)
    let mut message = Vec::new();
    message.extend_from_slice(&seqno.to_be_bytes());
    message.extend_from_slice(to_address.as_bytes());
    message.extend_from_slice(&amount_nanotons.to_be_bytes());

    // Sign
    let signature = signing_key.sign(&message);

    // Combine: signature + message (simplified envelope)
    let mut envelope = Vec::new();
    envelope.extend_from_slice(&signature.to_bytes());
    envelope.extend_from_slice(&message);

    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn nanoton_conversion() {
        assert_eq!(nanoton_to_ton(1_000_000_000), dec!(1.000000000));
        assert_eq!(nanoton_to_ton(500_000_000), dec!(0.500000000));
        assert_eq!(ton_to_nanoton(dec!(1.0)), 1_000_000_000);
        assert_eq!(ton_to_nanoton(dec!(2.5)), 2_500_000_000);
    }

    #[test]
    fn crc16_deterministic() {
        let crc1 = crc16_ccitt(b"test");
        let crc2 = crc16_ccitt(b"test");
        assert_eq!(crc1, crc2, "CRC16 must be deterministic");
        assert_ne!(crc16_ccitt(b"a"), crc16_ccitt(b"b"), "Different data = different CRC");
    }

    #[test]
    fn address_derivation_deterministic() {
        let pubkey = [0xAA; 32];
        let addr1 = derive_ton_address(&pubkey, 0);
        let addr2 = derive_ton_address(&pubkey, 0);
        assert_eq!(addr1.hash, addr2.hash);
        assert_eq!(addr1.friendly, addr2.friendly);
        assert_eq!(addr1.raw, addr2.raw);
    }

    #[test]
    fn address_different_workchain() {
        let pubkey = [0xBB; 32];
        let addr0 = derive_ton_address(&pubkey, 0);
        let addr1 = derive_ton_address(&pubkey, -1);
        assert_ne!(addr0.hash, addr1.hash);
        assert_ne!(addr0.friendly, addr1.friendly);
    }

    #[test]
    fn friendly_address_format() {
        let pubkey = [0xCC; 32];
        let addr = derive_ton_address(&pubkey, 0);
        // Friendly address should be base64url encoded, no padding
        assert!(!addr.friendly.contains('+'));
        assert!(!addr.friendly.contains('/'));
        assert!(!addr.friendly.contains('='));
        assert!(addr.friendly.len() >= 40);
    }

    #[test]
    fn raw_address_format() {
        let pubkey = [0xDD; 32];
        let addr = derive_ton_address(&pubkey, 0);
        assert!(addr.raw.starts_with("0:"));
        assert_eq!(addr.raw.len(), 2 + 64); // "0:" + 64 hex chars
    }

    #[test]
    fn jetton_constants() {
        assert!(JETTON_USDT.starts_with("EQ"));
        assert!(JETTON_USDC.starts_with("EQ"));
    }

    #[test]
    fn scanner_configs() {
        let mainnet = TonScannerConfig::mainnet();
        assert!(mainnet.api_url.contains("toncenter.com"));
        let testnet = TonScannerConfig::testnet();
        assert!(testnet.api_url.contains("testnet"));
    }

    #[test]
    fn sign_transfer_deterministic() {
        let secret = [0x42u8; 32];
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&secret);

        let tx1 = sign_ton_transfer(&signing_key, "EQTest", 1_000_000_000, 1).unwrap();
        let tx2 = sign_ton_transfer(&signing_key, "EQTest", 1_000_000_000, 1).unwrap();
        assert_eq!(tx1, tx2, "Same inputs must produce same signed tx");

        let tx3 = sign_ton_transfer(&signing_key, "EQTest", 2_000_000_000, 1).unwrap();
        assert_ne!(tx1, tx3, "Different amount must produce different tx");
    }

    #[tokio::test]
    async fn ton_api_mainnet_balance() {
        // Soft-fail if TON API unavailable
        let client = reqwest::Client::new();
        let config = TonScannerConfig::mainnet();
        // Query a known address (TON Foundation)
        match get_balance(&client, &config, "EQDtFpEwcFAEcRe5mLVh2N6C0x-_hJEM7W61_JLnSF74p4q2").await {
            Ok(balance) => {
                println!("TON Foundation balance: {} TON", balance);
                assert!(balance >= dec!(0));
            }
            Err(e) => {
                eprintln!("Skipping TON balance test: {e}");
            }
        }
    }
}
