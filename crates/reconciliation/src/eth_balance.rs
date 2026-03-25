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
        // Convert wei to ETH (18 decimals)
        Ok(Decimal::new(wei as i64, 18))
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
        Ok(Decimal::new(balance as i64, decimals as u32))
    }

    pub fn chain(&self) -> Chain { Chain::Ethereum }
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
}
