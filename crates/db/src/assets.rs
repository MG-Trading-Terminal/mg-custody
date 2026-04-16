//! Managed asset persistence (dynamic token registry).

use chrono::{DateTime, Utc};
use custody_common::vault::ManagedAsset;
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn list_assets(&self, chain_filter: Option<&str>) -> anyhow::Result<Vec<ManagedAsset>> {
        let rows: Vec<ManagedAssetRow> = match chain_filter {
            Some(chain) => {
                sqlx::query_as(
                    "SELECT id, symbol, chain, contract_address, decimals, is_native, enabled, added_at
                     FROM managed_assets WHERE chain = $1 ORDER BY symbol",
                )
                .bind(chain)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT id, symbol, chain, contract_address, decimals, is_native, enabled, added_at
                     FROM managed_assets ORDER BY symbol",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(|r| r.into_asset()).collect())
    }

    pub async fn find_asset(&self, symbol: &str, chain: &str) -> anyhow::Result<Option<ManagedAsset>> {
        let row: Option<ManagedAssetRow> = sqlx::query_as(
            "SELECT id, symbol, chain, contract_address, decimals, is_native, enabled, added_at
             FROM managed_assets WHERE UPPER(symbol) = UPPER($1) AND chain = $2",
        )
        .bind(symbol)
        .bind(chain)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_asset()))
    }

    pub async fn find_asset_by_id(&self, id: Uuid) -> anyhow::Result<Option<ManagedAsset>> {
        let row: Option<ManagedAssetRow> = sqlx::query_as(
            "SELECT id, symbol, chain, contract_address, decimals, is_native, enabled, added_at
             FROM managed_assets WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_asset()))
    }

    pub async fn insert_asset(&self, a: &ManagedAsset) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO managed_assets (id, symbol, chain, contract_address, decimals, is_native, enabled, added_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (symbol, chain) DO NOTHING",
        )
        .bind(a.id)
        .bind(&a.symbol)
        .bind(a.chain.as_str())
        .bind(&a.contract_address)
        .bind(a.decimals as i16)
        .bind(a.is_native)
        .bind(a.enabled)
        .bind(a.added_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_asset_enabled(&self, id: Uuid, enabled: bool) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE managed_assets SET enabled = $2 WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_asset(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM managed_assets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct ManagedAssetRow {
    id: Uuid,
    symbol: String,
    chain: String,
    contract_address: Option<String>,
    decimals: i16,
    is_native: bool,
    enabled: bool,
    added_at: DateTime<Utc>,
}

impl ManagedAssetRow {
    fn into_asset(self) -> ManagedAsset {
        ManagedAsset {
            id: self.id,
            symbol: self.symbol,
            chain: Chain::parse_str(&self.chain),
            contract_address: self.contract_address,
            decimals: self.decimals as u8,
            is_native: self.is_native,
            enabled: self.enabled,
            added_at: self.added_at,
        }
    }
}

impl CustodyDb {
    /// Update the cached USD price for a token.
    pub async fn update_token_price(
        &self,
        symbol: &str,
        chain: &str,
        price_usd: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE managed_assets SET last_price_usd = $3, last_price_updated_at = NOW()
             WHERE UPPER(symbol) = UPPER($1) AND chain = $2",
        )
        .bind(symbol)
        .bind(chain)
        .bind(price_usd)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get all tokens with coingecko_id set (for price refresh).
    pub async fn tokens_with_coingecko_id(&self) -> anyhow::Result<Vec<(String, String, String)>> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT symbol, chain, coingecko_id FROM managed_assets
             WHERE coingecko_id IS NOT NULL AND coingecko_id != '' AND enabled = true",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get cached prices for all tokens.
    pub async fn get_token_prices(&self) -> anyhow::Result<Vec<(String, String, Option<rust_decimal::Decimal>)>> {
        let rows: Vec<(String, String, Option<rust_decimal::Decimal>)> = sqlx::query_as(
            "SELECT symbol, chain, last_price_usd FROM managed_assets WHERE enabled = true ORDER BY symbol",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
