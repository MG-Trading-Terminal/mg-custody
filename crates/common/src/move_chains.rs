//! Aptos and Sui chain support (Move VM-based L1s).
//!
//! Chain quirks handled here:
//!
//! **Aptos**:
//! - REST API (not JSON-RPC) — GET/POST to `/v1/` endpoints.
//! - Amounts in Octas: 1 APT = 10^8 Octas (8 decimal places).
//! - 32-byte hex addresses with 0x prefix.
//! - Ed25519 keys; BCS serialization for transactions.
//! - Account sequence number acts as nonce.
//!
//! **Sui**:
//! - JSON-RPC 2.0 API (POST to single endpoint).
//! - Amounts in MIST: 1 SUI = 10^9 MIST (9 decimal places).
//! - 32-byte hex addresses with 0x prefix.
//! - Ed25519 keys; BCS serialization; object-based UTXO model.
//! - Checkpoints replace block numbers for finality tracking.
//!
//! References:
//! - Aptos REST API: https://fullnode.mainnet.aptoslabs.com/v1/spec
//! - Sui JSON-RPC: https://docs.sui.io/sui-api-ref
//! - APT decimals: https://aptos.dev/en/network/blockchain/base-token
//! - SUI decimals: https://docs.sui.io/concepts/tokenomics

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 1 APT = 10^8 Octas.
pub const APT_DECIMALS: u8 = 8;

/// 1 SUI = 10^9 MIST.
pub const SUI_DECIMALS: u8 = 9;

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SuiRpcRequest<'a, T: Serialize> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: T,
}

#[derive(Deserialize)]
struct SuiRpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Aptos Scanner
// ---------------------------------------------------------------------------

