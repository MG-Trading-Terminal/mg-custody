//! Persistent nonce counters for Ethereum-style accounts.

use crate::CustodyDb;

impl CustodyDb {
    /// Get the current nonce for an address.
    pub async fn get_nonce(&self, address: &str) -> anyhow::Result<Option<u64>> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT nonce FROM nonce_counters WHERE address = $1",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(n,)| n as u64))
    }

    /// Set the nonce for an address (used for initial sync from chain).
    pub async fn set_nonce(&self, address: &str, nonce: u64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO nonce_counters (address, nonce, updated_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (address) DO UPDATE SET nonce = $2, updated_at = NOW()",
        )
        .bind(address)
        .bind(nonce as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Atomically increment the nonce and return the previous value.
    pub async fn increment_nonce(&self, address: &str) -> anyhow::Result<u64> {
        let (prev,): (i64,) = sqlx::query_as(
            "UPDATE nonce_counters
             SET nonce = nonce + 1, updated_at = NOW()
             WHERE address = $1
             RETURNING nonce - 1",
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;

        Ok(prev as u64)
    }
}
