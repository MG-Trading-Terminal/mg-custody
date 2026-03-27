//! Chain reorganization detection — handle block reorgs safely.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::Chain;

/// A detected chain reorganization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorgEvent {
    pub chain: Chain,
    pub depth: u32,
    pub old_tip_hash: String,
    pub new_tip_hash: String,
    pub old_height: u64,
    pub new_height: u64,
    pub affected_deposits: Vec<String>,
    pub affected_withdrawals: Vec<String>,
    pub detected_at: DateTime<Utc>,
}

/// Maximum safe reorg depth per chain.
pub fn max_reorg_depth(chain: Chain) -> u32 {
    match chain {
        Chain::Bitcoin => 6,      // 6 blocks (1 hour)
        Chain::Ethereum => 64,    // 64 blocks (~13 min, post-merge)
        Chain::Tron => 20,        // 20 blocks (~1 min)
        Chain::Solana => 32,      // 32 slots (~12 sec)
    }
}

/// Check if a reorg is within safe bounds.
pub fn is_reorg_safe(chain: Chain, depth: u32) -> bool {
    depth <= max_reorg_depth(chain)
}

/// Actions to take after detecting a reorg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReorgAction {
    Rescan,
    RevertDeposits,
    RebroadcastWithdrawals,
    HaltScanner,
    AlertOperator,
}

pub fn reorg_actions(chain: Chain, depth: u32) -> Vec<ReorgAction> {
    let mut actions = vec![ReorgAction::Rescan];
    if depth > 1 { actions.push(ReorgAction::RevertDeposits); }
    if depth > max_reorg_depth(chain) / 2 { actions.push(ReorgAction::AlertOperator); }
    if depth > max_reorg_depth(chain) { actions.push(ReorgAction::HaltScanner); }
    actions
}

/// Withdrawal queue with priority scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalQueue {
    pub pending: Vec<QueuedWithdrawal>,
    pub max_concurrent: u32,
    pub current_processing: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedWithdrawal {
    pub id: uuid::Uuid,
    pub priority: QueuePriority,
    pub chain: Chain,
    pub amount_usd: rust_decimal::Decimal,
    pub queued_at: DateTime<Utc>,
    pub estimated_process_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePriority { Low, Normal, High, Urgent }

/// Transaction batching — group small transfers for gas efficiency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBatch {
    pub id: uuid::Uuid,
    pub chain: Chain,
    pub items: Vec<BatchItem>,
    pub total_gas_individual: u64,
    pub total_gas_batched: u64,
    pub savings_percent: f32,
    pub status: BatchStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub withdrawal_id: uuid::Uuid,
    pub to_address: String,
    pub amount: rust_decimal::Decimal,
    pub asset: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus { Collecting, Ready, Processing, Completed, Failed }

/// Optimal batch size per chain.
pub fn optimal_batch_size(chain: Chain) -> u32 {
    match chain {
        Chain::Ethereum => 20,  // multicall batching
        Chain::Bitcoin => 50,   // UTXO batching
        Chain::Tron => 10,
        Chain::Solana => 5,     // limited by tx size (1232 bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorg_depths() {
        assert_eq!(max_reorg_depth(Chain::Bitcoin), 6);
        assert_eq!(max_reorg_depth(Chain::Ethereum), 64);
    }

    #[test]
    fn reorg_safe() {
        assert!(is_reorg_safe(Chain::Bitcoin, 3));
        assert!(!is_reorg_safe(Chain::Bitcoin, 10));
    }

    #[test]
    fn reorg_actions_shallow() {
        let actions = reorg_actions(Chain::Ethereum, 1);
        assert!(actions.contains(&ReorgAction::Rescan));
        assert!(!actions.contains(&ReorgAction::HaltScanner));
    }

    #[test]
    fn reorg_actions_deep() {
        let actions = reorg_actions(Chain::Ethereum, 100);
        assert!(actions.contains(&ReorgAction::HaltScanner));
        assert!(actions.contains(&ReorgAction::AlertOperator));
    }

    #[test]
    fn queue_priority_ordering() {
        assert!(QueuePriority::Low < QueuePriority::Normal);
        assert!(QueuePriority::High < QueuePriority::Urgent);
    }

    #[test]
    fn optimal_batch_sizes() {
        assert_eq!(optimal_batch_size(Chain::Ethereum), 20);
        assert_eq!(optimal_batch_size(Chain::Solana), 5);
    }

    #[test]
    fn batch_status_serialization() {
        assert_eq!(serde_json::to_string(&BatchStatus::Collecting).unwrap(), r#""collecting""#);
    }

    #[test]
    fn reorg_action_serialization() {
        assert_eq!(serde_json::to_string(&ReorgAction::HaltScanner).unwrap(), r#""halt_scanner""#);
    }

    #[test]
    fn reorg_event_structure() {
        let event = ReorgEvent {
            chain: Chain::Bitcoin, depth: 3,
            old_tip_hash: "old".into(), new_tip_hash: "new".into(),
            old_height: 840000, new_height: 840000,
            affected_deposits: vec!["dep1".into()],
            affected_withdrawals: vec![],
            detected_at: Utc::now(),
        };
        assert_eq!(event.affected_deposits.len(), 1);
    }
}
