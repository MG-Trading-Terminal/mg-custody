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

impl LedgerEntry {
    /// Create a double-entry pair (debit + credit).
    pub fn pair(
        debit_account: &str,
        credit_account: &str,
        amount: Decimal,
        asset: &str,
        chain: Chain,
        reference_id: Uuid,
        reference_type: &str,
    ) -> (Self, Self) {
        let debit_id = Uuid::new_v4();
        let credit_id = Uuid::new_v4();
        let now = Utc::now();

        let debit = Self {
            id: debit_id,
            entry_type: EntryType::Debit,
            account: debit_account.to_string(),
            amount,
            asset: asset.to_string(),
            chain,
            counterpart_id: credit_id,
            reference_id,
            reference_type: reference_type.to_string(),
            created_at: now,
        };
        let credit = Self {
            id: credit_id,
            entry_type: EntryType::Credit,
            account: credit_account.to_string(),
            amount,
            asset: asset.to_string(),
            chain,
            counterpart_id: debit_id,
            reference_id,
            reference_type: reference_type.to_string(),
            created_at: now,
        };
        (debit, credit)
    }
}

impl CustodyDb {
    /// Insert a double-entry pair (debit + credit) atomically.
    pub async fn insert_ledger_pair(
        &self,
        debit: &LedgerEntry,
        credit: &LedgerEntry,
    ) -> anyhow::Result<()> {
        let entry_type_debit = match debit.entry_type {
            EntryType::Debit => "debit",
            EntryType::Credit => "credit",
        };
        let entry_type_credit = match credit.entry_type {
            EntryType::Debit => "debit",
            EntryType::Credit => "credit",
        };
        let chain_debit = debit.chain.as_str();
        let chain_credit = credit.chain.as_str();

        let mut tx = self.pool.begin().await?;

        // ON CONFLICT DO NOTHING is a no-op for reference types
        // that aren't covered by `idx_custody_ledger_deposit_idem`
        // (no constraint can match → no conflict → row inserts as
        // normal). For deposit / deposit_credit rows it makes the
        // pair idempotent: a retry after a partial-write window
        // (credit succeeded but status flip failed, see
        // confirmations::tracker::check_deposits) now skips the
        // duplicate insert instead of doubling the user's
        // balance. The write is the LAST line of defence against
        // the historical double-credit class — the tracker's
        // own ordering (credit before status flip) still does
        // the heavy lifting; this just removes the silent retry
        // failure mode.
        sqlx::query(
            "INSERT INTO custody_ledger (id, entry_type, account, amount, asset, chain, counterpart_id, reference_id, reference_type, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT DO NOTHING"
        )
        .bind(debit.id)
        .bind(entry_type_debit)
        .bind(&debit.account)
        .bind(debit.amount)
        .bind(&debit.asset)
        .bind(chain_debit)
        .bind(debit.counterpart_id)
        .bind(debit.reference_id)
        .bind(&debit.reference_type)
        .bind(debit.created_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO custody_ledger (id, entry_type, account, amount, asset, chain, counterpart_id, reference_id, reference_type, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT DO NOTHING"
        )
        .bind(credit.id)
        .bind(entry_type_credit)
        .bind(&credit.account)
        .bind(credit.amount)
        .bind(&credit.asset)
        .bind(chain_credit)
        .bind(credit.counterpart_id)
        .bind(credit.reference_id)
        .bind(&credit.reference_type)
        .bind(credit.created_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Get the balance for an account (sum of credits - sum of debits).
    pub async fn account_balance(&self, account: &str, asset: &str) -> anyhow::Result<Decimal> {
        let row: Option<(Decimal,)> = sqlx::query_as(
            "SELECT COALESCE(SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE -amount END), 0)
             FROM custody_ledger WHERE account = $1 AND asset = $2"
        )
        .bind(account)
        .bind(asset)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0).unwrap_or(Decimal::ZERO))
    }

    /// Get total balance across all accounts for an asset (useful for reconciliation).
    pub async fn total_asset_balance(&self, asset: &str) -> anyhow::Result<Decimal> {
        let row: Option<(Decimal,)> = sqlx::query_as(
            "SELECT COALESCE(SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE -amount END), 0)
             FROM custody_ledger WHERE asset = $1"
        )
        .bind(asset)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0).unwrap_or(Decimal::ZERO))
    }

    /// Credit a deposit: debit external, credit hot wallet.
    pub async fn credit_deposit(
        &self,
        deposit_id: Uuid,
        asset: &str,
        chain: Chain,
        amount: Decimal,
        user_id: Uuid,
    ) -> anyhow::Result<()> {
        let (debit, credit) = LedgerEntry::pair(
            &format!("external:{}", chain.as_str()),
            &format!("hot:{asset}"),
            amount,
            asset,
            chain,
            deposit_id,
            "deposit",
        );
        self.insert_ledger_pair(&debit, &credit).await?;

        // Also credit user's internal balance
        let (debit2, credit2) = LedgerEntry::pair(
            &format!("hot:{asset}"),
            &format!("user:{user_id}:{asset}"),
            amount,
            asset,
            chain,
            deposit_id,
            "deposit_credit",
        );
        self.insert_ledger_pair(&debit2, &credit2).await?;

        Ok(())
    }

    /// Debit a withdrawal: debit hot wallet, credit external.
    pub async fn debit_withdrawal(
        &self,
        withdrawal_id: Uuid,
        asset: &str,
        chain: Chain,
        amount: Decimal,
        user_id: Uuid,
    ) -> anyhow::Result<()> {
        // Debit user's internal balance
        let (debit, credit) = LedgerEntry::pair(
            &format!("user:{user_id}:{asset}"),
            &format!("hot:{asset}"),
            amount,
            asset,
            chain,
            withdrawal_id,
            "withdrawal_debit",
        );
        self.insert_ledger_pair(&debit, &credit).await?;

        // Debit hot wallet, credit external
        let (debit2, credit2) = LedgerEntry::pair(
            &format!("hot:{asset}"),
            &format!("external:{}", chain.as_str()),
            amount,
            asset,
            chain,
            withdrawal_id,
            "withdrawal",
        );
        self.insert_ledger_pair(&debit2, &credit2).await?;

        Ok(())
    }

    /// Record a sweep between wallet tiers.
    pub async fn record_sweep(
        &self,
        sweep_id: Uuid,
        from_tier: &str,
        to_tier: &str,
        asset: &str,
        chain: Chain,
        amount: Decimal,
    ) -> anyhow::Result<()> {
        let (debit, credit) = LedgerEntry::pair(
            &format!("{from_tier}:{asset}"),
            &format!("{to_tier}:{asset}"),
            amount,
            asset,
            chain,
            sweep_id,
            "sweep",
        );
        self.insert_ledger_pair(&debit, &credit).await
    }

    /// Record an internal transfer between two vaults (ledger-only, no on-chain tx).
    pub async fn record_internal_transfer(
        &self,
        transfer_id: Uuid,
        source_vault_id: Uuid,
        dest_vault_id: Uuid,
        asset: &str,
        chain: Chain,
        amount: Decimal,
    ) -> anyhow::Result<()> {
        let (debit, credit) = LedgerEntry::pair(
            &format!("vault:{source_vault_id}:{asset}:{}", chain.as_str()),
            &format!("vault:{dest_vault_id}:{asset}:{}", chain.as_str()),
            amount,
            asset,
            chain,
            transfer_id,
            "internal_transfer",
        );
        self.insert_ledger_pair(&debit, &credit).await
    }

    /// Get recent ledger entries for an account.
    pub async fn recent_entries(
        &self,
        account: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<LedgerEntry>> {
        let rows: Vec<(Uuid, String, String, Decimal, String, String, Uuid, Uuid, String, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, entry_type, account, amount, asset, chain, counterpart_id, reference_id, reference_type, created_at
                 FROM custody_ledger WHERE account = $1
                 ORDER BY created_at DESC LIMIT $2"
            )
            .bind(account)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(|(id, entry_type, account, amount, asset, chain, counterpart_id, reference_id, reference_type, created_at)| {
            let c = custody_common::Chain::try_parse_str(&chain)
                .ok_or_else(|| anyhow::anyhow!("custody_ledger row {id}: unknown chain '{chain}'"))?;
            Ok(LedgerEntry {
                id,
                entry_type: if entry_type == "credit" { EntryType::Credit } else { EntryType::Debit },
                account,
                amount,
                asset,
                chain: c,
                counterpart_id,
                reference_id,
                reference_type,
                created_at,
            })
        }).collect()
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

    #[test]
    fn ledger_pair_creates_matching_entries() {
        let (debit, credit) = LedgerEntry::pair(
            "hot:eth", "user:123:eth",
            Decimal::new(1000, 0), "ETH",
            Chain::Ethereum, Uuid::new_v4(), "deposit",
        );

        assert_eq!(debit.entry_type, EntryType::Debit);
        assert_eq!(credit.entry_type, EntryType::Credit);
        assert_eq!(debit.counterpart_id, credit.id);
        assert_eq!(credit.counterpart_id, debit.id);
        assert_eq!(debit.amount, credit.amount);
        assert_eq!(debit.account, "hot:eth");
        assert_eq!(credit.account, "user:123:eth");
    }
}
