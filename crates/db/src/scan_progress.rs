//! Scan progress tracking per chain.

use custody_common::Chain;
use crate::CustodyDb;

impl CustodyDb {
    /// Get the last scanned block height for a chain.
    pub async fn get_scan_progress(&self, chain: Chain) -> anyhow::Result<Option<u64>> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT last_block FROM scan_progress WHERE chain = $1",
        )
        .bind(chain.as_str())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(h,)| h as u64))
    }

    /// Set the last scanned block height for a chain.
    pub async fn set_scan_progress(&self, chain: Chain, block_height: u64) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO scan_progress (chain, last_block, updated_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (chain) DO UPDATE SET last_block = $2, updated_at = NOW()",
        )
        .bind(chain.as_str())
        .bind(block_height as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
