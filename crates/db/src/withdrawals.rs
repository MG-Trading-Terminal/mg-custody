//! Withdrawal repository.

use custody_common::{Asset, WalletTier, Withdrawal, WithdrawalStatus};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;
use crate::addresses::chain_from_str;

impl CustodyDb {
    /// Insert a new withdrawal request.
    pub async fn insert_withdrawal(&self, w: &Withdrawal) -> anyhow::Result<()> {
        let status_str = format!("{:?}", w.status).to_lowercase();
        let tier_str = format!("{:?}", w.wallet_tier).to_lowercase();

        sqlx::query(
            "INSERT INTO withdrawals (id, exchange_withdrawal_id, asset, chain, destination_address,
             amount, fee, status, wallet_tier, tx_hash, block_height, confirmations, error_message,
             created_at, signed_at, broadcast_at, confirmed_at, delay_release_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
             ON CONFLICT (exchange_withdrawal_id) DO NOTHING",
        )
        .bind(w.id)
        .bind(w.exchange_withdrawal_id)
        .bind(w.asset.as_str())
        .bind(w.chain.as_str())
        .bind(&w.destination_address)
        .bind(w.amount)
        .bind(w.fee)
        .bind(&status_str)
        .bind(&tier_str)
        .bind(&w.tx_hash)
        .bind(w.block_height.map(|h| h as i64))
        .bind(w.confirmations as i64)
        .bind(&w.error_message)
        .bind(w.created_at)
        .bind(w.signed_at)
        .bind(w.broadcast_at)
        .bind(w.confirmed_at)
        .bind(w.delay_release_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find withdrawal by exchange's idempotency key.
    pub async fn find_withdrawal_by_exchange_id(
        &self,
        exchange_id: Uuid,
    ) -> anyhow::Result<Option<Withdrawal>> {
        let row: Option<WithdrawalRow> = sqlx::query_as(
            "SELECT * FROM withdrawals WHERE exchange_withdrawal_id = $1",
        )
        .bind(exchange_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_withdrawal()))
    }

    /// Find withdrawal by custody ref ID.
    pub async fn find_withdrawal(&self, id: Uuid) -> anyhow::Result<Option<Withdrawal>> {
        let row: Option<WithdrawalRow> = sqlx::query_as(
            "SELECT * FROM withdrawals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_withdrawal()))
    }

    /// Get all withdrawals in a given status.
    pub async fn withdrawals_by_status(
        &self,
        status: WithdrawalStatus,
    ) -> anyhow::Result<Vec<Withdrawal>> {
        let status_str = format!("{:?}", status).to_lowercase();
        let rows: Vec<WithdrawalRow> = sqlx::query_as(
            "SELECT * FROM withdrawals WHERE status = $1 ORDER BY created_at ASC",
        )
        .bind(&status_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_withdrawal()).collect())
    }

    /// Update withdrawal status.
    pub async fn update_withdrawal_status(
        &self,
        id: Uuid,
        status: WithdrawalStatus,
    ) -> anyhow::Result<()> {
        let status_str = format!("{:?}", status).to_lowercase();
        sqlx::query("UPDATE withdrawals SET status = $1 WHERE id = $2")
            .bind(&status_str)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Set tx_hash after broadcast.
    pub async fn set_withdrawal_tx_hash(
        &self,
        id: Uuid,
        tx_hash: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE withdrawals SET tx_hash = $1, broadcast_at = NOW() WHERE id = $2",
        )
        .bind(tx_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Store signed transaction bytes after signing.
    pub async fn set_signed_tx(
        &self,
        id: Uuid,
        signed_tx_bytes: &[u8],
        nonce: u64,
        gas_price_gwei: Decimal,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE withdrawals SET signed_tx_bytes = $1, nonce = $2, gas_price_gwei = $3, signed_at = NOW()
             WHERE id = $4",
        )
        .bind(signed_tx_bytes)
        .bind(nonce as i64)
        .bind(gas_price_gwei)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get signed transaction bytes for broadcast.
    pub async fn get_signed_tx_bytes(&self, id: Uuid) -> anyhow::Result<Option<Vec<u8>>> {
        let row: Option<(Option<Vec<u8>>,)> = sqlx::query_as(
            "SELECT signed_tx_bytes FROM withdrawals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(bytes,)| bytes))
    }

    /// Get total outflow in the last N hours (for circuit breaker).
    ///
    /// Sums amounts of all non-failed, non-cancelled withdrawals created in the window.
    pub async fn outflow_since(
        &self,
        hours: i64,
    ) -> anyhow::Result<Decimal> {
        let row: Option<(Option<Decimal>,)> = sqlx::query_as(
            "SELECT SUM(amount) FROM withdrawals
             WHERE created_at >= NOW() - INTERVAL '1 hour' * $1
             AND status NOT IN ('failed', 'cancelled')",
        )
        .bind(hours)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(s,)| s).unwrap_or(Decimal::ZERO))
    }

    /// Get hourly outflow (last 1 hour).
    pub async fn hourly_outflow(&self) -> anyhow::Result<Decimal> {
        self.outflow_since(1).await
    }

    /// Get daily outflow (last 24 hours).
    pub async fn daily_outflow(&self) -> anyhow::Result<Decimal> {
        self.outflow_since(24).await
    }
}

#[derive(sqlx::FromRow)]
struct WithdrawalRow {
    id: Uuid,
    exchange_withdrawal_id: Uuid,
    asset: String,
    chain: String,
    destination_address: String,
    amount: Decimal,
    fee: Option<Decimal>,
    status: String,
    wallet_tier: String,
    tx_hash: Option<String>,
    block_height: Option<i64>,
    confirmations: i64,
    error_message: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    signed_at: Option<chrono::DateTime<chrono::Utc>>,
    broadcast_at: Option<chrono::DateTime<chrono::Utc>>,
    confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    delay_release_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl WithdrawalRow {
    fn into_withdrawal(self) -> Withdrawal {
        Withdrawal {
            id: self.id,
            exchange_withdrawal_id: self.exchange_withdrawal_id,
            asset: Asset::new(&self.asset),
            chain: chain_from_str(&self.chain),
            destination_address: self.destination_address,
            amount: self.amount,
            fee: self.fee,
            status: match self.status.as_str() {
                "received" => WithdrawalStatus::Received,
                "validated" => WithdrawalStatus::Validated,
                "pendingapproval" => WithdrawalStatus::PendingApproval,
                "approved" => WithdrawalStatus::Approved,
                "signed" => WithdrawalStatus::Signed,
                "broadcast" => WithdrawalStatus::Broadcast,
                "confirming" => WithdrawalStatus::Confirming,
                "finalized" => WithdrawalStatus::Finalized,
                "failed" => WithdrawalStatus::Failed,
                "cancelled" => WithdrawalStatus::Cancelled,
                _ => WithdrawalStatus::Received,
            },
            wallet_tier: match self.wallet_tier.as_str() {
                "warm" => WalletTier::Warm,
                "cold" => WalletTier::Cold,
                _ => WalletTier::Hot,
            },
            tx_hash: self.tx_hash,
            block_height: self.block_height.map(|h| h as u64),
            confirmations: self.confirmations as u64,
            error_message: self.error_message,
            created_at: self.created_at,
            signed_at: self.signed_at,
            broadcast_at: self.broadcast_at,
            confirmed_at: self.confirmed_at,
            delay_release_at: self.delay_release_at,
        }
    }
}
