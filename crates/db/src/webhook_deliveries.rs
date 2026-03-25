//! Webhook delivery tracking — REAL SQL queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::CustodyDb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus { Pending, Delivered, Failed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub event_type: String,
    pub payload: String,
    pub attempts: u32,
    pub max_attempts: u32,
    pub status: DeliveryStatus,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
}

pub fn backoff_seconds(attempts: u32) -> u64 {
    std::cmp::min(2u64.pow(attempts), 300)
}

impl CustodyDb {
    pub async fn insert_webhook_delivery(&self, d: &WebhookDelivery) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO webhook_deliveries (id, event_type, payload, attempts, max_attempts, status, next_retry_at, created_at)
             VALUES ($1, $2, $3::jsonb, $4, $5, $6, $7, $8)",
        )
        .bind(d.id)
        .bind(&d.event_type)
        .bind(&d.payload)
        .bind(d.attempts as i32)
        .bind(d.max_attempts as i32)
        .bind("pending")
        .bind(d.next_retry_at)
        .bind(d.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pending_webhook_deliveries(&self) -> anyhow::Result<Vec<WebhookDelivery>> {
        let rows: Vec<WebhookDeliveryRow> = sqlx::query_as(
            "SELECT id, event_type, payload::text, attempts, max_attempts, status,
                    next_retry_at, last_error, created_at, delivered_at
             FROM webhook_deliveries
             WHERE status = 'pending' AND (next_retry_at IS NULL OR next_retry_at <= NOW())
             ORDER BY created_at ASC
             LIMIT 100",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_delivery()).collect())
    }

    pub async fn mark_webhook_delivered(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE webhook_deliveries SET status = 'delivered', delivered_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_webhook_failure(&self, id: Uuid, error: &str, next_retry_at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE webhook_deliveries SET
                attempts = attempts + 1,
                last_error = $1,
                next_retry_at = $2,
                status = CASE WHEN attempts + 1 >= max_attempts THEN 'failed' ELSE 'pending' END
             WHERE id = $3",
        )
        .bind(error)
        .bind(next_retry_at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get failed webhook deliveries (dead letter queue).
    pub async fn failed_webhook_deliveries(&self, limit: i64) -> anyhow::Result<Vec<WebhookDelivery>> {
        let rows: Vec<WebhookDeliveryRow> = sqlx::query_as(
            "SELECT id, event_type, payload::text, attempts, max_attempts, status,
                    next_retry_at, last_error, created_at, delivered_at
             FROM webhook_deliveries
             WHERE status = 'failed'
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_delivery()).collect())
    }

    /// Purge old delivered webhooks (older than days).
    pub async fn purge_old_deliveries(&self, older_than_days: i32) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM webhook_deliveries
             WHERE status = 'delivered' AND delivered_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(older_than_days)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Retry a specific failed webhook delivery.
    pub async fn retry_failed_delivery(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE webhook_deliveries SET status = 'pending', next_retry_at = NOW() WHERE id = $1 AND status = 'failed'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct WebhookDeliveryRow {
    id: Uuid,
    event_type: String,
    payload: Option<String>,
    attempts: i32,
    max_attempts: i32,
    status: String,
    next_retry_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    delivered_at: Option<DateTime<Utc>>,
}

impl WebhookDeliveryRow {
    fn into_delivery(self) -> WebhookDelivery {
        WebhookDelivery {
            id: self.id,
            event_type: self.event_type,
            payload: self.payload.unwrap_or_default(),
            attempts: self.attempts as u32,
            max_attempts: self.max_attempts as u32,
            status: match self.status.as_str() {
                "delivered" => DeliveryStatus::Delivered,
                "failed" => DeliveryStatus::Failed,
                _ => DeliveryStatus::Pending,
            },
            next_retry_at: self.next_retry_at,
            last_error: self.last_error,
            created_at: self.created_at,
            delivered_at: self.delivered_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff() {
        assert_eq!(backoff_seconds(0), 1);
        assert_eq!(backoff_seconds(1), 2);
        assert_eq!(backoff_seconds(2), 4);
        assert_eq!(backoff_seconds(9), 300); // capped
        assert_eq!(backoff_seconds(20), 300);
    }
}
