//! Deposit repository.

use custody_common::{Deposit, DepositStatus};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Insert a newly detected deposit.
    pub async fn insert_deposit(&self, deposit: &Deposit) -> anyhow::Result<()> {
        let _ = deposit;
        // TODO: sqlx insert
        Ok(())
    }

    /// Find deposit by tx_hash (deduplication).
    pub async fn find_deposit_by_tx_hash(&self, tx_hash: &str) -> anyhow::Result<Option<Deposit>> {
        let _ = tx_hash;
        // TODO: sqlx query
        Ok(None)
    }

    /// Get all deposits in a given status.
    pub async fn deposits_by_status(&self, status: DepositStatus) -> anyhow::Result<Vec<Deposit>> {
        let _ = status;
        // TODO: sqlx query
        Ok(vec![])
    }

    /// Update deposit status and confirmations.
    pub async fn update_deposit_status(
        &self,
        id: Uuid,
        status: DepositStatus,
        confirmations: u64,
    ) -> anyhow::Result<()> {
        let _ = (id, status, confirmations);
        // TODO: sqlx update
        Ok(())
    }
}
