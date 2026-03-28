//! Deposit repository.

use custody_common::{Asset, Chain, Deposit, DepositStatus};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;
use crate::addresses::chain_from_str;

impl CustodyDb {
    /// Insert a newly detected deposit.
    pub async fn insert_deposit(&self, deposit: &Deposit) -> anyhow::Result<()> {
        let status_str = format!("{:?}", deposit.status).to_lowercase();
        sqlx::query(
            "INSERT INTO deposits (id, user_id, asset, chain, address, amount, tx_hash, log_index,
             block_height, confirmations, required_confirmations, status, detected_at, confirmed_at, credited_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (tx_hash, log_index) DO NOTHING",
        )
        .bind(deposit.id)
        .bind(deposit.user_id)
        .bind(deposit.asset.as_str())
        .bind(deposit.chain.as_str())
        .bind(&deposit.address)
        .bind(deposit.amount)
        .bind(&deposit.tx_hash)
        .bind(deposit.log_index.map(|i| i as i32))
        .bind(deposit.block_height as i64)
        .bind(deposit.confirmations as i64)
        .bind(deposit.required_confirmations as i64)
        .bind(&status_str)
        .bind(deposit.detected_at)
        .bind(deposit.confirmed_at)
        .bind(deposit.credited_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find deposit by tx_hash (deduplication).
    pub async fn find_deposit_by_tx_hash(&self, tx_hash: &str) -> anyhow::Result<Option<Deposit>> {
        let row: Option<DepositRow> = sqlx::query_as(
            "SELECT id, user_id, asset, chain, address, amount, tx_hash, log_index,
             block_height, confirmations, required_confirmations, status,
             detected_at, confirmed_at, credited_at
             FROM deposits WHERE tx_hash = $1",
        )
        .bind(tx_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_deposit()))
    }

    /// Get all deposits in a given status.
    pub async fn deposits_by_status(&self, status: DepositStatus) -> anyhow::Result<Vec<Deposit>> {
        let status_str = format!("{:?}", status).to_lowercase();
        let rows: Vec<DepositRow> = sqlx::query_as(
            "SELECT id, user_id, asset, chain, address, amount, tx_hash, log_index,
             block_height, confirmations, required_confirmations, status,
             detected_at, confirmed_at, credited_at
             FROM deposits WHERE status = $1
             ORDER BY detected_at ASC",
        )
        .bind(&status_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_deposit()).collect())
    }

    /// Count deposits detected for a given chain on the current calendar day (UTC).
    pub async fn deposits_count_today(&self, chain: Chain) -> anyhow::Result<u64> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deposits
             WHERE chain = $1
               AND detected_at >= date_trunc('day', NOW() AT TIME ZONE 'UTC')"
        )
        .bind(chain.as_str())
        .fetch_one(&self.pool)
        .await?;
        Ok(count as u64)
    }

    /// Update deposit status and confirmations.
    pub async fn update_deposit_status(
        &self,
        id: Uuid,
        status: DepositStatus,
        confirmations: u64,
    ) -> anyhow::Result<()> {
        let status_str = format!("{:?}", status).to_lowercase();
        let confirmed_at = if status == DepositStatus::Confirmed {
            Some(chrono::Utc::now())
        } else {
            None
        };

        sqlx::query(
            "UPDATE deposits SET status = $1, confirmations = $2, confirmed_at = COALESCE($3, confirmed_at)
             WHERE id = $4",
        )
        .bind(&status_str)
        .bind(confirmations as i64)
        .bind(confirmed_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct DepositRow {
    id: Uuid,
    user_id: Uuid,
    asset: String,
    chain: String,
    address: String,
    amount: Decimal,
    tx_hash: String,
    log_index: Option<i32>,
    block_height: i64,
    confirmations: i64,
    required_confirmations: i64,
    status: String,
    detected_at: chrono::DateTime<chrono::Utc>,
    confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    credited_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl DepositRow {
    fn into_deposit(self) -> Deposit {
        Deposit {
            id: self.id,
            user_id: self.user_id,
            asset: Asset::new(&self.asset),
            chain: chain_from_str(&self.chain),
            address: self.address,
            amount: self.amount,
            tx_hash: self.tx_hash,
            log_index: self.log_index.map(|i| i as u32),
            block_height: self.block_height as u64,
            confirmations: self.confirmations as u64,
            required_confirmations: self.required_confirmations as u64,
            status: match self.status.as_str() {
                "confirmed" => DepositStatus::Confirmed,
                "credited" => DepositStatus::Credited,
                "failed" => DepositStatus::Failed,
                _ => DepositStatus::Detected,
            },
            detected_at: self.detected_at,
            confirmed_at: self.confirmed_at,
            credited_at: self.credited_at,
            // memo fields populated when migration 024 has run.
            memo: None,
            source_memo: None,
        }
    }
}
