//! Deposit address repository.

use custody_common::{Asset, Chain, DepositAddress};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Look up an existing deposit address for a user+asset+chain.
    pub async fn find_address(
        &self,
        user_id: Uuid,
        asset: &Asset,
        chain: Chain,
    ) -> anyhow::Result<Option<DepositAddress>> {
        // TODO: sqlx query
        let _ = (user_id, asset, chain);
        Ok(None)
    }

    /// Insert a newly generated deposit address.
    pub async fn insert_address(&self, addr: &DepositAddress) -> anyhow::Result<()> {
        let _ = addr;
        // TODO: sqlx insert
        Ok(())
    }

    /// Get all known deposit addresses for a chain (used by scanner).
    pub async fn all_addresses_for_chain(&self, chain: Chain) -> anyhow::Result<Vec<String>> {
        let _ = chain;
        // TODO: sqlx query
        Ok(vec![])
    }
}
