//! Bitcoin UTXO management in PostgreSQL.

use anyhow::Result;
use uuid::Uuid;

use crate::CustodyDb;

/// A row from the utxos table.
#[derive(Debug, Clone)]
pub struct UtxoRow {
    pub id: Uuid,
    pub txid: String,
    pub vout: i32,
    pub amount_sats: i64,
    pub address: String,
    pub confirmations: i64,
    pub locked: bool,
}

impl CustodyDb {
    /// Get all unspent, unlocked UTXOs for Bitcoin.
    pub async fn get_unspent_utxos(&self) -> Result<Vec<UtxoRow>> {
        let rows: Vec<(Uuid, String, i32, i64, String, i64, bool)> = sqlx::query_as(
            "SELECT id, txid, vout, amount_sats, address, confirmations, locked
             FROM utxos
             WHERE spending_tx IS NULL AND locked = FALSE
             ORDER BY amount_sats DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(id, txid, vout, amount_sats, address, confirmations, locked)| {
            UtxoRow { id, txid, vout, amount_sats, address, confirmations, locked }
        }).collect())
    }

    /// Lock UTXOs for spending (prevents double-spend during signing).
    pub async fn lock_utxos(&self, utxo_ids: &[Uuid]) -> Result<()> {
        for id in utxo_ids {
            sqlx::query("UPDATE utxos SET locked = TRUE WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Mark UTXOs as spent.
    pub async fn spend_utxos(&self, utxo_ids: &[Uuid], spending_tx: &str) -> Result<()> {
        for id in utxo_ids {
            sqlx::query("UPDATE utxos SET spending_tx = $1, locked = TRUE WHERE id = $2")
                .bind(spending_tx)
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    /// Insert a new UTXO (from scanner or change output).
    pub async fn insert_utxo(
        &self,
        id: Uuid,
        txid: &str,
        vout: i32,
        amount_sats: i64,
        address: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO utxos (id, txid, vout, amount_sats, address, confirmations, locked)
             VALUES ($1, $2, $3, $4, $5, 0, FALSE)
             ON CONFLICT (txid, vout) DO NOTHING"
        )
        .bind(id)
        .bind(txid)
        .bind(vout)
        .bind(amount_sats)
        .bind(address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Unlock UTXOs (if signing failed, release locks).
    pub async fn unlock_utxos(&self, utxo_ids: &[Uuid]) -> Result<()> {
        for id in utxo_ids {
            sqlx::query("UPDATE utxos SET locked = FALSE WHERE id = $1 AND spending_tx IS NULL")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }
}
