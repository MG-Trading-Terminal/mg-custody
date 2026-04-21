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
    /// Insert a freshly-derived address. Returns `Ok(true)` when the
    /// row landed, `Ok(false)` when a concurrent caller beat us to
    /// it (ON CONFLICT DO NOTHING skipped this one).
    ///
    /// Previously always returned `Ok(())` regardless of whether
    /// anything was inserted — so the allocator's `get_or_create`
    /// couldn't tell whether the local `DepositAddress` it built
    /// actually matched what ended up in the DB. On race the loser
    /// returned its locally-derived address (from a wasted HD
    /// index) while the DB held the winner's — users deposited to
    /// the unscanned loser address and the funds were lost.
    pub async fn insert_address(&self, addr: &DepositAddress) -> anyhow::Result<bool> {
        let result = sqlx::query(
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

        Ok(result.rows_affected() > 0)
    }

    /// Look up a Bitcoin address in the `addresses` table and return
    /// the stored derivation-path string. The consolidation worker
    /// uses this to resolve each UTXO's signing key; `None` means the
    /// address is unknown to us (e.g., a Bitcoin Core wallet imported
    /// it manually) and we must refuse to sign.
    pub async fn find_derivation_path_for_btc_address(
        &self,
        address: &str,
    ) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT derivation_path FROM addresses
             WHERE address = $1 AND chain = 'bitcoin'",
        )
        .bind(address)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(p,)| p))
    }

    /// Find the user's deposit address for the native asset of `chain`.
    ///
    /// Used by the NFT transfer builder: ERC-721/1155 balances live at
    /// the user's EVM address (which is the same derivation as ETH),
    /// not in a separate per-NFT address, so "where does this NFT
    /// live?" maps onto "what's this user's native-asset address on
    /// this chain?"
    pub async fn find_native_address_for_chain(
        &self,
        user_id: Uuid,
        chain: Chain,
    ) -> anyhow::Result<Option<String>> {
        let native_symbol = match chain {
            Chain::Bitcoin => "BTC",
            Chain::Ethereum => "ETH",
            Chain::Tron => "TRX",
            Chain::Solana => "SOL",
        };
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT address FROM addresses
             WHERE user_id = $1 AND asset = $2 AND chain = $3",
        )
        .bind(user_id)
        .bind(native_symbol)
        .bind(chain.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(a,)| a))
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
