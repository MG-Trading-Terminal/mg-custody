//! Gas Station V2 — per-vault auto-fueling, multi-chain, EVM gas management.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::Chain;

/// Gas station configuration per vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultGasConfig {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub enabled: bool,
    pub gas_threshold: Decimal,
    pub gas_cap: Decimal,
    pub max_gas_price: Decimal,
    pub funding_vault_id: Uuid,
}

/// Gas station funding event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasFundingEvent {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub chain: Chain,
    pub address: String,
    pub amount: Decimal,
    pub tx_hash: Option<String>,
    pub status: FundingStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FundingStatus { Pending, Broadcast, Confirmed, Failed }

/// EVM gas price tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmGasTracker {
    pub chain: Chain,
    pub chain_id: u64,
    pub base_fee_gwei: Decimal,
    pub priority_fee_gwei: Decimal,
    pub blob_base_fee_gwei: Option<Decimal>,
    pub gas_used_ratio: f32,
    pub pending_tx_count: u32,
    pub last_block: u64,
    pub updated_at: DateTime<Utc>,
}

/// Compliance screening providers (beyond Chainalysis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceProvider {
    Chainalysis,
    Elliptic,
    Crystal,
    TrmLabs,
    Scorechain,
    Merkle,
    Internal,
}

/// Multi-provider screening result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiProviderScreening {
    pub address: String,
    pub chain: Chain,
    pub results: Vec<ProviderResult>,
    pub aggregated_risk: AggregatedRisk,
    pub screened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResult {
    pub provider: ComplianceProvider,
    pub risk_score: f32,
    pub risk_level: String,
    pub flags: Vec<String>,
    pub latency_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedRisk {
    pub level: String,
    pub score: f32,
    pub method: AggregationMethod,
    pub providers_consulted: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationMethod { WorstOf, Average, Majority }

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn vault_gas_config() {
        let config = VaultGasConfig {
            vault_id: Uuid::nil(), chain: Chain::Ethereum,
            enabled: true, gas_threshold: dec!(0.01), gas_cap: dec!(0.05),
            max_gas_price: dec!(200), funding_vault_id: Uuid::nil(),
        };
        assert!(config.enabled);
    }

    #[test]
    fn compliance_provider_serialization() {
        assert_eq!(serde_json::to_string(&ComplianceProvider::Elliptic).unwrap(), r#""elliptic""#);
        assert_eq!(serde_json::to_string(&ComplianceProvider::TrmLabs).unwrap(), r#""trm_labs""#);
    }

    #[test]
    fn aggregation_methods() {
        assert_eq!(serde_json::to_string(&AggregationMethod::WorstOf).unwrap(), r#""worst_of""#);
    }

    #[test]
    fn funding_status() {
        assert_eq!(serde_json::to_string(&FundingStatus::Broadcast).unwrap(), r#""broadcast""#);
    }

    #[test]
    fn evm_gas_tracker() {
        let tracker = EvmGasTracker {
            chain: Chain::Ethereum, chain_id: 1,
            base_fee_gwei: dec!(25), priority_fee_gwei: dec!(2),
            blob_base_fee_gwei: Some(dec!(0.001)),
            gas_used_ratio: 0.75, pending_tx_count: 15000,
            last_block: 19500000, updated_at: Utc::now(),
        };
        assert!(tracker.blob_base_fee_gwei.is_some());
    }

    #[test]
    fn multi_provider_screening() {
        let screening = MultiProviderScreening {
            address: "0x1".into(), chain: Chain::Ethereum,
            results: vec![
                ProviderResult { provider: ComplianceProvider::Chainalysis, risk_score: 0.1, risk_level: "low".into(), flags: vec![], latency_ms: 200 },
                ProviderResult { provider: ComplianceProvider::Elliptic, risk_score: 0.15, risk_level: "low".into(), flags: vec![], latency_ms: 150 },
            ],
            aggregated_risk: AggregatedRisk { level: "low".into(), score: 0.15, method: AggregationMethod::WorstOf, providers_consulted: 2 },
            screened_at: Utc::now(),
        };
        assert_eq!(screening.results.len(), 2);
    }
}