/// Aptos REST API client for deposit scanning and balance queries.
pub struct AptosScanner {
    /// Base URL, e.g. `https://fullnode.mainnet.aptoslabs.com/v1`.
    pub rest_url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct AptosLedger {
    /// Current ledger version (acts like a block number).
    ledger_version: String,
}

#[derive(Deserialize)]
struct AptosCoinStore {
    data: AptosCoinStoreData,
}

#[derive(Deserialize)]
struct AptosCoinStoreData {
    coin: AptosCoin,
}

#[derive(Deserialize)]
struct AptosCoin {
    value: String,
}

impl AptosScanner {
    /// Create a scanner with a custom REST base URL.
    pub fn new(rest_url: impl Into<String>) -> Self {
        Self {
            rest_url: rest_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Mainnet scanner using the official Aptos Labs full-node.
    pub fn mainnet() -> Self {
        Self::new("https://fullnode.mainnet.aptoslabs.com/v1")
    }

    /// Devnet scanner.
    pub fn devnet() -> Self {
        Self::new("https://fullnode.devnet.aptoslabs.com/v1")
    }

    /// Return the current ledger version (analogous to block number).
    pub async fn latest_version(&self) -> Result<u64> {
        let url = format!("{}/", self.rest_url);
        let resp: AptosLedger = self
            .client
            .get(&url)
            .send()
            .await
            .context("Aptos REST ledger request failed")?
            .json()
            .await
            .context("Aptos ledger response parse failed")?;
        resp.ledger_version
            .parse::<u64>()
            .context("Aptos ledger_version is not a valid u64")
    }

    /// Return the APT balance for `address` as a `Decimal` (8 dp).
    ///
    /// Fetches `0x1::coin::CoinStore<0x1::aptos_coin::AptosCoin>` resource.
    pub async fn get_balance(&self, address: &str) -> Result<Decimal> {
        validate_aptos_address(address)?;
        let resource_type =
            "0x1::coin::CoinStore%3C0x1::aptos_coin::AptosCoin%3E";
        let url = format!(
            "{}/accounts/{}/resource/{}",
            self.rest_url,
            address,
            resource_type
        );
        let resp: AptosCoinStore = self
            .client
            .get(&url)
            .send()
            .await
            .context("Aptos balance request failed")?
            .json()
            .await
            .context("Aptos balance response parse failed")?;

        let octas = Decimal::from_str(&resp.data.coin.value)
            .context("Aptos coin value is not a valid decimal")?;
        Ok(octas_to_apt(octas))
    }
}

// ---------------------------------------------------------------------------
// Sui Scanner
// ---------------------------------------------------------------------------

/// Sui JSON-RPC client for deposit scanning and balance queries.
pub struct SuiScanner {
    /// Full-node RPC URL, e.g. `https://fullnode.mainnet.sui.io:443`.
    pub rpc_url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SuiBalance {
    /// Total balance in MIST (as a string).
    #[serde(rename = "totalBalance")]
    total_balance: String,
}

impl SuiScanner {
    /// Create a scanner with a custom RPC URL.
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Mainnet scanner using the official Sui Foundation full-node.
    pub fn mainnet() -> Self {
        Self::new("https://fullnode.mainnet.sui.io:443")
    }

    /// Devnet scanner.
    pub fn devnet() -> Self {
        Self::new("https://fullnode.devnet.sui.io:443")
    }

    /// Return the latest checkpoint sequence number (analogous to block number).
    pub async fn latest_checkpoint(&self) -> Result<u64> {
        let req = SuiRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "sui_getLatestCheckpointSequenceNumber",
            params: serde_json::json!([]),
        };
        let resp: SuiRpcResponse<String> = self
            .client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .context("Sui RPC checkpoint request failed")?
            .json()
            .await
            .context("Sui RPC checkpoint response parse failed")?;

        if let Some(err) = resp.error {
            anyhow::bail!("Sui RPC error: {err}");
        }
        resp.result
            .context("Sui checkpoint result missing")?
            .parse::<u64>()
            .context("Sui checkpoint is not a valid u64")
    }

    /// Return the SUI balance for `address` as a `Decimal` (9 dp).
    pub async fn get_balance(&self, address: &str) -> Result<Decimal> {
        validate_sui_address(address)?;
        let req = SuiRpcRequest {
            jsonrpc: "2.0",
            id: 2,
            method: "suix_getBalance",
            params: serde_json::json!([address, "0x2::sui::SUI"]),
        };
        let resp: SuiRpcResponse<SuiBalance> = self
            .client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .context("Sui balance request failed")?
            .json()
            .await
            .context("Sui balance response parse failed")?;

        if let Some(err) = resp.error {
            anyhow::bail!("Sui RPC error: {err}");
        }
        let mist = Decimal::from_str(
            &resp.result.context("Sui balance result missing")?.total_balance,
        )
        .context("Sui totalBalance is not a valid decimal")?;
        Ok(mist_to_sui(mist))
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Convert Octas (Aptos base unit) to APT Decimal.
///
/// 1 APT = 10^8 Octas.
pub fn octas_to_apt(octas: Decimal) -> Decimal {
    octas / Decimal::new(100_000_000, 0)
}

/// Convert APT Decimal to Octas integer.
pub fn apt_to_octas(apt: Decimal) -> u64 {
    (apt * Decimal::new(100_000_000, 0)).trunc().to_string().parse().unwrap_or(0)
}

/// Convert MIST (Sui base unit) to SUI Decimal.
///
/// 1 SUI = 10^9 MIST.
pub fn mist_to_sui(mist: Decimal) -> Decimal {
    mist / Decimal::new(1_000_000_000, 0)
}

/// Convert SUI Decimal to MIST integer.
pub fn sui_to_mist(sui: Decimal) -> u64 {
    (sui * Decimal::new(1_000_000_000, 0)).trunc().to_string().parse().unwrap_or(0)
}

/// Validate an Aptos address (0x-prefixed, 64 hex chars).
pub fn validate_aptos_address(addr: &str) -> Result<()> {
    let hex = addr
        .strip_prefix("0x")
        .ok_or_else(|| anyhow::anyhow!("Aptos address must start with 0x: {addr}"))?;
    anyhow::ensure!(
        hex.len() == 64,
        "Aptos address must be 64 hex chars after 0x, got {}: {addr}",
        hex.len()
    );
    anyhow::ensure!(
        hex.chars().all(|c| c.is_ascii_hexdigit()),
        "Aptos address contains non-hex characters: {addr}"
    );
    Ok(())
}

/// Validate a Sui address (0x-prefixed, 64 hex chars — same format as Aptos).
pub fn validate_sui_address(addr: &str) -> Result<()> {
    let hex = addr
        .strip_prefix("0x")
        .ok_or_else(|| anyhow::anyhow!("Sui address must start with 0x: {addr}"))?;
    anyhow::ensure!(
        hex.len() == 64,
        "Sui address must be 64 hex chars after 0x, got {}: {addr}",
        hex.len()
    );
    anyhow::ensure!(
        hex.chars().all(|c| c.is_ascii_hexdigit()),
        "Sui address contains non-hex characters: {addr}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Signing support
// ---------------------------------------------------------------------------

/// Sign an Aptos transfer (simplified BCS serialization).
///
/// Real implementation would use full BCS encoding for RawTransaction.
pub fn sign_aptos_transfer(
    signing_key: &ed25519_dalek::SigningKey,
    to_address: &str,
    amount_octas: u64,
    sequence_number: u64,
    chain_id: u8,
) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer;

    validate_aptos_address(to_address)?;

    // Simplified transaction payload (real impl needs BCS)
    let mut payload = Vec::new();
    // Transaction header
    payload.push(chain_id);
    payload.extend_from_slice(&sequence_number.to_le_bytes());
    // Destination (32 bytes from hex)
    let to_hex = to_address.strip_prefix("0x").unwrap_or(to_address);
    let to_bytes = hex::decode(to_hex).context("Invalid Aptos address hex")?;
    payload.extend_from_slice(&to_bytes);
    // Amount
    payload.extend_from_slice(&amount_octas.to_le_bytes());

    let signature = signing_key.sign(&payload);

    let mut signed = Vec::new();
    signed.extend_from_slice(&payload);
    signed.extend_from_slice(&signature.to_bytes());
    signed.extend_from_slice(&signing_key.verifying_key().to_bytes());

    Ok(signed)
}

/// Sign a Sui transfer (simplified).
///
/// Real implementation would use full BCS TransactionData encoding.
pub fn sign_sui_transfer(
    signing_key: &ed25519_dalek::SigningKey,
    to_address: &str,
    amount_mist: u64,
    gas_budget: u64,
) -> Result<Vec<u8>> {
    use ed25519_dalek::Signer;

    validate_sui_address(to_address)?;

    let mut payload = Vec::new();
    // Destination
    let to_hex = to_address.strip_prefix("0x").unwrap_or(to_address);
    let to_bytes = hex::decode(to_hex).context("Invalid Sui address hex")?;
    payload.extend_from_slice(&to_bytes);
    // Amount + gas
    payload.extend_from_slice(&amount_mist.to_le_bytes());
    payload.extend_from_slice(&gas_budget.to_le_bytes());

    let signature = signing_key.sign(&payload);

    let mut signed = Vec::new();
    // Sui uses "flag + signature + pubkey" scheme
    signed.push(0x00); // Ed25519 flag
    signed.extend_from_slice(&signature.to_bytes());
    signed.extend_from_slice(&signing_key.verifying_key().to_bytes());

    Ok(signed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- APT decimals ---

    #[test]
    fn octas_to_apt_one_apt() {
        // 100_000_000 octas = 1 APT
        let apt = octas_to_apt(Decimal::new(100_000_000, 0));
        assert_eq!(apt, dec!(1));
    }

    #[test]
    fn octas_to_apt_half_apt() {
        let apt = octas_to_apt(Decimal::new(50_000_000, 0));
        assert_eq!(apt, dec!(0.5));
    }

    #[test]
    fn apt_to_octas_round_trip() {
        let original = dec!(2.5);
        let octas = apt_to_octas(original);
        assert_eq!(octas, 250_000_000);
        let back = octas_to_apt(Decimal::new(octas as i64, 0));
        assert_eq!(back, original);
    }

    #[test]
    fn apt_decimals_constant_is_8() {
        assert_eq!(APT_DECIMALS, 8);
    }

    // --- SUI decimals ---

    #[test]
    fn mist_to_sui_one_sui() {
        // 1_000_000_000 MIST = 1 SUI
        let sui = mist_to_sui(Decimal::new(1_000_000_000, 0));
        assert_eq!(sui, dec!(1));
    }

    #[test]
    fn mist_to_sui_fractional() {
        let sui = mist_to_sui(Decimal::new(500_000_000, 0));
        assert_eq!(sui, dec!(0.5));
    }

    #[test]
    fn sui_to_mist_round_trip() {
        let original = dec!(3.14);
        let mist = sui_to_mist(original);
        let back = mist_to_sui(Decimal::new(mist as i64, 0));
        // Truncation means we lose sub-MIST precision — within 1 MIST is fine.
        let diff = (back - original).abs();
        assert!(diff < dec!(0.000000002), "diff: {diff}");
    }

    #[test]
    fn sui_decimals_constant_is_9() {
        assert_eq!(SUI_DECIMALS, 9);
    }

    // --- Aptos address validation ---

    #[test]
    fn aptos_address_valid() {
        let addr = format!("0x{}", "a".repeat(64));
        assert!(validate_aptos_address(&addr).is_ok());
    }

    #[test]
    fn aptos_address_rejects_no_prefix() {
        let addr = "a".repeat(64);
        assert!(validate_aptos_address(&addr).is_err());
    }

    #[test]
    fn aptos_address_rejects_short() {
        assert!(validate_aptos_address("0xdeadbeef").is_err());
    }

    #[test]
    fn aptos_address_rejects_non_hex() {
        let addr = format!("0x{}zzzz", "a".repeat(60));
        assert!(validate_aptos_address(&addr).is_err());
    }

    // --- Sui address validation ---

    #[test]
    fn sui_address_valid() {
        let addr = format!("0x{}", "f".repeat(64));
        assert!(validate_sui_address(&addr).is_ok());
    }

    #[test]
    fn sui_address_rejects_no_prefix() {
        assert!(validate_sui_address(&"f".repeat(64)).is_err());
    }

    // --- Scanner constructors ---

    #[test]
    fn aptos_mainnet_url() {
        let s = AptosScanner::mainnet();
        assert_eq!(s.rest_url, "https://fullnode.mainnet.aptoslabs.com/v1");
    }

    #[test]
    fn sui_mainnet_url() {
        let s = SuiScanner::mainnet();
        assert_eq!(s.rpc_url, "https://fullnode.mainnet.sui.io:443");
    }

    // --- Signing ---

    #[test]
    fn aptos_signing_deterministic() {
        let secret = [0x42u8; 32];
        let key = ed25519_dalek::SigningKey::from_bytes(&secret);
        let addr = "0x0000000000000000000000000000000000000000000000000000000000000001";

        let tx1 = sign_aptos_transfer(&key, addr, 100_000_000, 0, 1).unwrap();
        let tx2 = sign_aptos_transfer(&key, addr, 100_000_000, 0, 1).unwrap();
        assert_eq!(tx1, tx2, "Same inputs = same signed tx");

        let tx3 = sign_aptos_transfer(&key, addr, 200_000_000, 0, 1).unwrap();
        assert_ne!(tx1, tx3, "Different amount = different tx");
    }

    #[test]
    fn aptos_signing_invalid_address() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[0x42u8; 32]);
        assert!(sign_aptos_transfer(&key, "not-valid", 100, 0, 1).is_err());
        assert!(sign_aptos_transfer(&key, "0xshort", 100, 0, 1).is_err());
    }

    #[test]
    fn sui_signing_deterministic() {
        let secret = [0x42u8; 32];
        let key = ed25519_dalek::SigningKey::from_bytes(&secret);
        let addr = "0x0000000000000000000000000000000000000000000000000000000000000002";

        let tx1 = sign_sui_transfer(&key, addr, 1_000_000_000, 10_000_000).unwrap();
        let tx2 = sign_sui_transfer(&key, addr, 1_000_000_000, 10_000_000).unwrap();
        assert_eq!(tx1, tx2, "Same inputs = same signed tx");

        // Sui signature format: flag(1) + sig(64) + pubkey(32) = 97 bytes
        assert_eq!(tx1.len(), 97, "Sui signed tx = flag + sig + pubkey");
        assert_eq!(tx1[0], 0x00, "Ed25519 flag byte");
    }

    #[test]
    fn sui_signing_invalid_address() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[0x42u8; 32]);
        assert!(sign_sui_transfer(&key, "invalid", 1000, 1000).is_err());
    }
}
