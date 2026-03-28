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
        let chain_str = chain.as_str();
        let asset_str = asset.as_str();

        let row: Option<AddressRow> = sqlx::query_as(
            "SELECT id, user_id, asset, chain, address, derivation_path, created_at
             FROM addresses
             WHERE user_id = $1 AND asset = $2 AND chain = $3",
        )
        .bind(user_id)
        .bind(asset_str)
        .bind(chain_str)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_deposit_address()))
    }

    /// Insert a newly generated deposit address.
    pub async fn insert_address(&self, addr: &DepositAddress) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO addresses (id, user_id, asset, chain, address, derivation_path, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, asset, chain) DO NOTHING",
        )
        .bind(addr.id)
        .bind(addr.user_id)
        .bind(addr.asset.as_str())
        .bind(addr.chain.as_str())
        .bind(&addr.address)
        .bind(&addr.derivation_path)
        .bind(addr.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all known deposit addresses for a chain (used by scanner).
    pub async fn all_addresses_for_chain(&self, chain: Chain) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT address FROM addresses WHERE chain = $1",
        )
        .bind(chain.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(addr,)| addr).collect())
    }

    /// Get all (address, user_id) pairs for a chain (used by scanner address cache).
    pub async fn addresses_with_users(&self, chain: Chain) -> anyhow::Result<Vec<(String, Uuid)>> {
        let rows: Vec<(String, Uuid)> = sqlx::query_as(
            "SELECT address, user_id FROM addresses WHERE chain = $1",
        )
        .bind(chain.as_str())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Check whether an address string already exists in the `addresses` table.
    ///
    /// Used by the derivation explorer to mark addresses as "used".
    pub async fn address_exists(&self, address: &str) -> anyhow::Result<bool> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM addresses WHERE address = $1"
        )
        .bind(address)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    /// Atomically allocate the next address index for a chain.
    pub async fn next_address_index(&self, chain: Chain) -> anyhow::Result<u32> {
        let (index,): (i32,) = sqlx::query_as(
            "UPDATE address_counters
             SET next_index = next_index + 1
             WHERE chain = $1
             RETURNING next_index - 1",
        )
        .bind(chain.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(index as u32)
    }
}

/// Internal row type for sqlx mapping.
#[derive(sqlx::FromRow)]
struct AddressRow {
    id: Uuid,
    user_id: Uuid,
    asset: String,
    chain: String,
    address: String,
    derivation_path: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl AddressRow {
    fn into_deposit_address(self) -> DepositAddress {
        DepositAddress {
            id: self.id,
            user_id: self.user_id,
            asset: Asset::new(&self.asset),
            chain: chain_from_str(&self.chain),
            address: self.address,
            derivation_path: self.derivation_path,
            created_at: self.created_at,
        }
    }
}

/// Parse chain string to Chain enum.
pub(crate) fn chain_from_str(s: &str) -> Chain {
    match s {
        "bitcoin" => Chain::Bitcoin,
        "ethereum" => Chain::Ethereum,
        "tron" => Chain::Tron,
        "solana" => Chain::Solana,
        _ => Chain::Ethereum,
    }
}
