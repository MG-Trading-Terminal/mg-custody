//! Intent engine — user-level transaction intents resolved to optimal execution.
//!
//! "Send $1000 USDC to 0x..." → resolved to best chain, route, fee, timing.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::Chain;

/// A user-level intent (what they want to happen).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionIntent {
    pub id: Uuid,
    pub intent_type: IntentType,
    pub source_vault_id: Uuid,
    pub resolved: Option<ResolvedIntent>,
    pub status: IntentStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IntentType {
    Send { to: String, asset: String, amount: Decimal, preferred_chain: Option<Chain> },
    Swap { from_asset: String, to_asset: String, amount: Decimal },
    Bridge { asset: String, amount: Decimal, from_chain: Chain, to_chain: Chain },
    Stake { asset: String, amount: Decimal, validator: Option<String> },
    BatchPayout { payees: Vec<PayeeIntent> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayeeIntent {
    pub address: String,
    pub asset: String,
    pub amount: Decimal,
    pub chain: Option<Chain>,
}

/// How the intent was resolved to concrete transactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedIntent {
    pub chain: Chain,
    pub steps: Vec<ExecutionStep>,
    pub total_fee_usd: Decimal,
    pub estimated_time_secs: u32,
    pub route_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub order: u32,
    pub action: String,
    pub chain: Chain,
    pub estimated_gas: u64,
    pub fee_usd: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus { Created, Resolving, Resolved, Executing, Completed, Failed }

/// Resolve "send USDC to address" — pick best chain.
pub fn resolve_best_chain_for_usdc(amount: Decimal) -> (Chain, &'static str) {
    use rust_decimal_macros::dec;
    if amount < dec!(10) {
        (Chain::Solana, "Lowest fees for small amounts")
    } else if amount < dec!(1000) {
        (Chain::Tron, "Near-zero fees, fast confirmation")
    } else {
        (Chain::Ethereum, "Highest liquidity for large amounts")
    }
}

/// Estimate total cost for an intent.
pub fn estimate_intent_cost(intent: &IntentType) -> IntentCostEstimate {
    use rust_decimal_macros::dec;
    match intent {
        IntentType::Send { .. } => IntentCostEstimate {
            gas_fee_usd: dec!(2.50),
            service_fee_usd: Decimal::ZERO,
            total_usd: dec!(2.50),
            steps: 1,
        },
        IntentType::Swap { amount, .. } => IntentCostEstimate {
            gas_fee_usd: dec!(15.00),
            service_fee_usd: *amount * dec!(0.003), // 0.3% swap fee
            total_usd: dec!(15.00) + *amount * dec!(0.003),
            steps: 2, // approve + swap
        },
        IntentType::Bridge { amount, .. } => IntentCostEstimate {
            gas_fee_usd: dec!(5.00),
            service_fee_usd: *amount * dec!(0.001),
            total_usd: dec!(5.00) + *amount * dec!(0.001),
            steps: 3, // lock + relay + unlock
        },
        IntentType::Stake { .. } => IntentCostEstimate {
            gas_fee_usd: dec!(10.00),
            service_fee_usd: Decimal::ZERO,
            total_usd: dec!(10.00),
            steps: 1,
        },
        IntentType::BatchPayout { payees } => {
            let per_tx = dec!(1.50);
            let total = per_tx * Decimal::new(payees.len() as i64, 0);
            IntentCostEstimate { gas_fee_usd: total, service_fee_usd: Decimal::ZERO, total_usd: total, steps: payees.len() as u32 }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentCostEstimate {
    pub gas_fee_usd: Decimal,
    pub service_fee_usd: Decimal,
    pub total_usd: Decimal,
    pub steps: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn resolve_small_usdc_solana() {
        let (chain, _) = resolve_best_chain_for_usdc(dec!(5));
        assert_eq!(chain, Chain::Solana);
    }

    #[test]
    fn resolve_medium_usdc_tron() {
        let (chain, _) = resolve_best_chain_for_usdc(dec!(500));
        assert_eq!(chain, Chain::Tron);
    }

    #[test]
    fn resolve_large_usdc_ethereum() {
        let (chain, _) = resolve_best_chain_for_usdc(dec!(50000));
        assert_eq!(chain, Chain::Ethereum);
    }

    #[test]
    fn estimate_send_cost() {
        let intent = IntentType::Send { to: "0x1".into(), asset: "ETH".into(), amount: dec!(1), preferred_chain: None };
        let cost = estimate_intent_cost(&intent);
        assert_eq!(cost.steps, 1);
        assert!(cost.total_usd > Decimal::ZERO);
    }

    #[test]
    fn estimate_swap_cost() {
        let intent = IntentType::Swap { from_asset: "USDC".into(), to_asset: "ETH".into(), amount: dec!(10000) };
        let cost = estimate_intent_cost(&intent);
        assert_eq!(cost.steps, 2);
        assert!(cost.service_fee_usd > Decimal::ZERO); // 0.3%
    }

    #[test]
    fn estimate_batch_payout() {
        let intent = IntentType::BatchPayout { payees: vec![
            PayeeIntent { address: "0x1".into(), asset: "USDC".into(), amount: dec!(100), chain: None },
            PayeeIntent { address: "0x2".into(), asset: "USDC".into(), amount: dec!(200), chain: None },
            PayeeIntent { address: "0x3".into(), asset: "USDC".into(), amount: dec!(300), chain: None },
        ]};
        let cost = estimate_intent_cost(&intent);
        assert_eq!(cost.steps, 3);
    }

    #[test]
    fn intent_status_serialization() {
        assert_eq!(serde_json::to_string(&IntentStatus::Resolving).unwrap(), r#""resolving""#);
    }

    #[test]
    fn intent_type_send() {
        let t = IntentType::Send { to: "0x1".into(), asset: "ETH".into(), amount: dec!(1.5), preferred_chain: Some(Chain::Ethereum) };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("send"));
    }

    #[test]
    fn intent_type_bridge() {
        let t = IntentType::Bridge { asset: "USDC".into(), amount: dec!(1000), from_chain: Chain::Ethereum, to_chain: Chain::Solana };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("bridge"));
    }
}
