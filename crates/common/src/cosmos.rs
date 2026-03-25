//! Cosmos SDK chain adapter types.
//!
//! Types for interacting with Cosmos-based chains (ATOM, OSMO, etc.)
//! using Tendermint RPC and amino/protobuf encoding.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Cosmos chain configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosChainConfig {
    pub chain_id: String,       // e.g., "cosmoshub-4"
    pub rpc_url: String,        // e.g., "https://rpc.cosmos.network"
    pub rest_url: String,       // LCD REST API
    pub denom: String,          // e.g., "uatom"
    pub address_prefix: String, // e.g., "cosmos"
    pub decimals: u8,           // 6 for ATOM
    pub gas_price: String,      // e.g., "0.025uatom"
}

/// Known Cosmos chains.
pub fn cosmos_chains() -> Vec<CosmosChainConfig> {
    vec![
        CosmosChainConfig {
            chain_id: "cosmoshub-4".into(),
            rpc_url: "https://rpc.cosmos.network".into(),
            rest_url: "https://lcd.cosmos.network".into(),
            denom: "uatom".into(),
            address_prefix: "cosmos".into(),
            decimals: 6,
            gas_price: "0.025uatom".into(),
        },
        CosmosChainConfig {
            chain_id: "osmosis-1".into(),
            rpc_url: "https://rpc.osmosis.zone".into(),
            rest_url: "https://lcd.osmosis.zone".into(),
            denom: "uosmo".into(),
            address_prefix: "osmo".into(),
            decimals: 6,
            gas_price: "0.025uosmo".into(),
        },
        CosmosChainConfig {
            chain_id: "celestia".into(),
            rpc_url: "https://rpc.celestia.network".into(),
            rest_url: "https://lcd.celestia.network".into(),
            denom: "utia".into(),
            address_prefix: "celestia".into(),
            decimals: 6,
            gas_price: "0.002utia".into(),
        },
    ]
}

/// Cosmos transaction types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CosmosMsgType {
    Send,
    Delegate,
    Undelegate,
    Redelegate,
    ClaimRewards,
    Vote,
    IbcTransfer,
}

/// A Cosmos bank send message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosSendMsg {
    pub from_address: String,
    pub to_address: String,
    pub amount: Decimal,
    pub denom: String,
}

/// A Cosmos staking delegate message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosDelegateMsg {
    pub delegator_address: String,
    pub validator_address: String,
    pub amount: Decimal,
    pub denom: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmos_chains_list() {
        let chains = cosmos_chains();
        assert!(chains.len() >= 3);
        assert_eq!(chains[0].chain_id, "cosmoshub-4");
        assert_eq!(chains[0].denom, "uatom");
    }

    #[test]
    fn cosmos_msg_types() {
        let msg = CosmosMsgType::Delegate;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, "\"delegate\"");
    }

    #[test]
    fn cosmos_send_msg() {
        let msg = CosmosSendMsg {
            from_address: "cosmos1abc...".into(),
            to_address: "cosmos1def...".into(),
            amount: rust_decimal_macros::dec!(10.5),
            denom: "uatom".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("cosmos1abc"));
    }
}
