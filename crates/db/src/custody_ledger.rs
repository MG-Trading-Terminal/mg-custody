//! Double-entry custody ledger.
//!
//! Every fund movement (deposit credit, withdrawal debit, sweep) is recorded
//! as a pair of debit and credit entries. The sum of all entries for any account
//! must always equal zero (balanced books).
//!
//! This is the source of truth for internal balances, separate from on-chain state.

use chrono::{DateTime, Utc};
use custody_common::Chain;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CustodyDb;

/// Type of ledger entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Debit,
    Credit,
}

/// A single ledger entry (half of a double-entry pair).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: Uuid,
    pub entry_type: EntryType,
    /// Account identifier (e.g., "hot:eth", "user:123:eth", "fees:eth")
    pub account: String,
    pub amount: Decimal,
    pub asset: String,
    pub chain: Chain,
    /// Reference to the corresponding entry (debit ↔ credit pair).
    pub counterpart_id: Uuid,
    /// What triggered this entry (deposit_id, withdrawal_id, sweep_id).
    pub reference_id: Uuid,
    pub reference_type: String,
    pub created_at: DateTime<Utc>,
}

impl CustodyDb {
    /// Insert a double-entry pair (debit + credit) atomically.
    pub async fn insert_ledger_pair(
        &self,
        debit: &LedgerEntry,
        credit: &LedgerEntry,
    ) -> anyhow::Result<()> {
        let _ = (debit, credit);
        // TODO: INSERT both in a transaction
        // The debit.counterpart_id = credit.id and vice versa
        Ok(())
    }

    /// Get the balance for an account (sum of credits - sum of debits).
    pub async fn account_balance(&self, account: &str, asset: &str) -> anyhow::Result<Decimal> {
        let _ = (account, asset);
        // TODO: SELECT SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE -amount END)
        //       FROM custody_ledger WHERE account = $1 AND asset = $2
        Ok(Decimal::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_types_serialize() {
        assert_eq!(serde_json::to_string(&EntryType::Debit).unwrap(), "\"debit\"");
        assert_eq!(serde_json::to_string(&EntryType::Credit).unwrap(), "\"credit\"");
    }
}
