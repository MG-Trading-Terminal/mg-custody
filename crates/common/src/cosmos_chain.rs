//! Cosmos SDK chain adapter types — IBC, staking, governance.
//!
//! Supports ATOM, OSMO, TIA (Celestia), DYDX and other Cosmos chains.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A Cosmos SDK chain definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosChainDef {
    pub chain_id: String,
    pub name: String,
    pub bech32_prefix: String,
    pub native_denom: String,
    pub native_decimals: u8,
    pub rpc_url: String,
    pub rest_url: String,
    pub coin_type: u32,
    pub gas_price: Decimal,
    pub gas_denom: String,
}

/// Known Cosmos chains.
pub fn cosmos_chains() -> Vec<CosmosChainDef> {
    use rust_decimal_macros::dec;
    vec![
        CosmosChainDef {
            chain_id: "cosmoshub-4".into(), name: "Cosmos Hub".into(),
            bech32_prefix: "cosmos".into(), native_denom: "uatom".into(),
            native_decimals: 6, rpc_url: "https://rpc.cosmos.network".into(),
            rest_url: "https://rest.cosmos.network".into(),
            coin_type: 118, gas_price: dec!(0.025), gas_denom: "uatom".into(),
        },
        CosmosChainDef {
            chain_id: "osmosis-1".into(), name: "Osmosis".into(),
            bech32_prefix: "osmo".into(), native_denom: "uosmo".into(),
            native_decimals: 6, rpc_url: "https://rpc.osmosis.zone".into(),
            rest_url: "https://rest.osmosis.zone".into(),
            coin_type: 118, gas_price: dec!(0.025), gas_denom: "uosmo".into(),
        },
        CosmosChainDef {
            chain_id: "celestia".into(), name: "Celestia".into(),
            bech32_prefix: "celestia".into(), native_denom: "utia".into(),
            native_decimals: 6, rpc_url: "https://rpc.celestia.network".into(),
            rest_url: "https://rest.celestia.network".into(),
            coin_type: 118, gas_price: dec!(0.002), gas_denom: "utia".into(),
        },
        CosmosChainDef {
            chain_id: "dydx-mainnet-1".into(), name: "dYdX".into(),
            bech32_prefix: "dydx".into(), native_denom: "adydx".into(),
            native_decimals: 18, rpc_url: "https://rpc.dydx.exchange".into(),
            rest_url: "https://rest.dydx.exchange".into(),
            coin_type: 118, gas_price: dec!(12500000000), gas_denom: "adydx".into(),
        },
        CosmosChainDef {
            chain_id: "injective-1".into(), name: "Injective".into(),
            bech32_prefix: "inj".into(), native_denom: "inj".into(),
            native_decimals: 18, rpc_url: "https://rpc.injective.network".into(),
            rest_url: "https://rest.injective.network".into(),
            coin_type: 60, gas_price: dec!(500000000), gas_denom: "inj".into(),
        },
    ]
}

/// Cosmos SDK transaction types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CosmosMsg {
    /// MsgSend — simple token transfer.
    Send {
        from_address: String,
        to_address: String,
        amount: Vec<CosmosCoin>,
    },
    /// MsgDelegate — stake tokens to a validator.
    Delegate {
        delegator_address: String,
        validator_address: String,
        amount: CosmosCoin,
    },
    /// MsgUndelegate — unstake from a validator.
    Undelegate {
        delegator_address: String,
        validator_address: String,
        amount: CosmosCoin,
    },
    /// MsgWithdrawDelegatorReward — claim staking rewards.
    WithdrawRewards {
        delegator_address: String,
        validator_address: String,
    },
    /// IBC MsgTransfer — cross-chain transfer.
    IbcTransfer {
        source_port: String,
        source_channel: String,
        sender: String,
        receiver: String,
        token: CosmosCoin,
        timeout_height: u64,
        timeout_timestamp: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosCoin {
    pub denom: String,
    pub amount: String,
}

/// Cosmos transaction fee.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosFee {
    pub amount: Vec<CosmosCoin>,
    pub gas_limit: u64,
}

