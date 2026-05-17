//! Scheduled transaction persistence.

use chrono::{DateTime, Utc};
use custody_common::operations::{ScheduleStatus, ScheduledTransaction};
use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_scheduled_tx(&self, tx: &ScheduledTransaction) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO scheduled_transactions (id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, note, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(tx.id).bind(tx.vault_id).bind(tx.asset.as_str()).bind(tx.chain.as_str())
        .bind(&tx.destination_address).bind(tx.amount).bind(tx.scheduled_at)
        .bind(tx.status.to_string()).bind(&tx.note).bind(tx.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_scheduled_tx(&self, id: Uuid) -> anyhow::Result<Option<ScheduledTransaction>> {
        let row: Option<ScheduledTxRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
             FROM scheduled_transactions WHERE id = $1",
        ).bind(id).fetch_optional(&self.pool).await?;
        row.map(|r| r.into_tx()).transpose()
    }

    pub async fn list_scheduled_tx(&self, vault_id: Option<Uuid>, status: Option<&str>) -> anyhow::Result<Vec<ScheduledTransaction>> {
        let rows: Vec<ScheduledTxRow> = match (vault_id, status) {
            (Some(v), Some(s)) => sqlx::query_as(
                "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
                 FROM scheduled_transactions WHERE vault_id = $1 AND status = $2 ORDER BY scheduled_at",
            ).bind(v).bind(s).fetch_all(&self.pool).await?,
            (Some(v), None) => sqlx::query_as(
                "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
                 FROM scheduled_transactions WHERE vault_id = $1 ORDER BY scheduled_at",
            ).bind(v).fetch_all(&self.pool).await?,
            (None, Some(s)) => sqlx::query_as(
                "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
                 FROM scheduled_transactions WHERE status = $1 ORDER BY scheduled_at",
            ).bind(s).fetch_all(&self.pool).await?,
            (None, None) => sqlx::query_as(
                "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
                 FROM scheduled_transactions ORDER BY scheduled_at LIMIT 200",
            ).fetch_all(&self.pool).await?,
        };
        rows.into_iter().map(|r| r.into_tx()).collect()
    }

    pub async fn cancel_scheduled_tx(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE scheduled_transactions SET status = 'cancelled', cancelled_at = NOW() WHERE id = $1 AND status = 'pending'",
        ).bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn execute_scheduled_tx(&self, id: Uuid, withdrawal_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE scheduled_transactions SET status = 'executed', withdrawal_id = $2, executed_at = NOW() WHERE id = $1 AND status = 'pending'",
        ).bind(id).bind(withdrawal_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get scheduled transactions that are due for execution.
    pub async fn due_scheduled_tx(&self) -> anyhow::Result<Vec<ScheduledTransaction>> {
        let rows: Vec<ScheduledTxRow> = sqlx::query_as(
            "SELECT id, vault_id, asset, chain, destination_address, amount, scheduled_at, status, withdrawal_id, note, created_at, executed_at, cancelled_at
             FROM scheduled_transactions WHERE status = 'pending' AND scheduled_at <= NOW() ORDER BY scheduled_at LIMIT 50",
        ).fetch_all(&self.pool).await?;
        rows.into_iter().map(|r| r.into_tx()).collect()
    }
}

#[derive(sqlx::FromRow)]
struct ScheduledTxRow {
    id: Uuid, vault_id: Uuid, asset: String, chain: String,
    destination_address: String, amount: Decimal, scheduled_at: DateTime<Utc>,
    status: String, withdrawal_id: Option<Uuid>, note: Option<String>,
    created_at: DateTime<Utc>, executed_at: Option<DateTime<Utc>>,
    cancelled_at: Option<DateTime<Utc>>,
}

impl ScheduledTxRow {
    fn into_tx(self) -> anyhow::Result<ScheduledTransaction> {
        let status = match self.status.as_str() {
            "executed" => ScheduleStatus::Executed,
            "cancelled" => ScheduleStatus::Cancelled,
            "failed" => ScheduleStatus::Failed,
            "expired" => ScheduleStatus::Expired,
            _ => ScheduleStatus::Pending,
        };
        let chain = Chain::try_parse_str(&self.chain)
            .ok_or_else(|| anyhow::anyhow!("scheduled_transactions row {}: unknown chain '{}'", self.id, self.chain))?;
        Ok(ScheduledTransaction {
            id: self.id, vault_id: self.vault_id,
            asset: Asset::new(&self.asset), chain,
            destination_address: self.destination_address, amount: self.amount,
            scheduled_at: self.scheduled_at, status,
            withdrawal_id: self.withdrawal_id, note: self.note,
            created_at: self.created_at, executed_at: self.executed_at,
            cancelled_at: self.cancelled_at,
        })
    }
}
