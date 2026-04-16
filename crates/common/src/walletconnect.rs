//! WalletConnect v2 types for DeFi connectivity.
//!
//! Implements the WalletConnect v2 protocol types for connecting
//! custody wallets to dApps. Uses the open WC v2 relay protocol.

use serde::{Deserialize, Serialize};

/// WalletConnect session proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcSessionProposal {
    pub id: u64,
    pub proposer: WcPeerMeta,
    pub required_namespaces: std::collections::HashMap<String, WcNamespace>,
}

/// WalletConnect peer metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcPeerMeta {
    pub name: String,
    pub description: String,
    pub url: String,
    pub icons: Vec<String>,
}

/// WalletConnect namespace (chains + methods + events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcNamespace {
    pub chains: Vec<String>,    // e.g., ["eip155:1", "eip155:137"]
    pub methods: Vec<String>,   // e.g., ["eth_sendTransaction", "personal_sign"]
    pub events: Vec<String>,    // e.g., ["chainChanged", "accountsChanged"]
}

/// WalletConnect JSON-RPC request from dApp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcRequest {
    pub id: u64,
    pub topic: String,
    pub method: String,
    pub params: serde_json::Value,
    pub chain_id: String,
}

/// Our response to a WC request (approve/reject).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WcResponse {
    Approve { result: serde_json::Value },
    Reject { error: WcError },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcError {
    pub code: i64,
    pub message: String,
}

/// Supported WalletConnect methods.
pub fn supported_wc_methods() -> Vec<&'static str> {
    vec![
        "eth_sendTransaction",
        "eth_signTransaction",
        "eth_sign",
        "personal_sign",
        "eth_signTypedData",
        "eth_signTypedData_v4",
    ]
}

/// DEX aggregator swap route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexSwapRoute {
    pub aggregator: String,   // "1inch", "0x", "jupiter"
    pub from_token: String,
    pub to_token: String,
    pub amount_in: String,
    pub amount_out_min: String,
    pub route: Vec<SwapHop>,
    pub gas_estimate: u64,
    pub price_impact_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapHop {
    pub dex: String,         // "uniswap_v3", "curve", "balancer"
    pub pool_address: String,
    pub token_in: String,
    pub token_out: String,
    pub fee_bps: u32,
}

/// Token allowance entry (for allowance scanner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAllowanceEntry {
    pub chain_id: u64,
    pub owner: String,
    pub token: String,
    pub token_symbol: String,
    pub spender: String,
    pub spender_name: Option<String>,
    pub allowance: String,    // raw uint256 string
    pub is_unlimited: bool,
    pub last_updated_block: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_methods() {
        let methods = supported_wc_methods();
        assert!(methods.contains(&"eth_sendTransaction"));
        assert!(methods.contains(&"personal_sign"));
        assert!(methods.len() >= 6);
    }

    #[test]
    fn wc_response_types() {
        let approve = WcResponse::Approve { result: serde_json::json!("0xhash") };
        let json = serde_json::to_string(&approve).unwrap();
        assert!(json.contains("approve"));

        let reject = WcResponse::Reject { error: WcError { code: 4001, message: "User rejected".into() } };
        let json = serde_json::to_string(&reject).unwrap();
        assert!(json.contains("reject"));
    }

    #[test]
    fn dex_swap_route() {
        let route = DexSwapRoute {
            aggregator: "1inch".into(), from_token: "ETH".into(), to_token: "USDC".into(),
            amount_in: "1000000000000000000".into(), amount_out_min: "3400000000".into(),
            route: vec![SwapHop { dex: "uniswap_v3".into(), pool_address: "0x...".into(),
                token_in: "ETH".into(), token_out: "USDC".into(), fee_bps: 30 }],
            gas_estimate: 150000, price_impact_bps: 5,
        };
        assert_eq!(route.route.len(), 1);
        assert_eq!(route.price_impact_bps, 5);
    }
}
