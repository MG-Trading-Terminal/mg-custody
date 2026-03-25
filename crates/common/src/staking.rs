//! Staking types and primitives.
//!
//! Supports native staking on PoS chains (ETH, SOL, Cosmos).
//! Tracks validator selection, rewards, and unstaking/withdrawal queues.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// Staking position status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingStatus {
    /// Stake transaction pending.
    Pending,
    /// Actively staking, earning rewards.
    Active,
    /// Unstaking initiated, in cooldown period.
    Unstaking,
    /// Unstaking complete, ready to withdraw.
    Withdrawable,
    /// Withdrawn back to custody.
    Withdrawn,
    /// Slashed by the network.
    Slashed,
}

/// A staking position (delegation to a validator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingPosition {
    pub id: Uuid,
    pub chain: Chain,
    pub validator_address: String,
    pub validator_name: Option<String>,
    pub amount: Decimal,
    pub asset: String,
    pub status: StakingStatus,
    pub rewards_earned: Decimal,
    pub annual_yield_pct: Option<Decimal>,
    pub staked_at: DateTime<Utc>,
    pub unstake_requested_at: Option<DateTime<Utc>>,
    pub withdrawable_at: Option<DateTime<Utc>>,
}

/// Staking chain parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingParams {
    pub chain: Chain,
    /// Minimum stake amount.
    pub min_stake: Decimal,
    /// Unbonding/cooldown period in seconds.
    pub unbonding_period_secs: u64,
    /// Current estimated APY.
    pub estimated_apy_pct: Decimal,
}

/// ETH staking parameters.
pub fn eth_staking_params() -> StakingParams {
    StakingParams {
        chain: Chain::Ethereum,
        min_stake: Decimal::new(32, 0), // 32 ETH per validator
        unbonding_period_secs: 27 * 3600, // ~27 hours (variable)
        estimated_apy_pct: Decimal::new(36, 1), // ~3.6% APY
    }
}

/// SOL staking parameters.
pub fn sol_staking_params() -> StakingParams {
    StakingParams {
        chain: Chain::Solana,
        min_stake: Decimal::new(1, 0), // 1 SOL minimum
        unbonding_period_secs: 2 * 24 * 3600 + 12 * 3600, // ~2-3 days (epoch boundary)
        estimated_apy_pct: Decimal::new(70, 1), // ~7.0% APY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn eth_staking_params_valid() {
        let params = eth_staking_params();
        assert_eq!(params.chain, Chain::Ethereum);
        assert_eq!(params.min_stake, dec!(32));
        assert!(params.estimated_apy_pct > dec!(0));
    }

    #[test]
    fn sol_staking_params_valid() {
        let params = sol_staking_params();
        assert_eq!(params.chain, Chain::Solana);
        assert_eq!(params.min_stake, dec!(1));
    }

    #[test]
    fn staking_status_serialization() {
        assert_eq!(serde_json::to_string(&StakingStatus::Active).unwrap(), "\"active\"");
        assert_eq!(serde_json::to_string(&StakingStatus::Unstaking).unwrap(), "\"unstaking\"");
        assert_eq!(serde_json::to_string(&StakingStatus::Slashed).unwrap(), "\"slashed\"");
    }

    #[test]
    fn staking_position_creation() {
        let pos = StakingPosition {
            id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            validator_address: "0x123...".to_string(),
            validator_name: Some("Lido".to_string()),
            amount: dec!(32),
            asset: "ETH".to_string(),
            status: StakingStatus::Active,
            rewards_earned: dec!(0.5),
            annual_yield_pct: Some(dec!(3.6)),
            staked_at: Utc::now(),
            unstake_requested_at: None,
            withdrawable_at: None,
        };
        assert_eq!(pos.status, StakingStatus::Active);
        assert_eq!(pos.amount, dec!(32));
    }
}
