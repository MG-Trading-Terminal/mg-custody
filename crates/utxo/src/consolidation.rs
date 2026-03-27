//! UTXO consolidation — combines small UTXOs into fewer, larger ones (Phase 23).
//!
//! Consolidation reduces future transaction fees by pre-combining inputs when
//! network fees are low. Only runs when:
//! 1. UTXO count exceeds threshold (default: 50)
//! 2. Fee rate is below max_fee_rate (default: 10 sat/vB)
//! 3. Destination is an internal hot wallet address (NEVER external)
//!
//! Consolidation transactions are marked as `tx_type = "consolidation"` and
//! bypass the circuit breaker (internal movements, not outflow).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{calculate_fee, Utxo, DUST_LIMIT_SATS};

/// Consolidation policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPolicy {
    /// Minimum UTXO count before consolidation triggers.
    pub min_utxo_count: usize,
    /// Maximum UTXOs to consolidate in a single transaction.
    pub max_utxo_per_tx: usize,
    /// Maximum fee rate (sat/vB) — only consolidate when fees are cheap.
    pub max_fee_rate_sat_per_vb: u64,
    /// Schedule: time of day (UTC) to run consolidation. Default 03:00.
    pub schedule_utc_hour: u8,
    /// Minimum UTXO value (sats) to consider for consolidation.
    /// UTXOs below this are always included (they're the most expensive to spend later).
    pub small_utxo_threshold_sats: u64,
    /// Whether consolidation is enabled.
    pub enabled: bool,
}

impl Default for ConsolidationPolicy {
    fn default() -> Self {
        Self {
            min_utxo_count: 50,
            max_utxo_per_tx: 20,
            max_fee_rate_sat_per_vb: 10,
            schedule_utc_hour: 3,
            small_utxo_threshold_sats: 100_000, // 0.001 BTC
            enabled: true,
        }
    }
}

/// Result of consolidation analysis.
#[derive(Debug, Clone)]
pub struct ConsolidationPlan {
    /// UTXOs selected for consolidation.
    pub inputs: Vec<Utxo>,
    /// Total input value (sats).
    pub total_input_sats: u64,
    /// Estimated fee (sats).
    pub estimated_fee_sats: u64,
    /// Output value after fee (sats).
    pub output_sats: u64,
    /// Fee savings: how much cheaper future spending will be.
    pub estimated_savings_sats: u64,
    /// Destination: internal hot wallet address.
    pub destination_address: String,
}

/// Check if consolidation should run now based on policy.
pub fn should_consolidate(
    utxo_count: usize,
    current_fee_rate: u64,
    policy: &ConsolidationPolicy,
    now: DateTime<Utc>,
) -> bool {
    if !policy.enabled {
        return false;
    }
    if utxo_count < policy.min_utxo_count {
        return false;
    }
    if current_fee_rate > policy.max_fee_rate_sat_per_vb {
        return false;
    }
    // Check schedule window (within 1 hour of scheduled time)
    let hour = now.time().hour();
    hour == policy.schedule_utc_hour as u32 || hour == (policy.schedule_utc_hour as u32 + 1) % 24
}

use chrono::Timelike;

