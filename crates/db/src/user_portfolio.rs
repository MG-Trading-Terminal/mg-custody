//! User-facing portfolio and activity DB helpers.
//!
//! Provides aggregated balance lookups across vault accounts, per-user deposit
//! history, and a lightweight activity feed drawn from deposits and withdrawals.
//! All queries are scoped to a single user_id — no cross-user data leakage.

use chrono::{DateTime, Utc};
use custody_common::{Asset, Deposit, DepositStatus};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;
use crate::addresses::chain_from_str;

// ---------------------------------------------------------------------------
// Aggregated balance per (asset, chain)
// ---------------------------------------------------------------------------

/// A single row from the aggregated vault-account balance query.
#[derive(Debug)]
pub struct UserBalance {
    pub asset: String,
    pub chain: String,
    pub balance: Decimal,
}

// ---------------------------------------------------------------------------
// Activity feed event
// ---------------------------------------------------------------------------

/// A lightweight activity event for the user activity feed.
#[derive(Debug)]
pub struct ActivityEvent {
    pub event_type: String,
    pub asset: String,
    pub chain: String,
    pub amount: Decimal,
    pub tx_hash: Option<String>,
    pub at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// User profile
// ---------------------------------------------------------------------------

/// Minimal profile data returned to the UI.
#[derive(Debug)]
pub struct UserProfile {
    pub user_id: Uuid,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// CustodyDb impl
// ---------------------------------------------------------------------------

impl CustodyDb {
    /// Sum vault account balances for all vaults owned by `user_id`,
    /// grouped by (asset, chain). Excludes frozen accounts.
    pub async fn user_aggregated_balances(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<UserBalance>> {
        let rows: Vec<(String, String, Decimal)> = sqlx::query_as(
            "SELECT va.asset, va.chain, SUM(va.balance) AS total
             FROM vault_accounts va
             JOIN vaults v ON va.vault_id = v.id
             WHERE v.owner_id = $1
               AND va.frozen = false
             GROUP BY va.asset, va.chain
             ORDER BY va.asset, va.chain",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(asset, chain, balance)| UserBalance { asset, chain, balance })
            .collect())
    }

    /// List deposits for a specific user, newest first, capped at `limit`.
    pub async fn list_deposits_by_user(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<Deposit>> {
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
            detected_at: DateTime<Utc>,
            confirmed_at: Option<DateTime<Utc>>,
            credited_at: Option<DateTime<Utc>>,
        }

        let rows: Vec<DepositRow> = sqlx::query_as(
            "SELECT id, user_id, asset, chain, address, amount, tx_hash, log_index,
                    block_height, confirmations, required_confirmations, status,
                    detected_at, confirmed_at, credited_at
             FROM deposits
             WHERE user_id = $1
             ORDER BY detected_at DESC
             LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Deposit {
                id: r.id,
                user_id: r.user_id,
                asset: Asset::new(&r.asset),
                chain: chain_from_str(&r.chain),
                address: r.address,
                amount: r.amount,
                tx_hash: r.tx_hash,
                log_index: r.log_index.map(|i| i as u32),
                block_height: r.block_height as u64,
                confirmations: r.confirmations as u64,
                required_confirmations: r.required_confirmations as u64,
                status: match r.status.as_str() {
                    "confirmed" => DepositStatus::Confirmed,
                    "credited" => DepositStatus::Credited,
                    "failed" => DepositStatus::Failed,
                    _ => DepositStatus::Detected,
                },
                detected_at: r.detected_at,
                confirmed_at: r.confirmed_at,
                credited_at: r.credited_at,
                memo: None,
                source_memo: None,
            })
            .collect())
    }

    /// Return the last `limit` activity events (deposits + withdrawals) for a user,
    /// merged and sorted newest-first.
    pub async fn user_activity_feed(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<ActivityEvent>> {
        // Deposit events: direct user_id column on deposits table.
        let deposit_rows: Vec<(String, String, Decimal, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT asset, chain, amount, tx_hash, detected_at
             FROM deposits
             WHERE user_id = $1
             ORDER BY detected_at DESC
             LIMIT $2",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        // Withdrawal events: identify by vault ownership.
        // Withdrawals reference destination_address which may be external,
        // so we use the vault owner as the subject when source vault is owned by user.
        // We fall back to an empty set if the join returns nothing.
        let withdrawal_rows: Vec<(String, String, Decimal, Option<String>, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT w.asset, w.chain, w.amount, w.tx_hash, w.created_at
                 FROM withdrawals w
                 JOIN vault_accounts va ON va.address = w.destination_address
                 JOIN vaults v ON va.vault_id = v.id
                 WHERE v.owner_id = $1
                 ORDER BY w.created_at DESC
                 LIMIT $2",
            )
            .bind(user_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let mut events: Vec<ActivityEvent> = Vec::new();

        for (asset, chain, amount, tx_hash, at) in deposit_rows {
            events.push(ActivityEvent {
                event_type: "deposit_detected".to_string(),
                asset,
                chain,
                amount,
                tx_hash: Some(tx_hash),
                at,
            });
        }

        for (asset, chain, amount, tx_hash, at) in withdrawal_rows {
            events.push(ActivityEvent {
                event_type: "withdrawal_submitted".to_string(),
                asset,
                chain,
                amount,
                tx_hash,
                at,
            });
        }

        events.sort_by(|a, b| b.at.cmp(&a.at));
        events.truncate(limit as usize);
        Ok(events)
    }

    /// Load profile data for the authenticated user.
    pub async fn load_user_profile(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Option<UserProfile>> {
        let row: Option<(Uuid, String, String, DateTime<Utc>, Option<DateTime<Utc>>)> =
            sqlx::query_as(
                "SELECT id, email, role, created_at, last_login_at
                 FROM workspace_users
                 WHERE id = $1",
            )
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|(user_id, email, role, created_at, last_login_at)| UserProfile {
            user_id,
            email,
            role,
            created_at,
            last_login_at,
        }))
    }

    /// Load the current password hash for a user (for change-password verification).
    pub async fn get_user_password_hash(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT password_hash FROM workspace_users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|(h,)| h))
    }
}
