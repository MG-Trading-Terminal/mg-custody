//! Polkadot/Kusama chain types.
//! Uses sr25519 signing (Schnorrkel) — different from both secp256k1 and Ed25519.

use serde::{Deserialize, Serialize};

/// Polkadot chain config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolkadotChainConfig {
    pub chain: String,          // "polkadot", "kusama"
    pub rpc_url: String,
    pub address_prefix: u16,    // SS58: 0=Polkadot, 2=Kusama
    pub decimals: u8,           // 10 for DOT, 12 for KSM
    pub denom: String,
}

pub fn polkadot_chains() -> Vec<PolkadotChainConfig> {
    vec![
        PolkadotChainConfig {
            chain: "polkadot".into(), rpc_url: "wss://rpc.polkadot.io".into(),
            address_prefix: 0, decimals: 10, denom: "DOT".into(),
        },
        PolkadotChainConfig {
            chain: "kusama".into(), rpc_url: "wss://kusama-rpc.polkadot.io".into(),
            address_prefix: 2, decimals: 12, denom: "KSM".into(),
        },
    ]
}

/// Polkadot transaction types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolkadotTxType {
    Transfer,
    Bond,
    Unbond,
    Nominate,
    ClaimRewards,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polkadot_chains_exist() {
        let chains = polkadot_chains();
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0].chain, "polkadot");
        assert_eq!(chains[0].decimals, 10);
        assert_eq!(chains[1].chain, "kusama");
    }

    #[test]
    fn tx_types_serialize() {
        assert_eq!(serde_json::to_string(&PolkadotTxType::Nominate).unwrap(), "\"nominate\"");
    }
}
