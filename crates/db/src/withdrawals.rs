//! Withdrawal repository.

use custody_common::{Withdrawal, WithdrawalStatus};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Insert a new withdrawal request.
    pub async fn insert_withdrawal(&self, w: &Withdrawal) -> anyhow::Result<()> {
        let _ = w;
        // TODO: sqlx insert
        Ok(())
    }

    /// Find withdrawal by exchange's idempotency key.
    pub async fn find_withdrawal_by_exchange_id(
        &self,
        exchange_id: Uuid,
    ) -> anyhow::Result<Option<Withdrawal>> {
        let _ = exchange_id;
        // TODO: sqlx query
        Ok(None)
    }

    /// Find withdrawal by custody ref ID.
    pub async fn find_withdrawal(&self, id: Uuid) -> anyhow::Result<Option<Withdrawal>> {
        let _ = id;
        // TODO: sqlx query
        Ok(None)
    }

    /// Get all withdrawals in a given status.
    pub async fn withdrawals_by_status(
        &self,
        status: WithdrawalStatus,
    ) -> anyhow::Result<Vec<Withdrawal>> {
        let _ = status;
        // TODO: sqlx query
        Ok(vec![])
    }

    /// Update withdrawal status.
    pub async fn update_withdrawal_status(
        &self,
        id: Uuid,
        status: WithdrawalStatus,
    ) -> anyhow::Result<()> {
        let _ = (id, status);
        // TODO: sqlx update
        Ok(())
    }

    /// Set tx_hash after broadcast.
    pub async fn set_withdrawal_tx_hash(
        &self,
        id: Uuid,
        tx_hash: &str,
    ) -> anyhow::Result<()> {
        let _ = (id, tx_hash);
        // TODO: sqlx update
        Ok(())
    }
}
