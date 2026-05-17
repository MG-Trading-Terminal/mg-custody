//! Webhook endpoint management persistence.

use chrono::{DateTime, Utc};
use custody_common::webhook::WebhookEndpoint;
use custody_common::Chain;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_webhook_endpoint(&self, ep: &WebhookEndpoint) -> anyhow::Result<()> {
        let event_filter: Vec<&str> = ep.event_filter.iter().map(|s| s.as_str()).collect();
        let chain_filter: Vec<&str> = ep.chain_filter.iter().map(|c| c.as_str()).collect();

        sqlx::query(
            "INSERT INTO webhook_endpoints (id, url, secret, event_filter, chain_filter, enabled, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(ep.id)
        .bind(&ep.url)
        .bind(&ep.secret)
        .bind(&event_filter)
        .bind(&chain_filter)
        .bind(ep.enabled)
        .bind(&ep.description)
        .bind(ep.created_at)
        .bind(ep.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_webhook_endpoint(&self, id: Uuid) -> anyhow::Result<Option<WebhookEndpoint>> {
        let row: Option<WebhookEndpointRow> = sqlx::query_as(
            "SELECT id, url, secret, event_filter, chain_filter, enabled, description, created_at, updated_at
             FROM webhook_endpoints WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| r.into_endpoint()).transpose()
    }

    pub async fn list_webhook_endpoints(&self) -> anyhow::Result<Vec<WebhookEndpoint>> {
        let rows: Vec<WebhookEndpointRow> = sqlx::query_as(
            "SELECT id, url, secret, event_filter, chain_filter, enabled, description, created_at, updated_at
             FROM webhook_endpoints ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|r| r.into_endpoint()).collect()
    }

    pub async fn list_active_webhook_endpoints(&self) -> anyhow::Result<Vec<WebhookEndpoint>> {
        let rows: Vec<WebhookEndpointRow> = sqlx::query_as(
            "SELECT id, url, secret, event_filter, chain_filter, enabled, description, created_at, updated_at
             FROM webhook_endpoints WHERE enabled = true ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(|r| r.into_endpoint()).collect()
    }

    pub async fn update_webhook_endpoint(
        &self,
        id: Uuid,
        url: Option<&str>,
        event_filter: Option<&[String]>,
        chain_filter: Option<&[Chain]>,
        enabled: Option<bool>,
        description: Option<&str>,
    ) -> anyhow::Result<bool> {
        // Build update dynamically — for simplicity, always set updated_at
        let ef: Option<Vec<&str>> = event_filter.map(|f| f.iter().map(|s| s.as_str()).collect());
        let cf: Option<Vec<&str>> = chain_filter.map(|f| f.iter().map(|c| c.as_str()).collect());

        let result = sqlx::query(
            "UPDATE webhook_endpoints SET
                url = COALESCE($2, url),
                event_filter = COALESCE($3, event_filter),
                chain_filter = COALESCE($4, chain_filter),
                enabled = COALESCE($5, enabled),
                description = COALESCE($6, description),
                updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(url)
        .bind(ef.as_deref())
        .bind(cf.as_deref())
        .bind(enabled)
        .bind(description)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_webhook_endpoint(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM webhook_endpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct WebhookEndpointRow {
    id: Uuid,
    url: String,
    secret: Option<String>,
    event_filter: Vec<String>,
    chain_filter: Vec<String>,
    enabled: bool,
    description: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl WebhookEndpointRow {
    fn into_endpoint(self) -> anyhow::Result<WebhookEndpoint> {
        let chain_filter = self.chain_filter.iter()
            .map(|c| Chain::try_parse_str(c)
                .ok_or_else(|| anyhow::anyhow!("webhook_endpoints row {}: unknown chain '{c}'", self.id)))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(WebhookEndpoint {
            id: self.id,
            url: self.url,
            secret: self.secret,
            event_filter: self.event_filter,
            chain_filter,
            enabled: self.enabled,
            description: self.description,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