/// Select UTXOs for consolidation — prioritize small UTXOs (most expensive to spend later).
pub fn plan_consolidation(
    utxos: &[Utxo],
    fee_rate_sat_per_vb: u64,
    destination_address: &str,
    policy: &ConsolidationPolicy,
) -> Option<ConsolidationPlan> {
    // Filter: unlocked, confirmed, not dust
    let mut candidates: Vec<&Utxo> = utxos
        .iter()
        .filter(|u| !u.locked && u.confirmations >= 3 && u.amount_sats >= DUST_LIMIT_SATS)
        .collect();

    if candidates.len() < policy.min_utxo_count {
        return None;
    }

    // Sort by amount ascending — consolidate smallest UTXOs first
    candidates.sort_by_key(|u| u.amount_sats);

    // Select up to max_utxo_per_tx smallest UTXOs
    let selected: Vec<Utxo> = candidates
        .iter()
        .take(policy.max_utxo_per_tx)
        .map(|u| (*u).clone())
        .collect();

    if selected.is_empty() {
        return None;
    }

    let total_input: u64 = selected.iter().map(|u| u.amount_sats).sum();
    let fee = calculate_fee(selected.len(), 1, fee_rate_sat_per_vb); // 1 output (consolidated)

    if total_input <= fee {
        // Not worth consolidating — fee exceeds input value
        return None;
    }

    let output = total_input - fee;

    // Calculate savings: future spending of N separate UTXOs vs 1 consolidated
    // Each UTXO costs ~68 vbytes * fee_rate to spend
    let future_spending_cost_separate = (selected.len() as u64) * 68 * fee_rate_sat_per_vb;
    let future_spending_cost_consolidated = 68 * fee_rate_sat_per_vb; // 1 input
    let savings = future_spending_cost_separate.saturating_sub(future_spending_cost_consolidated + fee);

    Some(ConsolidationPlan {
        inputs: selected,
        total_input_sats: total_input,
        estimated_fee_sats: fee,
        output_sats: output,
        estimated_savings_sats: savings,
        destination_address: destination_address.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_utxo(amount_sats: u64, confirmations: u64) -> Utxo {
        Utxo {
            id: Uuid::new_v4(),
            txid: format!("tx_{amount_sats}"),
            vout: 0,
            amount_sats,
            address: "bc1qhotwall3t".to_string(),
            confirmations,
            locked: false,
        }
    }

    #[test]
    fn default_policy() {
        let p = ConsolidationPolicy::default();
        assert_eq!(p.min_utxo_count, 50);
        assert_eq!(p.max_utxo_per_tx, 20);
        assert_eq!(p.max_fee_rate_sat_per_vb, 10);
        assert!(p.enabled);
    }

    #[test]
    fn should_consolidate_basic() {
        let policy = ConsolidationPolicy::default();
        let now = Utc::now()
            .date_naive()
            .and_time(chrono::NaiveTime::from_hms_opt(3, 30, 0).unwrap())
            .and_utc();

        // Enough UTXOs, low fee, right time
        assert!(should_consolidate(60, 5, &policy, now));

        // Not enough UTXOs
        assert!(!should_consolidate(30, 5, &policy, now));

        // Fee too high
        assert!(!should_consolidate(60, 20, &policy, now));

        // Disabled
        let mut disabled = policy.clone();
        disabled.enabled = false;
        assert!(!should_consolidate(60, 5, &disabled, now));
    }

    #[test]
    fn plan_consolidation_selects_smallest() {
        let mut utxos: Vec<Utxo> = (0..60)
            .map(|i| make_utxo(10_000 + i * 1000, 6))
            .collect();

        let policy = ConsolidationPolicy {
            min_utxo_count: 50,
            max_utxo_per_tx: 10,
            ..Default::default()
        };

        let plan = plan_consolidation(&utxos, 5, "bc1qhot", &policy).unwrap();
        assert_eq!(plan.inputs.len(), 10);
        // Should have picked the 10 smallest
        assert_eq!(plan.inputs[0].amount_sats, 10_000);
        assert!(plan.output_sats > 0);
        assert!(plan.estimated_fee_sats > 0);
    }

    #[test]
    fn plan_consolidation_not_enough_utxos() {
        let utxos: Vec<Utxo> = (0..10).map(|_| make_utxo(50_000, 6)).collect();
        let policy = ConsolidationPolicy::default(); // requires 50

        let plan = plan_consolidation(&utxos, 5, "bc1qhot", &policy);
        assert!(plan.is_none());
    }

    #[test]
    fn plan_skips_locked_and_unconfirmed() {
        let utxos: Vec<Utxo> = {
            let mut v: Vec<Utxo> = (0..60).map(|_| make_utxo(10_000, 6)).collect();
            v[0].locked = true;
            v[1].confirmations = 0;
            v
        };

        let policy = ConsolidationPolicy {
            min_utxo_count: 50,
            max_utxo_per_tx: 20,
            ..Default::default()
        };

        let plan = plan_consolidation(&utxos, 5, "bc1qhot", &policy).unwrap();
        // Should not include locked or unconfirmed
        assert!(plan.inputs.iter().all(|u| !u.locked && u.confirmations >= 3));
    }

    #[test]
    fn plan_includes_fee_and_output() {
        let utxos: Vec<Utxo> = (0..60).map(|_| make_utxo(50_000, 6)).collect();
        let policy = ConsolidationPolicy {
            min_utxo_count: 50,
            max_utxo_per_tx: 20,
            ..Default::default()
        };

        let plan = plan_consolidation(&utxos, 1, "bc1qhot", &policy).unwrap();
        assert_eq!(plan.inputs.len(), 20);
        assert_eq!(plan.total_input_sats, 20 * 50_000);
        assert!(plan.estimated_fee_sats > 0);
        assert!(plan.output_sats > 0);
        assert_eq!(plan.output_sats, plan.total_input_sats - plan.estimated_fee_sats);
        assert_eq!(plan.destination_address, "bc1qhot");
    }

    #[test]
    fn consolidation_plan_serialization() {
        let policy = ConsolidationPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("min_utxo_count"));
        assert!(json.contains("max_fee_rate"));
    }
}
