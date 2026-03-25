//! API key persistence.

use chrono::{DateTime, Utc};
use custody_common::connectivity::{ApiKey, ApiKeyRole, ApiPermission};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_api_key(&self, key: &ApiKey) -> anyhow::Result<()> {
        let perms: Vec<String> = key.permissions.iter().map(|p| p.to_string()).collect();
        sqlx::query(
            "INSERT INTO api_keys (id, name, key_hash, role, permissions, ip_whitelist, rate_limit_per_minute, enabled, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(key.id)
        .bind(&key.name)
        .bind(&key.key_hash)
        .bind(key.role.to_string())
        .bind(&perms)
        .bind(&key.ip_whitelist)
        .bind(key.rate_limit_per_minute as i32)
        .bind(key.enabled)
        .bind(key.created_at)
        .bind(key.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_api_key_by_hash(&self, key_hash: &str) -> anyhow::Result<Option<ApiKey>> {
        let row: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, name, key_hash, role, permissions, ip_whitelist, rate_limit_per_minute, enabled, created_at, last_used_at, expires_at
             FROM api_keys WHERE key_hash = $1",
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_api_key()))
    }

    pub async fn find_api_key(&self, id: Uuid) -> anyhow::Result<Option<ApiKey>> {
        let row: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, name, key_hash, role, permissions, ip_whitelist, rate_limit_per_minute, enabled, created_at, last_used_at, expires_at
             FROM api_keys WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into_api_key()))
    }

    pub async fn list_api_keys(&self) -> anyhow::Result<Vec<ApiKey>> {
        let rows: Vec<ApiKeyRow> = sqlx::query_as(
            "SELECT id, name, key_hash, role, permissions, ip_whitelist, rate_limit_per_minute, enabled, created_at, last_used_at, expires_at
             FROM api_keys ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_api_key()).collect())
    }

    pub async fn update_api_key_last_used(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn revoke_api_key(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE api_keys SET enabled = false WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_api_key(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    name: String,
    key_hash: String,
    role: String,
    permissions: Vec<String>,
    ip_whitelist: Vec<String>,
    rate_limit_per_minute: i32,
    enabled: bool,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
}

impl ApiKeyRow {
    fn into_api_key(self) -> ApiKey {
        let role = match self.role.as_str() {
            "admin" => ApiKeyRole::Admin,
            "signer" => ApiKeyRole::Signer,
            "trader" => ApiKeyRole::Trader,
            _ => ApiKeyRole::Viewer,
        };
        let permissions: Vec<ApiPermission> = self.permissions.iter()
            .filter_map(|s| {
                let quoted = format!("\"{s}\"");
                serde_json::from_str(&quoted).ok()
            })
            .collect();

        ApiKey {
            id: self.id,
            name: self.name,
            key: None, // never returned from DB
            key_hash: self.key_hash,
            role,
            permissions,
            ip_whitelist: self.ip_whitelist,
            rate_limit_per_minute: self.rate_limit_per_minute as u32,
            enabled: self.enabled,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            expires_at: self.expires_at,
        }
    }
}
