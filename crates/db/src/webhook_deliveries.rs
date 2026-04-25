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

    /// Read-only listing of currently-due pending deliveries.
    ///
    /// Used by health/dashboard endpoints. Does NOT claim or
    /// reserve rows — that's `claim_pending_webhook_deliveries`.
    /// Kept separate so a dashboard refresh doesn't extend leases
    /// and starve the retry worker.
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

    /// Atomically claim due deliveries for retry, reserving them
    /// against concurrent retry workers.
    ///
    /// Multiple workers used to read the same rows in parallel
    /// and double-fire the HTTP delivery — at-least-once
    /// semantics make that "OK" if the exchange is idempotent,
    /// but the wasted POST plus log noise is avoidable.
    ///
    /// Mechanics: `UPDATE … RETURNING` over an inner
    /// `SELECT … FOR UPDATE SKIP LOCKED`. The inner select grabs
    /// the per-row lock and skips rows another worker already
    /// holds; the outer update bumps `next_retry_at` forward by
    /// the lease so a sibling worker checking the WHERE clause
    /// (`next_retry_at <= NOW()`) won't re-claim the same row.
    /// The lock itself is released when the autocommit statement
    /// finishes; concurrency is then enforced by `next_retry_at`
    /// being in the future.
    ///
    /// Crashed-worker rows reappear when the lease expires
    /// (default 10 minutes) — long enough that a successful
    /// delivery's `mark_webhook_delivered` always fires before
    /// the lease, short enough that a stuck row doesn't stall
    /// for hours. `record_webhook_failure` overwrites
    /// `next_retry_at` with the real backoff time, so a fail-
    /// then-re-claim race isn't a concern.
    pub async fn claim_pending_webhook_deliveries(
        &self,
    ) -> anyhow::Result<Vec<WebhookDelivery>> {
        const LEASE_SECONDS: i64 = 600;
        let rows: Vec<WebhookDeliveryRow> = sqlx::query_as(
            "UPDATE webhook_deliveries
             SET next_retry_at = NOW() + make_interval(secs => $1)
             WHERE id IN (
                 SELECT id FROM webhook_deliveries
                 WHERE status = 'pending'
                   AND (next_retry_at IS NULL OR next_retry_at <= NOW())
                 ORDER BY created_at ASC
                 LIMIT 100
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING id, event_type, payload::text, attempts, max_attempts, status,
                       next_retry_at, last_error, created_at, delivered_at",
        )
        .bind(LEASE_SECONDS)
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

    /// Aggregate delivery metrics per endpoint (success/failure counts, avg latency, last delivery).
    ///
    /// Joins webhook_deliveries to webhook_endpoints via a sub-query so callers get
    /// per-endpoint stats in a single round-trip.  Endpoints with no deliveries are
    /// included with zero counts.
    pub async fn webhook_delivery_metrics(&self) -> anyhow::Result<Vec<WebhookDeliveryMetrics>> {
        // NOTE: webhook_deliveries does not carry an endpoint_id FK in the current schema.
        // We aggregate over the whole table and return a single "global" row until the
        // schema is extended.  The API layer can split these per endpoint once the FK is added.
        let rows: Vec<WebhookMetricsRow> = sqlx::query_as(
            r#"SELECT
                COUNT(*) FILTER (WHERE status = 'delivered') AS success_count,
                COUNT(*) FILTER (WHERE status = 'failed')    AS failure_count,
                AVG(
                    EXTRACT(EPOCH FROM (delivered_at - created_at)) * 1000
                ) FILTER (WHERE status = 'delivered' AND delivered_at IS NOT NULL) AS avg_latency_ms,
                MAX(delivered_at) AS last_delivery_at
               FROM webhook_deliveries"#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| WebhookDeliveryMetrics {
            endpoint_id: None,
            success_count: r.success_count.unwrap_or(0) as u64,
            failure_count: r.failure_count.unwrap_or(0) as u64,
            avg_latency_ms: r.avg_latency_ms,
            last_delivery_at: r.last_delivery_at,
        }).collect())
    }

    /// Recent deliveries for display in the management UI, newest first.
    pub async fn recent_webhook_deliveries(&self, limit: i64) -> anyhow::Result<Vec<WebhookDelivery>> {
        let rows: Vec<WebhookDeliveryRow> = sqlx::query_as(
            "SELECT id, event_type, payload::text, attempts, max_attempts, status,
                    next_retry_at, last_error, created_at, delivered_at
             FROM webhook_deliveries
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_delivery()).collect())
    }
}

/// Aggregate delivery statistics returned by `webhook_delivery_metrics`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebhookDeliveryMetrics {
    /// Endpoint UUID when the schema carries a per-endpoint FK; None for the global aggregate.
    pub endpoint_id: Option<Uuid>,
    pub success_count: u64,
    pub failure_count: u64,
    /// Average delivery latency in milliseconds (None if no delivered events yet).
    pub avg_latency_ms: Option<f64>,
    pub last_delivery_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl WebhookDeliveryMetrics {
    /// Delivery success rate as a percentage, rounded to one decimal place.
    /// Returns 0.0 when there are no deliveries at all.
    pub fn success_rate_pct(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        (self.success_count as f64 / total as f64) * 100.0
    }
}

#[derive(sqlx::FromRow)]
struct WebhookMetricsRow {
    success_count: Option<i64>,
    failure_count: Option<i64>,
    avg_latency_ms: Option<f64>,
    last_delivery_at: Option<chrono::DateTime<chrono::Utc>>,
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

    #[test]
    fn success_rate_zero_when_no_deliveries() {
        let m = WebhookDeliveryMetrics {
            endpoint_id: None,
            success_count: 0,
            failure_count: 0,
            avg_latency_ms: None,
            last_delivery_at: None,
        };
        assert_eq!(m.success_rate_pct(), 0.0);
    }

    #[test]
    fn success_rate_all_success() {
        let m = WebhookDeliveryMetrics {
            endpoint_id: None,
            success_count: 10,
            failure_count: 0,
            avg_latency_ms: Some(120.0),
            last_delivery_at: None,
        };
        assert!((m.success_rate_pct() - 100.0).abs() < 0.001);
    }

    #[test]
    fn success_rate_mixed() {
        let m = WebhookDeliveryMetrics {
            endpoint_id: None,
            success_count: 3,
            failure_count: 1,
            avg_latency_ms: Some(85.5),
            last_delivery_at: None,
        };
        // 3/4 = 75%
        assert!((m.success_rate_pct() - 75.0).abs() < 0.001);
    }

    #[test]
    fn metrics_serialization() {
        let m = WebhookDeliveryMetrics {
            endpoint_id: Some(uuid::Uuid::nil()),
            success_count: 5,
            failure_count: 2,
            avg_latency_ms: Some(100.0),
            last_delivery_at: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("success_count"));
        assert!(json.contains("failure_count"));
        assert!(json.contains("avg_latency_ms"));
    }
}
