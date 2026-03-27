//! Transaction management — boost, drop, freeze/unfreeze, staking operations.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Transaction state management operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxOperation {
    /// Speed up a pending transaction (RBF/fee bump).
    Boost,
    /// Cancel a pending ETH transaction (zero-value self-transfer).
    Drop,
    /// Freeze a transaction (prevent broadcasting).
    Freeze,
    /// Unfreeze a previously frozen transaction.
    Unfreeze,
    /// Cancel before broadcast.
    Cancel,
}

/// Boost (accelerate) transaction request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoostRequest {
    pub tx_id: Uuid,
    pub fee_level: BoostFeeLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoostFeeLevel {
    Low,    // 12.5% bump (BIP-125 minimum)
    Medium, // 50% bump
    High,   // 100% bump
    Custom(u32), // custom percentage
}

/// Drop (cancel) ETH transaction by sending 0 ETH to self with same nonce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropRequest {
    pub tx_id: Uuid,
    pub gas_price_multiplier: Decimal,
}

/// Freeze/unfreeze transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeRequest {
    pub tx_id: Uuid,
    pub reason: Option<String>,
}

/// Extended staking operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum StakingOperation {
    /// Split a staking position across multiple validators.
    Split {
        position_id: Uuid,
        target_validators: Vec<ValidatorSplit>,
    },
    /// Merge multiple positions into one.
    Merge {
        position_ids: Vec<Uuid>,
        target_validator: String,
    },
    /// Claim accumulated rewards.
    ClaimRewards {
        position_id: Uuid,
        compound: bool, // re-stake rewards
    },
    /// Redelegate to a different validator without unbonding.
    Redelegate {
        position_id: Uuid,
        from_validator: String,
        to_validator: String,
        amount: Decimal,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSplit {
    pub validator: String,
    pub amount: Decimal,
}

/// UTXO selection strategy override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtxoSelectionStrategy {
    /// Branch and Bound (optimal, slow for many UTXOs).
    BranchAndBound,
    /// Largest first (fast, may overpay fees).
    LargestFirst,
    /// Smallest first (consolidation-friendly).
    SmallestFirst,
    /// Random (privacy-preserving).
    Random,
    /// Manual (caller selects specific UTXOs).
    Manual,
}

/// UTXO consolidation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidateUtxosRequest {
    pub vault_id: Uuid,
    pub max_utxos: Option<u32>,
    pub fee_rate_sat_per_vb: Option<Decimal>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidateUtxosResult {
    pub input_count: u32,
    pub output_count: u32,
    pub total_value: Decimal,
    pub fee: Decimal,
    pub tx_hash: Option<String>,
    pub was_dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn tx_operation_serialization() {
        assert_eq!(serde_json::to_string(&TxOperation::Boost).unwrap(), r#""boost""#);
        assert_eq!(serde_json::to_string(&TxOperation::Drop).unwrap(), r#""drop""#);
        assert_eq!(serde_json::to_string(&TxOperation::Freeze).unwrap(), r#""freeze""#);
    }

    #[test]
    fn staking_split() {
        let op = StakingOperation::Split {
            position_id: Uuid::nil(),
            target_validators: vec![
                ValidatorSplit { validator: "lido".into(), amount: dec!(16) },
                ValidatorSplit { validator: "rocketpool".into(), amount: dec!(16) },
            ],
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("split"));
        assert!(json.contains("lido"));
    }

    #[test]
    fn staking_merge() {
        let op = StakingOperation::Merge {
            position_ids: vec![Uuid::nil()],
            target_validator: "lido".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("merge"));
    }

    #[test]
    fn staking_redelegate() {
        let op = StakingOperation::Redelegate {
            position_id: Uuid::nil(),
            from_validator: "old".into(), to_validator: "new".into(),
            amount: dec!(32),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("redelegate"));
    }

    #[test]
    fn staking_claim_with_compound() {
        let op = StakingOperation::ClaimRewards { position_id: Uuid::nil(), compound: true };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("claim_rewards"));
        assert!(json.contains("true"));
    }

    #[test]
    fn utxo_selection_strategies() {
        assert_eq!(serde_json::to_string(&UtxoSelectionStrategy::BranchAndBound).unwrap(), r#""branch_and_bound""#);
        assert_eq!(serde_json::to_string(&UtxoSelectionStrategy::Random).unwrap(), r#""random""#);
    }

    #[test]
    fn consolidate_dry_run() {
        let result = ConsolidateUtxosResult {
            input_count: 50, output_count: 1,
            total_value: dec!(1.5), fee: dec!(0.0001),
            tx_hash: None, was_dry_run: true,
        };
        assert!(result.was_dry_run);
        assert!(result.tx_hash.is_none());
    }

    #[test]
    fn boost_fee_levels() {
        let json = serde_json::to_string(&BoostFeeLevel::Medium).unwrap();
        assert!(json.contains("medium"));
    }
}
