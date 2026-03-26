//! Polkadot/Substrate chain adapter types — DOT, KSM, parachains.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A Polkadot/Substrate chain definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateChainDef {
    pub name: String,
    pub chain_type: SubstrateChainType,
    pub ss58_prefix: u16,
    pub native_token: String,
    pub native_decimals: u8,
    pub rpc_url: String,
    pub existential_deposit: Decimal,
    pub para_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateChainType {
    RelayChain,
    Parachain,
    SoloChain,
}

/// Known Polkadot ecosystem chains.
pub fn polkadot_chains() -> Vec<SubstrateChainDef> {
    use rust_decimal_macros::dec;
    vec![
        SubstrateChainDef {
            name: "Polkadot".into(), chain_type: SubstrateChainType::RelayChain,
            ss58_prefix: 0, native_token: "DOT".into(), native_decimals: 10,
            rpc_url: "wss://rpc.polkadot.io".into(),
            existential_deposit: dec!(1.0), para_id: None,
        },
        SubstrateChainDef {
            name: "Kusama".into(), chain_type: SubstrateChainType::RelayChain,
            ss58_prefix: 2, native_token: "KSM".into(), native_decimals: 12,
            rpc_url: "wss://kusama-rpc.polkadot.io".into(),
            existential_deposit: dec!(0.000333333333), para_id: None,
        },
        SubstrateChainDef {
            name: "Moonbeam".into(), chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 1284, native_token: "GLMR".into(), native_decimals: 18,
            rpc_url: "wss://wss.api.moonbeam.network".into(),
            existential_deposit: dec!(0.01), para_id: Some(2004),
        },
        SubstrateChainDef {
            name: "Acala".into(), chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 10, native_token: "ACA".into(), native_decimals: 12,
            rpc_url: "wss://acala-rpc.aca-api.network".into(),
            existential_deposit: dec!(0.1), para_id: Some(2000),
        },
        SubstrateChainDef {
            name: "Astar".into(), chain_type: SubstrateChainType::Parachain,
            ss58_prefix: 5, native_token: "ASTR".into(), native_decimals: 18,
            rpc_url: "wss://rpc.astar.network".into(),
            existential_deposit: dec!(0.01), para_id: Some(2006),
        },
    ]
}

/// Substrate transaction call types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "pallet", rename_all = "snake_case")]
pub enum SubstrateCall {
    /// balances.transferKeepAlive
    BalancesTransfer { dest: String, value: String },
    /// staking.bond
    StakingBond { value: String, payee: StakingPayee },
    /// staking.nominate
    StakingNominate { targets: Vec<String> },
    /// staking.unbond
    StakingUnbond { value: String },
    /// staking.withdrawUnbonded
    StakingWithdrawUnbonded { num_slashing_spans: u32 },
    /// xcm.limitedReserveTransferAssets (cross-chain)
    XcmTransfer { dest_chain: u32, beneficiary: String, asset: String, amount: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingPayee {
    Staked,
    Stash,
    Controller,
}

/// Encode SS58 address prefix check.
pub fn validate_ss58_prefix(address: &str, expected_prefix: u16) -> bool {
    // SS58 addresses start with specific characters based on prefix
    // Prefix 0 (Polkadot): starts with '1'
    // Prefix 2 (Kusama): starts with capital letters
    // This is a simplified check
    match expected_prefix {
        0 => address.starts_with('1') && address.len() >= 46 && address.len() <= 48,
        2 => address.len() >= 46 && address.len() <= 48,
        _ => address.len() >= 40, // generic substrate address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polkadot_chains_count() {
        assert!(polkadot_chains().len() >= 5);
    }

    #[test]
    fn polkadot_relay_config() {
        let chains = polkadot_chains();
        let dot = chains.iter().find(|c| c.name == "Polkadot").unwrap();
        assert_eq!(dot.ss58_prefix, 0);
        assert_eq!(dot.native_token, "DOT");
        assert_eq!(dot.native_decimals, 10);
        assert_eq!(dot.chain_type, SubstrateChainType::RelayChain);
    }

    #[test]
    fn kusama_config() {
        let chains = polkadot_chains();
        let ksm = chains.iter().find(|c| c.name == "Kusama").unwrap();
        assert_eq!(ksm.ss58_prefix, 2);
        assert_eq!(ksm.native_decimals, 12);
    }

    #[test]
    fn moonbeam_is_parachain() {
        let chains = polkadot_chains();
        let moon = chains.iter().find(|c| c.name == "Moonbeam").unwrap();
        assert_eq!(moon.chain_type, SubstrateChainType::Parachain);
        assert_eq!(moon.para_id, Some(2004));
    }

    #[test]
    fn substrate_transfer_call() {
        let call = SubstrateCall::BalancesTransfer {
            dest: "1abc...".into(), value: "10000000000".into(),
        };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("balances_transfer"));
    }

    #[test]
    fn substrate_staking_calls() {
        let bond = SubstrateCall::StakingBond {
            value: "100000000000".into(), payee: StakingPayee::Staked,
        };
        let json = serde_json::to_string(&bond).unwrap();
        assert!(json.contains("staking_bond"));

        let nominate = SubstrateCall::StakingNominate {
            targets: vec!["validator1".into(), "validator2".into()],
        };
        let json = serde_json::to_string(&nominate).unwrap();
        assert!(json.contains("staking_nominate"));
    }

    #[test]
    fn xcm_transfer_call() {
        let xcm = SubstrateCall::XcmTransfer {
            dest_chain: 2004, beneficiary: "0xabc".into(),
            asset: "DOT".into(), amount: "10000000000".into(),
        };
        let json = serde_json::to_string(&xcm).unwrap();
        assert!(json.contains("xcm_transfer"));
    }

    #[test]
    fn ss58_validation_polkadot() {
        // Polkadot addresses start with '1', 47-48 chars
        assert!(validate_ss58_prefix("1FRMM8PEiWXYax7rpS6X4XZX1aAAxSWx1CrKTyrVYhV24fg", 0));
        assert!(!validate_ss58_prefix("short", 0));
    }

    #[test]
    fn staking_payee_serialization() {
        assert_eq!(serde_json::to_string(&StakingPayee::Staked).unwrap(), r#""staked""#);
    }
}
