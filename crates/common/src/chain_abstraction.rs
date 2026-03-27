//! Chain abstraction — unified interface across all chains.
//!
//! Provides a single API surface regardless of underlying chain differences.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use crate::Chain;

/// Chain-agnostic transfer request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedTransfer {
    pub from_chain: Chain,
    pub to_chain: Chain,
    pub from_address: String,
    pub to_address: String,
    pub asset: String,
    pub amount: Decimal,
    pub is_cross_chain: bool,
}

impl UnifiedTransfer {
    pub fn new(from: Chain, to: Chain, from_addr: &str, to_addr: &str, asset: &str, amount: Decimal) -> Self {
        Self {
            from_chain: from, to_chain: to,
            from_address: from_addr.into(), to_address: to_addr.into(),
            asset: asset.into(), amount,
            is_cross_chain: from != to,
        }
    }
}

/// Chain capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCapabilities {
    pub chain: Chain,
    pub supports_smart_contracts: bool,
    pub supports_tokens: bool,
    pub supports_nfts: bool,
    pub supports_staking: bool,
    pub supports_defi: bool,
    pub supports_name_resolution: bool,
    pub tx_finality_type: FinalityType,
    pub address_model: AddressModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalityType { Probabilistic, Deterministic, Instant }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressModel { Account, Utxo }

pub fn chain_capabilities(chain: Chain) -> ChainCapabilities {
    match chain {
        Chain::Ethereum => ChainCapabilities {
            chain, supports_smart_contracts: true, supports_tokens: true,
            supports_nfts: true, supports_staking: true, supports_defi: true,
            supports_name_resolution: true, tx_finality_type: FinalityType::Deterministic,
            address_model: AddressModel::Account,
        },
        Chain::Bitcoin => ChainCapabilities {
            chain, supports_smart_contracts: false, supports_tokens: false,
            supports_nfts: false, supports_staking: false, supports_defi: false,
            supports_name_resolution: false, tx_finality_type: FinalityType::Probabilistic,
            address_model: AddressModel::Utxo,
        },
        Chain::Tron => ChainCapabilities {
            chain, supports_smart_contracts: true, supports_tokens: true,
            supports_nfts: true, supports_staking: true, supports_defi: true,
            supports_name_resolution: false, tx_finality_type: FinalityType::Deterministic,
            address_model: AddressModel::Account,
        },
        Chain::Solana => ChainCapabilities {
            chain, supports_smart_contracts: true, supports_tokens: true,
            supports_nfts: true, supports_staking: true, supports_defi: true,
            supports_name_resolution: true, tx_finality_type: FinalityType::Deterministic,
            address_model: AddressModel::Account,
        },
    }
}

/// Native token for each chain.
pub fn native_token(chain: Chain) -> (&'static str, &'static str, u8) {
    match chain {
        Chain::Bitcoin => ("BTC", "Bitcoin", 8),
        Chain::Ethereum => ("ETH", "Ether", 18),
        Chain::Tron => ("TRX", "TRON", 6),
        Chain::Solana => ("SOL", "Solana", 9),
    }
}

/// Minimum transaction amounts per chain.
pub fn min_transfer_amount(chain: Chain) -> Decimal {
    use rust_decimal_macros::dec;
    match chain {
        Chain::Bitcoin => dec!(0.00000546),  // dust limit
        Chain::Ethereum => dec!(0.000000000000000001), // 1 wei
        Chain::Tron => dec!(0.000001),
        Chain::Solana => dec!(0.000000001), // 1 lamport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn unified_transfer_same_chain() {
        let t = UnifiedTransfer::new(Chain::Ethereum, Chain::Ethereum, "0x1", "0x2", "ETH", dec!(1));
        assert!(!t.is_cross_chain);
    }

    #[test]
    fn unified_transfer_cross_chain() {
        let t = UnifiedTransfer::new(Chain::Ethereum, Chain::Solana, "0x1", "sol1", "USDC", dec!(100));
        assert!(t.is_cross_chain);
    }

    #[test]
    fn bitcoin_utxo_model() {
        let cap = chain_capabilities(Chain::Bitcoin);
        assert_eq!(cap.address_model, AddressModel::Utxo);
        assert!(!cap.supports_smart_contracts);
        assert_eq!(cap.tx_finality_type, FinalityType::Probabilistic);
    }

    #[test]
    fn ethereum_account_model() {
        let cap = chain_capabilities(Chain::Ethereum);
        assert_eq!(cap.address_model, AddressModel::Account);
        assert!(cap.supports_defi);
        assert!(cap.supports_name_resolution); // ENS
    }

    #[test]
    fn solana_capabilities() {
        let cap = chain_capabilities(Chain::Solana);
        assert!(cap.supports_nfts);
        assert!(cap.supports_staking);
        assert_eq!(cap.tx_finality_type, FinalityType::Deterministic);
    }

    #[test]
    fn native_tokens() {
        let (sym, _, dec) = native_token(Chain::Ethereum);
        assert_eq!(sym, "ETH");
        assert_eq!(dec, 18);
        let (sym, _, dec) = native_token(Chain::Bitcoin);
        assert_eq!(sym, "BTC");
        assert_eq!(dec, 8);
    }

    #[test]
    fn min_amounts() {
        assert!(min_transfer_amount(Chain::Bitcoin) > Decimal::ZERO);
        assert!(min_transfer_amount(Chain::Ethereum) > Decimal::ZERO);
    }

    #[test]
    fn finality_serialization() {
        assert_eq!(serde_json::to_string(&FinalityType::Probabilistic).unwrap(), r#""probabilistic""#);
    }

    #[test]
    fn address_model_serialization() {
        assert_eq!(serde_json::to_string(&AddressModel::Utxo).unwrap(), r#""utxo""#);
    }
}
