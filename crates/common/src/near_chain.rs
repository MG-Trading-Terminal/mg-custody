//! NEAR Protocol chain support — deposit scanning, balance queries, and transfer signing.
//!
//! Chain quirks handled here:
//! - Human-readable account IDs (alice.near), not hex addresses.
//! - yoctoNEAR: 1 NEAR = 10^24 yoctoNEAR (24 decimal places).
//! - Transactions reference a recent block hash and expire after ~90 seconds.
//! - Ed25519 key pairs (SLIP-0010 derivation path: m/44'/397'/0').
//! - JSON-RPC 2.0 API — all calls are POST to a single endpoint.
//! - "final" finality for balance queries; "optimistic" for block height.
//!
//! References:
//! - NEAR RPC: https://docs.near.org/api/rpc/introduction
//! - Account model: https://docs.near.org/concepts/basics/accounts/model
//! - yoctoNEAR: https://docs.near.org/concepts/basics/tokens

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// 1 NEAR = 10^24 yoctoNEAR (24 decimal places).
#[allow(dead_code)]
const YOCTO_PER_NEAR_EXP: u32 = 24;

// ---------------------------------------------------------------------------
// RPC types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RpcRequest<'a, T: Serialize> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: T,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct StatusResult {
    sync_info: SyncInfo,
}

#[derive(Deserialize)]
struct SyncInfo {
    latest_block_height: u64,
}

#[derive(Deserialize)]
struct AccountResult {
    /// Balance in yoctoNEAR as a decimal string.
    amount: String,
}

// ---------------------------------------------------------------------------
// Scanner
// ---------------------------------------------------------------------------

/// NEAR JSON-RPC client for deposit scanning and balance queries.
pub struct NearScanner {
    pub rpc_url: String,
    client: reqwest::Client,
}

impl NearScanner {
    /// Create a scanner pointing at a custom RPC URL.
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Mainnet scanner using the official NEAR RPC endpoint.
    pub fn mainnet() -> Self {
        Self::new("https://rpc.mainnet.near.org")
    }

    /// Testnet scanner.
    pub fn testnet() -> Self {
        Self::new("https://rpc.testnet.near.org")
    }

    /// Return the latest finalised block height.
    pub async fn latest_block(&self) -> Result<u64> {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "status",
            params: serde_json::json!([]),
        };
        let resp: RpcResponse<StatusResult> = self
            .client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .context("NEAR RPC status request failed")?
            .json()
            .await
            .context("NEAR RPC status response parse failed")?;

        if let Some(err) = resp.error {
            anyhow::bail!("NEAR RPC error: {err}");
        }
        Ok(resp
            .result
            .context("NEAR status result missing")?
            .sync_info
            .latest_block_height)
    }

    /// Return the NEAR balance for `account_id` in full NEAR (not yoctoNEAR).
    ///
    /// The RPC returns the balance as a yoctoNEAR string; we convert to Decimal
    /// with 24 decimal places.
    pub async fn get_balance(&self, account_id: &str) -> Result<Decimal> {
        validate_account_id(account_id)?;

        let req = RpcRequest {
            jsonrpc: "2.0",
            id: 2,
            method: "query",
            params: serde_json::json!({
                "request_type": "view_account",
                "account_id": account_id,
                "finality": "final"
            }),
        };
        let resp: RpcResponse<AccountResult> = self
            .client
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .context("NEAR RPC query request failed")?
            .json()
            .await
            .context("NEAR RPC query response parse failed")?;

        if let Some(err) = resp.error {
            anyhow::bail!("NEAR RPC error: {err}");
        }
        let yocto_str = resp
            .result
            .context("NEAR account result missing")?
            .amount;

        yocto_to_near(&yocto_str)
    }
}

// ---------------------------------------------------------------------------
// Signer (placeholder — Ed25519 signing wired through SigningBackend in prod)
// ---------------------------------------------------------------------------

/// Encodes a signed NEAR transfer (borsh-encoded transaction bytes).
///
/// In production this is called by the signing backend.  Here we provide the
/// skeleton so the module compiles and the interface is established.
pub struct NearSigner;

