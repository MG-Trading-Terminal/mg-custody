//! Deployed contract persistence.

use chrono::{DateTime, Utc};
use custody_common::contract_interactions::DeployedContract;
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_deployed_contract(&self, c: &DeployedContract) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO deployed_contracts (id, address, chain, name, abi_hash, deployer_vault_id, deploy_tx_hash, verified, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (address, chain) DO NOTHING",
        )
        .bind(c.id).bind(&c.address).bind(c.chain.as_str())
        .bind(&c.name).bind(&c.abi_hash)
        .bind(c.deployer_vault_id).bind(&c.deploy_tx_hash)
        .bind(c.verified).bind(c.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_deployed_contract(&self, address: &str, chain: &str) -> anyhow::Result<Option<DeployedContract>> {
        let row: Option<(Uuid, String, String, Option<String>, Option<String>, Option<Uuid>, Option<String>, bool, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, address, chain, name, abi_hash, deployer_vault_id, deploy_tx_hash, verified, created_at
             FROM deployed_contracts WHERE address = $1 AND chain = $2",
        ).bind(address).bind(chain).fetch_optional(&self.pool).await?;

        Ok(row.map(|(id, address, chain, name, abi_hash, deployer, tx, verified, created_at)| {
            DeployedContract { id, address, chain: Chain::parse_str(&chain), name, abi_hash, deployer_vault_id: deployer, deploy_tx_hash: tx, verified, created_at }
        }))
    }

    pub async fn list_deployed_contracts(&self, chain_filter: Option<&str>) -> anyhow::Result<Vec<DeployedContract>> {
        let rows: Vec<(Uuid, String, String, Option<String>, Option<String>, Option<Uuid>, Option<String>, bool, DateTime<Utc>)> = match chain_filter {
            Some(chain) => sqlx::query_as(
                "SELECT id, address, chain, name, abi_hash, deployer_vault_id, deploy_tx_hash, verified, created_at
                 FROM deployed_contracts WHERE chain = $1 ORDER BY created_at DESC",
            ).bind(chain).fetch_all(&self.pool).await?,
            None => sqlx::query_as(
                "SELECT id, address, chain, name, abi_hash, deployer_vault_id, deploy_tx_hash, verified, created_at
                 FROM deployed_contracts ORDER BY created_at DESC",
            ).fetch_all(&self.pool).await?,
        };

        Ok(rows.into_iter().map(|(id, address, chain, name, abi_hash, deployer, tx, verified, created_at)| {
            DeployedContract { id, address, chain: Chain::parse_str(&chain), name, abi_hash, deployer_vault_id: deployer, deploy_tx_hash: tx, verified, created_at }
        }).collect())
    }
}
