//! MEV protection — Flashbots, private transactions, sandwich attack prevention.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

/// MEV protection strategy for a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MevStrategy {
    /// No protection — standard mempool.
    None,
    /// Flashbots Protect — private transaction pool.
    FlashbotsProtect,
    /// MEV Blocker by CoW Protocol.
    MevBlocker,
    /// Direct to block builder.
    DirectBuilder,
    /// Solana: Jito block engine for tips.
    JitoBlockEngine,
}

/// Flashbots RPC endpoints.
pub mod flashbots {
    pub const PROTECT_RPC: &str = "https://rpc.flashbots.net";
    pub const FAST_RPC: &str = "https://rpc.flashbots.net/fast";
    pub const BUILDER_RELAY: &str = "https://relay.flashbots.net";

    /// Build Flashbots bundle request.
    pub fn build_bundle_request(
        signed_txs: &[String],
        target_block: u64,
    ) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "method": "eth_sendBundle",
            "params": [{
                "txs": signed_txs,
                "blockNumber": format!("0x{:x}", target_block),
            }]
        })
    }
}

/// MEV Blocker (CoW Protocol).
pub mod mev_blocker {
    pub const RPC: &str = "https://rpc.mevblocker.io";
    pub const FAST_RPC: &str = "https://rpc.mevblocker.io/fast";
}

/// Jito MEV for Solana.
pub mod jito {
    pub const BLOCK_ENGINE_URL: &str = "https://mainnet.block-engine.jito.wtf";
    pub const TIP_ACCOUNT: &str = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5";

    /// Minimum tip for Jito bundle (in lamports).
    pub const MIN_TIP_LAMPORTS: u64 = 10_000; // 0.00001 SOL
}

/// Analyze a transaction for MEV risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MevRiskAnalysis {
    pub chain: Chain,
    pub tx_type: String,
    pub risk_level: MevRiskLevel,
    pub sandwich_risk: bool,
    pub frontrun_risk: bool,
    pub recommended_strategy: MevStrategy,
    pub estimated_mev_cost: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MevRiskLevel {
    None,
    Low,
    Medium,
    High,
}

/// Assess MEV risk for a transaction.
pub fn assess_mev_risk(
    chain: Chain,
    tx_type: &str,
    value_usd: Decimal,
) -> MevRiskAnalysis {
    use rust_decimal_macros::dec;

    let (risk, sandwich, frontrun, strategy) = match (chain, tx_type) {
        (Chain::Ethereum, "swap") if value_usd > dec!(100000) => {
            (MevRiskLevel::High, true, true, MevStrategy::FlashbotsProtect)
        }
        (Chain::Ethereum, "swap") if value_usd > dec!(10000) => {
            (MevRiskLevel::Medium, true, false, MevStrategy::FlashbotsProtect)
        }
        (Chain::Ethereum, "swap") => {
            (MevRiskLevel::Low, false, false, MevStrategy::MevBlocker)
        }
        (Chain::Solana, "swap") if value_usd > dec!(50000) => {
            (MevRiskLevel::High, true, true, MevStrategy::JitoBlockEngine)
        }
        (Chain::Solana, "swap") => {
            (MevRiskLevel::Low, false, false, MevStrategy::JitoBlockEngine)
        }
        (_, "transfer") => {
            (MevRiskLevel::None, false, false, MevStrategy::None)
        }
        _ => (MevRiskLevel::Low, false, false, MevStrategy::None),
    };

    MevRiskAnalysis {
        chain, tx_type: tx_type.into(),
        risk_level: risk, sandwich_risk: sandwich,
        frontrun_risk: frontrun, recommended_strategy: strategy,
        estimated_mev_cost: if risk >= MevRiskLevel::Medium { Some(value_usd * dec!(0.003)) } else { None },
    }
}

/// Solana compute budget instructions.
pub mod compute_budget {
    /// Compute Budget program ID.
    pub const PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";

    /// Build SetComputeUnitLimit instruction data.
    /// Discriminator: 0x02
    pub fn set_compute_unit_limit(units: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity(5);
        data.push(0x02);
        data.extend_from_slice(&units.to_le_bytes());
        data
    }

    /// Build SetComputeUnitPrice instruction data (priority fee).
    /// Discriminator: 0x03
    pub fn set_compute_unit_price(micro_lamports: u64) -> Vec<u8> {
        let mut data = Vec::with_capacity(9);
        data.push(0x03);
        data.extend_from_slice(&micro_lamports.to_le_bytes());
        data
    }

    /// Recommended compute units for common operations.
    pub const UNITS_TRANSFER: u32 = 200_000;
    pub const UNITS_TOKEN_TRANSFER: u32 = 300_000;
    pub const UNITS_SWAP: u32 = 600_000;
    pub const UNITS_STAKE: u32 = 400_000;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn mev_risk_large_swap() {
        let risk = assess_mev_risk(Chain::Ethereum, "swap", dec!(500000));
        assert_eq!(risk.risk_level, MevRiskLevel::High);
        assert!(risk.sandwich_risk);
        assert_eq!(risk.recommended_strategy, MevStrategy::FlashbotsProtect);
        assert!(risk.estimated_mev_cost.is_some());
    }

    #[test]
    fn mev_risk_small_transfer() {
        let risk = assess_mev_risk(Chain::Ethereum, "transfer", dec!(100));
        assert_eq!(risk.risk_level, MevRiskLevel::None);
        assert!(!risk.sandwich_risk);
    }

    #[test]
    fn mev_risk_solana_swap() {
        let risk = assess_mev_risk(Chain::Solana, "swap", dec!(100000));
        assert_eq!(risk.risk_level, MevRiskLevel::High);
        assert_eq!(risk.recommended_strategy, MevStrategy::JitoBlockEngine);
    }

    #[test]
    fn flashbots_bundle_request() {
        let req = flashbots::build_bundle_request(
            &["0xsigned_tx_1".into()], 19500000,
        );
        assert_eq!(req["method"], "eth_sendBundle");
        assert!(req["params"][0]["blockNumber"].as_str().unwrap().starts_with("0x"));
    }

    #[test]
    fn compute_unit_limit_encoding() {
        let data = compute_budget::set_compute_unit_limit(600_000);
        assert_eq!(data[0], 0x02);
        assert_eq!(data.len(), 5);
        let decoded = u32::from_le_bytes(data[1..5].try_into().unwrap());
        assert_eq!(decoded, 600_000);
    }

    #[test]
    fn compute_unit_price_encoding() {
        let data = compute_budget::set_compute_unit_price(1_000_000);
        assert_eq!(data[0], 0x03);
        assert_eq!(data.len(), 9);
        let decoded = u64::from_le_bytes(data[1..9].try_into().unwrap());
        assert_eq!(decoded, 1_000_000);
    }

    #[test]
    fn mev_strategy_serialization() {
        assert_eq!(serde_json::to_string(&MevStrategy::FlashbotsProtect).unwrap(), r#""flashbots_protect""#);
        assert_eq!(serde_json::to_string(&MevStrategy::JitoBlockEngine).unwrap(), r#""jito_block_engine""#);
    }

    #[test]
    fn mev_risk_ordering() {
        assert!(MevRiskLevel::None < MevRiskLevel::Low);
        assert!(MevRiskLevel::Low < MevRiskLevel::Medium);
        assert!(MevRiskLevel::Medium < MevRiskLevel::High);
    }
}
