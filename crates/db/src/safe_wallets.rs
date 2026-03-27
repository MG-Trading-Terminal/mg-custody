//! Gnosis Safe wallet and transaction persistence.
//!
//! Two tables:
//! - safe_wallets: registered Safe proxy addresses with threshold + owners
//! - safe_transactions: proposed multi-sig transactions with status + signatures

use chrono::{DateTime, Utc};
use custody_common::safe_executor::{SafeManagedTx, SafeOperation, SafeSignature, SafeTxStatus, SafeWallet};
use custody_common::Chain;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    // -----------------------------------------------------------------------
    // Safe Wallets
    // -----------------------------------------------------------------------

    /// Insert or update a Safe wallet registration.
    pub async fn upsert_safe_wallet(&self, w: &SafeWallet) -> anyhow::Result<()> {
        let owners_json = serde_json::to_value(&w.owners)
            .unwrap_or(serde_json::json!([]));

        sqlx::query(
            "INSERT INTO safe_wallets (address, chain, threshold, owners, nonce, created_at)
             VALUES ($1, $2, $3, $4, $5, NOW())
             ON CONFLICT (address) DO UPDATE
             SET chain = EXCLUDED.chain,
                 threshold = EXCLUDED.threshold,
                 owners = EXCLUDED.owners,
                 nonce = EXCLUDED.nonce",
        )
        .bind(&w.address)
        .bind(w.chain.as_str())
        .bind(w.threshold as i32)
        .bind(&owners_json)
        .bind(w.nonce as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Find a Safe wallet by its proxy address (case-insensitive).
    pub async fn find_safe_wallet(&self, address: &str) -> anyhow::Result<Option<SafeWallet>> {
        let row: Option<SafeWalletRow> = sqlx::query_as(
            "SELECT address, chain, threshold, owners, nonce
             FROM safe_wallets WHERE lower(address) = lower($1)",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(row_to_safe_wallet))
    }

    /// List all registered Safe wallets.
    pub async fn list_safe_wallets(&self) -> anyhow::Result<Vec<SafeWallet>> {
        let rows: Vec<SafeWalletRow> = sqlx::query_as(
            "SELECT address, chain, threshold, owners, nonce
             FROM safe_wallets ORDER BY address ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(row_to_safe_wallet).collect())
    }

    // -----------------------------------------------------------------------
    // Safe Transactions
    // -----------------------------------------------------------------------

    /// Insert a newly proposed Safe transaction.
    pub async fn insert_safe_tx(&self, tx: &SafeManagedTx) -> anyhow::Result<()> {
        let sigs_json = serde_json::to_value(&tx.signatures)
            .unwrap_or(serde_json::json!([]));
        let status_str = status_to_str(tx.status);
        let op_str = op_to_str(tx.operation);

        sqlx::query(
            "INSERT INTO safe_transactions
             (id, safe_address, to_address, value_wei, data_bytes,
              operation, safe_tx_hash, signatures, status, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(tx.id)
        .bind(&tx.safe_address)
        .bind(&tx.to)
        .bind(tx.value.to_string())
        .bind(&tx.data)
        .bind(&op_str)
        .bind(&tx.safe_tx_hash)
        .bind(&sigs_json)
        .bind(&status_str)
        .bind(tx.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update an existing Safe transaction (signatures + status).
    pub async fn update_safe_tx(&self, tx: &SafeManagedTx) -> anyhow::Result<()> {
        let sigs_json = serde_json::to_value(&tx.signatures)
            .unwrap_or(serde_json::json!([]));
        let status_str = status_to_str(tx.status);

        sqlx::query(
            "UPDATE safe_transactions
             SET signatures = $1, status = $2
             WHERE id = $3",
        )
        .bind(&sigs_json)
        .bind(&status_str)
        .bind(tx.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Find a Safe transaction by ID.
    pub async fn find_safe_tx(&self, id: Uuid) -> anyhow::Result<Option<SafeManagedTx>> {
        let row: Option<SafeTxRow> = sqlx::query_as(
            "SELECT id, safe_address, to_address, value_wei, data_bytes,
              operation, safe_tx_hash, signatures, status, created_at
             FROM safe_transactions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_safe_tx).transpose()
    }

    /// List all transactions for a given Safe address, newest first.
    pub async fn list_safe_txs(&self, safe_address: &str) -> anyhow::Result<Vec<SafeManagedTx>> {
        let rows: Vec<SafeTxRow> = sqlx::query_as(
            "SELECT id, safe_address, to_address, value_wei, data_bytes,
              operation, safe_tx_hash, signatures, status, created_at
             FROM safe_transactions
             WHERE lower(safe_address) = lower($1)
             ORDER BY created_at DESC",
        )
        .bind(safe_address)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_safe_tx).collect()
    }
}

// ---------------------------------------------------------------------------
// Private row types + converters
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SafeWalletRow {
    address: String,
    chain: String,
    threshold: i32,
    owners: serde_json::Value,
    nonce: i64,
}

fn row_to_safe_wallet(r: SafeWalletRow) -> SafeWallet {
    use crate::addresses::chain_from_str;
    let owners: Vec<String> = r.owners
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    SafeWallet {
        address: r.address,
        chain: chain_from_str(&r.chain),
        threshold: r.threshold as u32,
        owners,
        nonce: r.nonce as u64,
    }
}

#[derive(sqlx::FromRow)]
struct SafeTxRow {
    id: Uuid,
    safe_address: String,
    to_address: String,
    value_wei: String,
    data_bytes: Vec<u8>,
    operation: String,
    safe_tx_hash: String,
    signatures: serde_json::Value,
    status: String,
    created_at: DateTime<Utc>,
}

fn row_to_safe_tx(r: SafeTxRow) -> anyhow::Result<SafeManagedTx> {
    let value = Decimal::from_str(&r.value_wei)
        .map_err(|e| anyhow::anyhow!("bad value_wei for safe_tx {}: {}", r.id, e))?;
    let operation = str_to_op(&r.operation)?;
    let status = str_to_status(&r.status)?;
    let signatures: Vec<SafeSignature> = serde_json::from_value(r.signatures)
        .unwrap_or_default();

    Ok(SafeManagedTx {
        id: r.id,
        safe_address: r.safe_address,
        to: r.to_address,
        value,
        data: r.data_bytes,
        operation,
        safe_tx_hash: r.safe_tx_hash,
        signatures,
        status,
        created_at: r.created_at,
    })
}

fn status_to_str(s: SafeTxStatus) -> &'static str {
    match s {
        SafeTxStatus::Proposed  => "proposed",
        SafeTxStatus::Signing   => "signing",
        SafeTxStatus::Ready     => "ready",
        SafeTxStatus::Executed  => "executed",
        SafeTxStatus::Failed    => "failed",
    }
}

fn str_to_status(s: &str) -> anyhow::Result<SafeTxStatus> {
    match s {
        "proposed"  => Ok(SafeTxStatus::Proposed),
        "signing"   => Ok(SafeTxStatus::Signing),
        "ready"     => Ok(SafeTxStatus::Ready),
        "executed"  => Ok(SafeTxStatus::Executed),
        "failed"    => Ok(SafeTxStatus::Failed),
        other => Err(anyhow::anyhow!("Unknown safe_tx status: {other}")),
    }
}

fn op_to_str(op: SafeOperation) -> &'static str {
    match op {
        SafeOperation::Call         => "call",
        SafeOperation::DelegateCall => "delegate_call",
    }
}

fn str_to_op(s: &str) -> anyhow::Result<SafeOperation> {
    match s {
        "call"          => Ok(SafeOperation::Call),
        "delegate_call" => Ok(SafeOperation::DelegateCall),
        other => Err(anyhow::anyhow!("Unknown safe operation: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for s in [
            SafeTxStatus::Proposed, SafeTxStatus::Signing, SafeTxStatus::Ready,
            SafeTxStatus::Executed, SafeTxStatus::Failed,
        ] {
            assert_eq!(str_to_status(status_to_str(s)).unwrap(), s);
        }
    }

    #[test]
    fn op_roundtrip() {
        for op in [SafeOperation::Call, SafeOperation::DelegateCall] {
            assert_eq!(str_to_op(op_to_str(op)).unwrap(), op);
        }
    }

    #[test]
    fn unknown_status_errors() {
        assert!(str_to_status("unknown_xyz").is_err());
    }

    #[test]
    fn unknown_op_errors() {
        assert!(str_to_op("staticcall").is_err());
    }
}