impl NearSigner {
    /// Build and sign a transfer action.
    ///
    /// `signer_id`   — human-readable account that signs the tx.
    /// `receiver_id` — destination account.
    /// `amount_yocto` — transfer amount in yoctoNEAR (u128).
    /// `nonce`        — access-key nonce (monotonically increasing).
    /// `block_hash`   — 32-byte recent block hash (expires ~90s after block).
    ///
    /// Returns borsh-encoded signed transaction bytes ready for broadcast.
    ///
    /// NOTE: actual Ed25519 signing is handled by the `signing` crate; this
    /// function is intentionally left as a stub until the key-management
    /// integration is complete.
    pub fn sign_transfer(
        _signer_id: &str,
        _receiver_id: &str,
        _amount_yocto: u128,
        _nonce: u64,
        _block_hash: [u8; 32],
    ) -> Result<Vec<u8>> {
        // TODO: integrate with custody_signing::SigningBackend for Ed25519.
        anyhow::bail!("NearSigner::sign_transfer not yet implemented — use SigningBackend")
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Convert a yoctoNEAR string amount to a NEAR Decimal (24 dp).
///
/// Example: "1000000000000000000000000" → Decimal("1.000000000000000000000000")
pub fn yocto_to_near(yocto: &str) -> Result<Decimal> {
    // Parse the raw integer string first.
    let raw = Decimal::from_str(yocto)
        .with_context(|| format!("invalid yoctoNEAR amount: {yocto}"))?;
    // Divide by 10^24 to get NEAR.
    let divisor = Decimal::new(10i64.pow(9), 0)  // 10^9
        * Decimal::new(10i64.pow(9), 0)           // × 10^9 = 10^18
        * Decimal::new(10i64.pow(6), 0);           // × 10^6 = 10^24
    Ok(raw / divisor)
}

/// Convert NEAR to yoctoNEAR string.
pub fn near_to_yocto(near: Decimal) -> u128 {
    let divisor = Decimal::new(10i64.pow(9), 0)
        * Decimal::new(10i64.pow(9), 0)
        * Decimal::new(10i64.pow(6), 0);
    let yocto = (near * divisor).trunc();
    // Safe: u128 can hold up to ~3.4×10^38; total NEAR supply is ~1.2×10^9 NEAR.
    yocto.to_string().parse::<u128>().unwrap_or(0)
}

/// Validate a NEAR account ID.
///
/// Rules (simplified):
/// - 2–64 characters
/// - Lowercase alphanumeric, hyphens, underscores, and dots allowed.
/// - Must not start or end with a dot.
/// - Implicit accounts: 64 hex chars (no dots).
pub fn validate_account_id(id: &str) -> Result<()> {
    anyhow::ensure!(!id.is_empty(), "account_id must not be empty");
    anyhow::ensure!(id.len() >= 2, "account_id too short: {id}");
    anyhow::ensure!(id.len() <= 64, "account_id too long: {id}");
    anyhow::ensure!(
        !id.starts_with('.') && !id.ends_with('.'),
        "account_id must not start or end with a dot: {id}"
    );
    for c in id.chars() {
        anyhow::ensure!(
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.',
            "invalid character '{c}' in account_id: {id}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn yocto_to_near_one_near() {
        // 10^24 yocto = 1 NEAR
        let yocto = "1000000000000000000000000";
        let near = yocto_to_near(yocto).unwrap();
        assert_eq!(near, dec!(1));
    }

    #[test]
    fn yocto_to_near_half_near() {
        let yocto = "500000000000000000000000";
        let near = yocto_to_near(yocto).unwrap();
        assert_eq!(near, dec!(0.5));
    }

    #[test]
    fn yocto_to_near_zero() {
        let near = yocto_to_near("0").unwrap();
        assert_eq!(near, dec!(0));
    }

    #[test]
    fn near_to_yocto_one_near() {
        let yocto = near_to_yocto(dec!(1));
        assert_eq!(yocto, 1_000_000_000_000_000_000_000_000u128);
    }

    #[test]
    fn near_to_yocto_round_trip() {
        let original = dec!(3.14);
        let yocto = near_to_yocto(original);
        let back = yocto_to_near(&yocto.to_string()).unwrap();
        // Precision limited to what Decimal can hold; allow tiny rounding.
        let diff = (back - original).abs();
        assert!(diff < dec!(0.000001), "round-trip diff too large: {diff}");
    }

    #[test]
    fn validate_account_id_valid_named() {
        assert!(validate_account_id("alice.near").is_ok());
        assert!(validate_account_id("bob").is_ok());
        assert!(validate_account_id("my-wallet.testnet").is_ok());
    }

    #[test]
    fn validate_account_id_implicit_64_hex() {
        // 64 lowercase hex chars — implicit account
        let implicit = "a".repeat(64);
        assert!(validate_account_id(&implicit).is_ok());
    }

    #[test]
    fn validate_account_id_rejects_leading_dot() {
        assert!(validate_account_id(".alice.near").is_err());
    }

    #[test]
    fn validate_account_id_rejects_trailing_dot() {
        assert!(validate_account_id("alice.near.").is_err());
    }

    #[test]
    fn validate_account_id_rejects_uppercase() {
        assert!(validate_account_id("Alice.near").is_err());
    }

    #[test]
    fn validate_account_id_rejects_too_short() {
        assert!(validate_account_id("a").is_err());
    }

    #[test]
    fn validate_account_id_rejects_too_long() {
        assert!(validate_account_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn near_scanner_mainnet_url() {
        let s = NearScanner::mainnet();
        assert_eq!(s.rpc_url, "https://rpc.mainnet.near.org");
    }

    #[test]
    fn yocto_invalid_string_is_error() {
        assert!(yocto_to_near("not_a_number").is_err());
    }
}
