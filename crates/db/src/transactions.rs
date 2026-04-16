//! Unified transaction history queries.
//!
//! Provides a single view across deposits, withdrawals, and internal transfers
//! via UNION queries with cursor-based pagination.

use chrono::{DateTime, Utc};
use custody_common::vault::{Transaction, TransactionFilter, TransactionType};
use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// List transactions with filtering and cursor pagination.
    ///
    /// Returns (transactions, next_cursor). The cursor is `created_at:id` for keyset pagination.
    pub async fn list_transactions(
        &self,
        filter: &TransactionFilter,
    ) -> anyhow::Result<(Vec<Transaction>, Option<String>)> {
        let limit = filter.effective_limit() as i64;
        // Fetch one extra to determine if there's a next page.
        let _fetch_limit = limit + 1;

        // Query each table separately and merge in Rust.
        // This avoids complex dynamic SQL while keeping correctness.
        let mut all_txs = Vec::new();

        let include_deposits = filter.tx_type.is_none()
            || filter.tx_type == Some(TransactionType::Deposit);
        let include_withdrawals = filter.tx_type.is_none()
            || filter.tx_type == Some(TransactionType::Withdrawal);
        let include_transfers = filter.tx_type.is_none()
            || filter.tx_type == Some(TransactionType::InternalTransfer);

        if include_deposits {
            let deposits = self.query_deposits_as_transactions(filter).await?;
            all_txs.extend(deposits);
        }

        if include_withdrawals {
            let withdrawals = self.query_withdrawals_as_transactions(filter).await?;
            all_txs.extend(withdrawals);
        }

        if include_transfers {
            let transfers = self.query_transfers_as_transactions(filter).await?;
            all_txs.extend(transfers);
        }

        // Sort by created_at DESC, then id for stable ordering.
        all_txs.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));

        // Apply cursor: skip until we pass the cursor position.
        if let Some(ref cursor) = filter.cursor
            && let Some((cursor_time, cursor_id)) = parse_cursor(cursor) {
                all_txs.retain(|tx| {
                    (tx.created_at, tx.id) < (cursor_time, cursor_id)
                });
            }

        // Paginate
        let has_more = all_txs.len() > limit as usize;
        all_txs.truncate(limit as usize);

        let next_cursor = if has_more {
            all_txs.last().map(|tx| encode_cursor(tx.created_at, tx.id))
        } else {
            None
        };

        Ok((all_txs, next_cursor))
    }

    /// Find a single transaction by ID across all tables.
    pub async fn find_transaction(&self, id: Uuid) -> anyhow::Result<Option<Transaction>> {
        // Try deposits first
        let row: Option<(Uuid, String, String, String, Decimal, Option<String>, String, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, asset, chain, address, amount, tx_hash, status::text, detected_at
                 FROM deposits WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some((id, asset, chain, addr, amount, tx_hash, status, created_at)) = row {
            return Ok(Some(Transaction {
                id, tx_type: TransactionType::Deposit,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee: None, status,
                source_vault_id: None, dest_vault_id: None,
                source_address: Some(addr.clone()), dest_address: Some(addr),
                tx_hash, created_at, updated_at: None,
            }));
        }

        // Try withdrawals
        let row: Option<(Uuid, String, String, String, Decimal, Option<Decimal>, Option<String>, String, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, asset, chain, destination_address, amount, fee, tx_hash, status::text, created_at
                 FROM withdrawals WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some((id, asset, chain, dest, amount, fee, tx_hash, status, created_at)) = row {
            return Ok(Some(Transaction {
                id, tx_type: TransactionType::Withdrawal,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee, status,
                source_vault_id: None, dest_vault_id: None,
                source_address: None, dest_address: Some(dest),
                tx_hash, created_at, updated_at: None,
            }));
        }

        // Try internal transfers
        let row: Option<(Uuid, Uuid, Uuid, String, String, Decimal, String, DateTime<Utc>, Option<DateTime<Utc>>)> =
            sqlx::query_as(
                "SELECT id, source_vault_id, dest_vault_id, asset, chain, amount, status, created_at, completed_at
                 FROM internal_transfers WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some((id, src, dst, asset, chain, amount, status, created_at, completed_at)) = row {
            return Ok(Some(Transaction {
                id, tx_type: TransactionType::InternalTransfer,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee: None, status,
                source_vault_id: Some(src), dest_vault_id: Some(dst),
                source_address: None, dest_address: None,
                tx_hash: None, created_at, updated_at: completed_at,
            }));
        }

        Ok(None)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    async fn query_deposits_as_transactions(
        &self,
        filter: &TransactionFilter,
    ) -> anyhow::Result<Vec<Transaction>> {
        // Base query — we apply simple filters via Rust since sqlx doesn't support dynamic WHERE
        let rows: Vec<(Uuid, String, String, String, Decimal, Option<String>, String, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, asset, chain, address, amount, tx_hash, status::text, detected_at
                 FROM deposits ORDER BY detected_at DESC LIMIT 500",
            )
            .fetch_all(&self.pool)
            .await?;

        let txs: Vec<Transaction> = rows
            .into_iter()
            .map(|(id, asset, chain, addr, amount, tx_hash, status, created_at)| Transaction {
                id, tx_type: TransactionType::Deposit,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee: None, status,
                source_vault_id: None, dest_vault_id: None,
                source_address: Some(addr.clone()), dest_address: Some(addr),
                tx_hash, created_at, updated_at: None,
            })
            .filter(|tx| apply_filter(tx, filter))
            .collect();
        Ok(txs)
    }

    async fn query_withdrawals_as_transactions(
        &self,
        filter: &TransactionFilter,
    ) -> anyhow::Result<Vec<Transaction>> {
        let rows: Vec<(Uuid, String, String, String, Decimal, Option<Decimal>, Option<String>, String, DateTime<Utc>)> =
            sqlx::query_as(
                "SELECT id, asset, chain, destination_address, amount, fee, tx_hash, status::text, created_at
                 FROM withdrawals ORDER BY created_at DESC LIMIT 500",
            )
            .fetch_all(&self.pool)
            .await?;

        let txs: Vec<Transaction> = rows
            .into_iter()
            .map(|(id, asset, chain, dest, amount, fee, tx_hash, status, created_at)| Transaction {
                id, tx_type: TransactionType::Withdrawal,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee, status,
                source_vault_id: None, dest_vault_id: None,
                source_address: None, dest_address: Some(dest),
                tx_hash, created_at, updated_at: None,
            })
            .filter(|tx| apply_filter(tx, filter))
            .collect();
        Ok(txs)
    }

    async fn query_transfers_as_transactions(
        &self,
        filter: &TransactionFilter,
    ) -> anyhow::Result<Vec<Transaction>> {
        let rows: Vec<(Uuid, Uuid, Uuid, String, String, Decimal, String, DateTime<Utc>, Option<DateTime<Utc>>)> =
            sqlx::query_as(
                "SELECT id, source_vault_id, dest_vault_id, asset, chain, amount, status, created_at, completed_at
                 FROM internal_transfers ORDER BY created_at DESC LIMIT 500",
            )
            .fetch_all(&self.pool)
            .await?;

        let txs: Vec<Transaction> = rows
            .into_iter()
            .map(|(id, src, dst, asset, chain, amount, status, created_at, completed_at)| Transaction {
                id, tx_type: TransactionType::InternalTransfer,
                asset: Asset::new(&asset), chain: Chain::parse_str(&chain),
                amount, fee: None, status,
                source_vault_id: Some(src), dest_vault_id: Some(dst),
                source_address: None, dest_address: None,
                tx_hash: None, created_at, updated_at: completed_at,
            })
            .filter(|tx| apply_filter(tx, filter))
            .collect();
        Ok(txs)
    }
}

/// Apply in-memory filters to a transaction.
fn apply_filter(tx: &Transaction, filter: &TransactionFilter) -> bool {
    if let Some(ref asset) = filter.asset
        && tx.asset.as_str() != asset {
            return false;
        }
    if let Some(ref chain) = filter.chain
        && tx.chain.as_str() != chain {
            return false;
        }
    if let Some(ref status) = filter.status
        && tx.status != *status {
            return false;
        }
    if let Some(ref after) = filter.after
        && tx.created_at < *after {
            return false;
        }
    if let Some(ref before) = filter.before
        && tx.created_at > *before {
            return false;
        }
    if let Some(ref min) = filter.min_amount
        && tx.amount < *min {
            return false;
        }
    if let Some(ref max) = filter.max_amount
        && tx.amount > *max {
            return false;
        }
    if let Some(vault_id) = filter.vault_id {
        let matches = tx.source_vault_id == Some(vault_id)
            || tx.dest_vault_id == Some(vault_id);
        if !matches {
            return false;
        }
    }
    true
}

fn encode_cursor(time: DateTime<Utc>, id: Uuid) -> String {
    format!("{}:{}", time.timestamp_millis(), id)
}

fn parse_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let mut parts = cursor.splitn(2, ':');
    let millis: i64 = parts.next()?.parse().ok()?;
    let id: Uuid = parts.next()?.parse().ok()?;
    let time = DateTime::from_timestamp_millis(millis)?;
    Some((time, id))
}
