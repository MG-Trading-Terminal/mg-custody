//! Exchange connection persistence.

use chrono::{DateTime, Utc};
use custody_common::connectivity::{
    ConnectionStatus, ConnectionType, ExchangeConnection,
};
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_connection(&self, c: &ExchangeConnection) -> anyhow::Result<()> {
        let conn_type = serde_json::to_string(&c.connection_type)?;
        let chains: Vec<&str> = c.supported_chains.iter().map(|ch| ch.as_str()).collect();

        sqlx::query(
            "INSERT INTO exchange_connections (id, name, connection_type, status, api_url, supported_chains, auto_credit, created_at, updated_at, last_heartbeat_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (name) DO NOTHING",
        )
        .bind(c.id)
        .bind(&c.name)
        .bind(conn_type.trim_matches('"'))
        .bind(c.status.to_string())
        .bind(&c.api_url)
        .bind(&chains)
        .bind(c.auto_credit)
        .bind(c.created_at)
        .bind(c.updated_at)
        .bind(c.last_heartbeat_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_connection(&self, id: Uuid) -> anyhow::Result<Option<ExchangeConnection>> {
        let row: Option<ConnectionRow> = sqlx::query_as(
            "SELECT id, name, connection_type, status, api_url, supported_chains, auto_credit, created_at, updated_at, last_heartbeat_at
             FROM exchange_connections WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_connection()).transpose()
    }

    pub async fn list_connections(&self) -> anyhow::Result<Vec<ExchangeConnection>> {
        let rows: Vec<ConnectionRow> = sqlx::query_as(
            "SELECT id, name, connection_type, status, api_url, supported_chains, auto_credit, created_at, updated_at, last_heartbeat_at
             FROM exchange_connections ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|r| r.into_connection()).collect()
    }

    pub async fn update_connection_status(&self, id: Uuid, status: &str) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE exchange_connections SET status = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_connection_heartbeat(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE exchange_connections SET last_heartbeat_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_connection(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM exchange_connections WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct ConnectionRow {
    id: Uuid,
    name: String,
    connection_type: String,
    status: String,
    api_url: Option<String>,
    supported_chains: Vec<String>,
    auto_credit: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_heartbeat_at: Option<DateTime<Utc>>,
}

impl ConnectionRow {
    fn into_connection(self) -> anyhow::Result<ExchangeConnection> {
        let connection_type = match self.connection_type.as_str() {
            "otc_desk" => ConnectionType::OtcDesk,
            "custodian" => ConnectionType::Custodian,
            "internal_vault" => ConnectionType::InternalVault,
            _ => ConnectionType::Exchange,
        };
        let status = match self.status.as_str() {
            "active" => ConnectionStatus::Active,
            "inactive" => ConnectionStatus::Inactive,
            "suspended" => ConnectionStatus::Suspended,
            _ => ConnectionStatus::Pending,
        };
        let supported_chains = self.supported_chains.iter()
            .map(|s| Chain::try_parse_str(s)
                .ok_or_else(|| anyhow::anyhow!("exchange_connections row {}: unknown chain '{s}'", self.id)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(ExchangeConnection {
            id: self.id,
            name: self.name,
            connection_type,
            status,
            api_url: self.api_url,
            supported_chains,
            auto_credit: self.auto_credit,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_heartbeat_at: self.last_heartbeat_at,
        })
    }
}