/// Build a standard Cosmos send transaction body (JSON).
pub fn build_send_tx_body(
    from: &str, to: &str, denom: &str, amount: &str, memo: &str,
) -> serde_json::Value {
    serde_json::json!({
        "body": {
            "messages": [{
                "@type": "/cosmos.bank.v1beta1.MsgSend",
                "from_address": from,
                "to_address": to,
                "amount": [{"denom": denom, "amount": amount}]
            }],
            "memo": memo,
            "timeout_height": "0",
            "extension_options": [],
            "non_critical_extension_options": []
        }
    })
}

/// Build an IBC transfer message body.
pub fn build_ibc_transfer_body(
    sender: &str, receiver: &str, denom: &str, amount: &str,
    source_port: &str, source_channel: &str, timeout_height: u64,
) -> serde_json::Value {
    serde_json::json!({
        "body": {
            "messages": [{
                "@type": "/ibc.applications.transfer.v1.MsgTransfer",
                "source_port": source_port,
                "source_channel": source_channel,
                "token": {"denom": denom, "amount": amount},
                "sender": sender,
                "receiver": receiver,
                "timeout_height": {"revision_number": "0", "revision_height": timeout_height.to_string()},
                "timeout_timestamp": "0"
            }],
            "memo": "",
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosmos_chains_count() {
        assert!(cosmos_chains().len() >= 5);
    }

    #[test]
    fn cosmos_hub_config() {
        let chains = cosmos_chains();
        let hub = chains.iter().find(|c| c.chain_id == "cosmoshub-4").unwrap();
        assert_eq!(hub.bech32_prefix, "cosmos");
        assert_eq!(hub.native_denom, "uatom");
        assert_eq!(hub.native_decimals, 6);
    }

    #[test]
    fn celestia_config() {
        let chains = cosmos_chains();
        let tia = chains.iter().find(|c| c.name == "Celestia").unwrap();
        assert_eq!(tia.bech32_prefix, "celestia");
        assert_eq!(tia.native_denom, "utia");
    }

    #[test]
    fn cosmos_send_msg() {
        let msg = CosmosMsg::Send {
            from_address: "cosmos1abc".into(),
            to_address: "cosmos1def".into(),
            amount: vec![CosmosCoin { denom: "uatom".into(), amount: "1000000".into() }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("send"));
        assert!(json.contains("uatom"));
    }

    #[test]
    fn cosmos_delegate_msg() {
        let msg = CosmosMsg::Delegate {
            delegator_address: "cosmos1abc".into(),
            validator_address: "cosmosvaloper1xyz".into(),
            amount: CosmosCoin { denom: "uatom".into(), amount: "5000000".into() },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("delegate"));
        assert!(json.contains("cosmosvaloper"));
    }

    #[test]
    fn ibc_transfer_msg() {
        let msg = CosmosMsg::IbcTransfer {
            source_port: "transfer".into(),
            source_channel: "channel-0".into(),
            sender: "cosmos1abc".into(),
            receiver: "osmo1def".into(),
            token: CosmosCoin { denom: "uatom".into(), amount: "1000000".into() },
            timeout_height: 100000,
            timeout_timestamp: 0,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ibc_transfer"));
        assert!(json.contains("channel-0"));
    }

    #[test]
    fn build_send_tx() {
        let body = build_send_tx_body("cosmos1from", "cosmos1to", "uatom", "1000000", "test");
        assert!(body["body"]["messages"][0]["@type"].as_str().unwrap().contains("MsgSend"));
    }

    #[test]
    fn build_ibc_tx() {
        let body = build_ibc_transfer_body(
            "cosmos1from", "osmo1to", "uatom", "1000000",
            "transfer", "channel-0", 100000,
        );
        assert!(body["body"]["messages"][0]["@type"].as_str().unwrap().contains("MsgTransfer"));
    }

    #[test]
    fn cosmos_coin_serialization() {
        let coin = CosmosCoin { denom: "uatom".into(), amount: "1000000".into() };
        let json = serde_json::to_string(&coin).unwrap();
        assert!(json.contains("uatom"));
        assert!(json.contains("1000000"));
    }
}
