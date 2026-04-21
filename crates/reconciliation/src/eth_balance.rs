//! Ethereum on-chain balance provider.

use anyhow::Result;
use custody_common::Chain;
use rust_decimal::Decimal;

/// Ethereum balance provider via JSON-RPC.
pub struct EthBalanceProvider {
    rpc_url: String,
    client: reqwest::Client,
}

impl EthBalanceProvider {
    pub fn new(rpc_url: &str) -> Self {
        Self { rpc_url: rpc_url.to_string(), client: reqwest::Client::new() }
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let body = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp: serde_json::Value = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send().await?
            .json().await?;
        if let Some(err) = resp.get("error").filter(|e| !e.is_null()) {
            anyhow::bail!("ETH RPC error: {err}");
        }
        Ok(resp["result"].clone())
    }

    /// Get native ETH balance for an address (in ETH, not wei).
    pub async fn native_balance(&self, address: &str) -> Result<Decimal> {
        let result = self.rpc("eth_getBalance", serde_json::json!([address, "latest"])).await?;
        let hex_str = result.as_str().unwrap_or("0x0");
        let wei = u128::from_str_radix(hex_str.trim_start_matches("0x"), 16).unwrap_or(0);
        u128_wei_to_decimal(wei, 18)
    }

    /// Get ERC-20 token balance for an address.
    pub async fn token_balance(&self, address: &str, contract: &str, decimals: u8) -> Result<Decimal> {
        // balanceOf(address) = 0x70a08231 + padded address
        let padded_addr = format!("000000000000000000000000{}", address.trim_start_matches("0x"));
        let data = format!("0x70a08231{padded_addr}");

        let result = self.rpc("eth_call", serde_json::json!([
            {"to": contract, "data": data},
            "latest"
        ])).await?;

        let hex_str = result.as_str().unwrap_or("0x0");
        let balance = u128::from_str_radix(hex_str.trim_start_matches("0x"), 16).unwrap_or(0);
        u128_wei_to_decimal(balance, decimals)
    }

    pub fn chain(&self) -> Chain { Chain::Ethereum }
}

/// Convert a raw `u128` wei/atomic value into `Decimal` safely.
///
/// The previous implementation cast through `i64` via `Decimal::new(wei as i64, 18)`.
/// `as i64` silently truncates values above 2^63 — roughly 9.22M ETH
/// in wei, or well under any serious USDT holding. A hot wallet
/// reconciliation against a big whale account would return a
/// wrapped-around nonsense value that the auditor compared to
/// ledger truth and flagged as a discrepancy (or worse, didn't
/// flag because the ledger had the same-wrong number).
///
/// `Decimal` handles up to 28-29 significant digits, which easily
/// covers any realistic token balance. Going through the string
/// form is the safe cross-precision path.
fn u128_wei_to_decimal(atomic: u128, decimals: u8) -> Result<Decimal> {
    let s = atomic.to_string();
    let mut d: Decimal = s
        .parse()
        .map_err(|e| anyhow::anyhow!("wei {atomic} not representable as Decimal: {e}"))?;
    d.set_scale(decimals as u32)
        .map_err(|e| anyhow::anyhow!("scale {decimals} invalid: {e}"))?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wei_to_eth_conversion() {
        // 1 ETH = 10^18 wei
        let eth = Decimal::new(1_000_000_000_000_000_000i64, 18);
        assert_eq!(eth, Decimal::new(1, 0));
    }

    #[test]
    fn u128_wei_handles_huge_balance_without_truncating() {
        // 10_000_000 ETH in wei — exceeds i64::MAX (~9.22M ETH).
        // The old `wei as i64` would overflow to a negative value.
        let wei: u128 = 10_000_000_u128 * 1_000_000_000_000_000_000u128;
        let decimal = u128_wei_to_decimal(wei, 18).unwrap();
        assert_eq!(decimal.to_string(), "10000000.000000000000000000");
    }

    #[test]
    fn u128_wei_handles_i64_boundary() {
        // Exactly at i64::MAX wei (~9.22 ETH) — make sure we don't
        // off-by-one anywhere.
        let wei = i64::MAX as u128;
        let decimal = u128_wei_to_decimal(wei, 18).unwrap();
        // 9223372036854775807 wei = 9.223372036854775807 ETH
        assert_eq!(decimal.to_string(), "9.223372036854775807");
    }

    #[test]
    fn u128_wei_usdt_6_decimals() {
        // A billion USDT in 6-decimal atomic units.
        let atomic: u128 = 1_000_000_000_u128 * 1_000_000u128;
        let decimal = u128_wei_to_decimal(atomic, 6).unwrap();
        assert_eq!(decimal.to_string(), "1000000000.000000");
    }
}
