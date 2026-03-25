//! Staking operations — beacon chain deposit, SOL delegation, rewards.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ETH beacon chain deposit parameters.
/// Requires exactly 32 ETH per validator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthStakeRequest {
    pub validator_pubkey: String,
    pub withdrawal_credentials: String,
    pub amount_eth: Decimal, // must be 32
    pub deposit_data_root: String,
}

/// SOL delegation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolDelegateRequest {
    pub stake_account: String,
    pub vote_account: String, // validator
    pub amount_lamports: u64,
}

/// Staking rewards tracking entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingReward {
    pub id: Uuid,
    pub position_id: Uuid,
    pub chain: String,
    pub asset: String,
    pub reward_amount: Decimal,
    pub epoch: u64,
    pub earned_at: chrono::DateTime<chrono::Utc>,
}

/// Validator performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorMetrics {
    pub address: String,
    pub chain: String,
    pub name: Option<String>,
    pub uptime_pct: Decimal,
    pub commission_pct: Decimal,
    pub total_staked: Decimal,
    pub apy_pct: Decimal,
    pub slashing_history: u32,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

impl ValidatorMetrics {
    /// Check if this validator is safe to delegate to.
    pub fn is_safe(&self) -> bool {
        self.uptime_pct >= Decimal::new(95, 0) // > 95% uptime
            && self.slashing_history == 0
            && self.commission_pct <= Decimal::new(20, 0) // < 20% commission
    }
}

/// DeFi protocol whitelist entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefiProtocolEntry {
    pub name: String,
    pub chain_id: u64,
    pub contract_addresses: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub risk_level: String,
    pub approved_by: String,
    pub approved_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn validator_safety_check() {
        let safe = ValidatorMetrics {
            address: "0xval1".into(), chain: "ethereum".into(), name: Some("Good Validator".into()),
            uptime_pct: dec!(99.5), commission_pct: dec!(5), total_staked: dec!(32000),
            apy_pct: dec!(3.6), slashing_history: 0, last_updated: chrono::Utc::now(),
        };
        assert!(safe.is_safe());

        let risky = ValidatorMetrics {
            uptime_pct: dec!(80), slashing_history: 1, ..safe.clone()
        };
        assert!(!risky.is_safe());
    }

    #[test]
    fn staking_reward_tracking() {
        let reward = StakingReward {
            id: Uuid::new_v4(), position_id: Uuid::new_v4(),
            chain: "ethereum".into(), asset: "ETH".into(),
            reward_amount: dec!(0.001), epoch: 300000,
            earned_at: chrono::Utc::now(),
        };
        assert!(reward.reward_amount > Decimal::ZERO);
    }

    #[test]
    fn defi_protocol_whitelist() {
        let entry = DefiProtocolEntry {
            name: "Uniswap V3".into(), chain_id: 1,
            contract_addresses: vec!["0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45".into()],
            allowed_methods: vec!["exactInputSingle".into(), "exactInput".into()],
            risk_level: "low".into(), approved_by: "admin".into(),
            approved_at: chrono::Utc::now(),
        };
        assert_eq!(entry.allowed_methods.len(), 2);
    }
}
