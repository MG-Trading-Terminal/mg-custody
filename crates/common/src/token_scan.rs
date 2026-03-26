//! Token auto-scan — detect tokens held by an address via RPC.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

/// Result of scanning an address for token balances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenScanResult {
    pub address: String,
    pub chain: Chain,
    pub native_balance: Decimal,
    pub tokens: Vec<DetectedToken>,
    pub scanned_at: DateTime<Utc>,
}

/// A token detected in a wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedToken {
    pub contract_address: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub decimals: u8,
    pub balance_raw: String,
    pub balance_human: Decimal,
}

/// ERC-20 token detection methods.
pub mod evm {
    /// `eth_getBalance` RPC method for native balance.
    pub fn eth_get_balance_request(address: &str, block: &str) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "eth_getBalance",
            "params": [address, block]
        })
    }

    /// ERC-20 token list scan via `eth_call` with balanceOf.
    pub fn build_balance_check_calls(address: &[u8; 20], token_contracts: &[&str]) -> Vec<serde_json::Value> {
        token_contracts.iter().enumerate().map(|(i, contract)| {
            let mut calldata = Vec::with_capacity(36);
            calldata.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]); // balanceOf
            let mut slot = [0u8; 32];
            slot[12..].copy_from_slice(address);
            calldata.extend_from_slice(&slot);

            serde_json::json!({
                "jsonrpc": "2.0", "id": i + 1,
                "method": "eth_call",
                "params": [{"to": contract, "data": format!("0x{}", hex::encode(&calldata))}, "latest"]
            })
        }).collect()
    }
}

/// Solana token account detection.
pub mod solana {
    /// `getTokenAccountsByOwner` RPC method.
    pub fn get_token_accounts_request(owner: &str) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getTokenAccountsByOwner",
            "params": [
                owner,
                {"programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"},
                {"encoding": "jsonParsed"}
            ]
        })
    }

    /// `getBalance` for native SOL.
    pub fn get_sol_balance_request(address: &str) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "getBalance",
            "params": [address, {"commitment": "finalized"}]
        })
    }
}

/// Tron token detection.
pub mod tron {
    /// TRC-20 token balance via Tron HTTP API.
    pub fn trc20_balance_url(address: &str) -> String {
        format!("https://api.trongrid.io/v1/accounts/{address}/tokens")
    }

    /// TRX native balance.
    pub fn trx_balance_url(address: &str) -> String {
        format!("https://api.trongrid.io/v1/accounts/{address}")
    }
}

/// Top ERC-20 contracts to scan for (by market cap).
pub fn top_erc20_contracts() -> Vec<(&'static str, &'static str, u8)> {
    vec![
        ("0xdAC17F958D2ee523a2206206994597C13D831ec7", "USDT", 6),
        ("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", "USDC", 6),
        ("0x6B175474E89094C44Da98b954EedeAC495271d0F", "DAI", 18),
        ("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", "WBTC", 8),
        ("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", "WETH", 18),
        ("0x514910771AF9Ca656af840dff83E8264EcF986CA", "LINK", 18),
        ("0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984", "UNI", 18),
        ("0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84", "stETH", 18),
        ("0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9", "AAVE", 18),
        ("0x95aD61b0a150d79219dCF64E1E6Cc01f0B64C4cE", "SHIB", 18),
        ("0x7D1AfA7B718fb893dB30A3aBc0Cfc608AaCfeBB0", "MATIC", 18),
        ("0xB50721BCf8d664c30412Cfbc6cf7a15145234ad1", "ARB", 18),
        ("0x5A98FcBEA516Cf06857215779Fd812CA3beF1B32", "LDO", 18),
        ("0xD533a949740bb3306d119CC777fa900bA034cd52", "CRV", 18),
        ("0xae78736Cd615f374D3085123A210448E74Fc6393", "rETH", 18),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn token_scan_result_structure() {
        let result = TokenScanResult {
            address: "0x123".into(), chain: Chain::Ethereum,
            native_balance: dec!(1.5),
            tokens: vec![DetectedToken {
                contract_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".into(),
                symbol: Some("USDT".into()), name: Some("Tether USD".into()),
                decimals: 6, balance_raw: "1000000".into(),
                balance_human: dec!(1.0),
            }],
            scanned_at: Utc::now(),
        };
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(result.tokens[0].decimals, 6);
    }

    #[test]
    fn eth_get_balance_request() {
        let req = evm::eth_get_balance_request("0x123", "latest");
        assert_eq!(req["method"], "eth_getBalance");
    }

    #[test]
    fn balance_check_batch() {
        let address = [0xAA; 20];
        let contracts = &["0xdAC17F958D2ee523a2206206994597C13D831ec7", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"];
        let calls = evm::build_balance_check_calls(&address, contracts);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["method"], "eth_call");
        assert_eq!(calls[1]["id"], 2);
    }

    #[test]
    fn solana_token_accounts_request() {
        let req = solana::get_token_accounts_request("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM");
        assert_eq!(req["method"], "getTokenAccountsByOwner");
    }

    #[test]
    fn sol_balance_request() {
        let req = solana::get_sol_balance_request("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM");
        assert_eq!(req["method"], "getBalance");
    }

    #[test]
    fn tron_urls() {
        let url = tron::trc20_balance_url("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t");
        assert!(url.contains("trongrid.io"));
        assert!(url.contains("tokens"));
    }

    #[test]
    fn top_erc20_list() {
        let tokens = top_erc20_contracts();
        assert!(tokens.len() >= 15);
        // USDT should be first
        assert_eq!(tokens[0].1, "USDT");
        assert_eq!(tokens[0].2, 6);
    }

    #[test]
    fn top_erc20_all_valid_addresses() {
        for (addr, _, _) in top_erc20_contracts() {
            assert!(addr.starts_with("0x"), "Invalid address: {addr}");
            assert_eq!(addr.len(), 42);
        }
    }
}
