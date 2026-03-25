//! Webhook delivery tracking for at-least-once delivery.
//!
//! Persists every webhook attempt so failed deliveries can be retried
//! with exponential backoff.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::CustodyDb;

/// Status of a webhook delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

/// A webhook delivery record.
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

/// Calculate exponential backoff delay in seconds: min(2^attempts, 300).
pub fn backoff_seconds(attempts: u32) -> u64 {
    std::cmp::min(2u64.pow(attempts), 300)
}

impl CustodyDb {
    /// Insert a new webhook delivery to be sent.
    pub async fn insert_webhook_delivery(&self, delivery: &WebhookDelivery) -> anyhow::Result<()> {
        let _ = delivery;
        // TODO: sqlx insert
        Ok(())
    }

    /// Get all pending deliveries that are ready for retry.
    pub async fn pending_webhook_deliveries(&self) -> anyhow::Result<Vec<WebhookDelivery>> {
        // TODO: SELECT * FROM webhook_deliveries WHERE status = 'pending' AND next_retry_at <= NOW()
        Ok(vec![])
    }

    /// Mark a delivery as successfully delivered.
    pub async fn mark_webhook_delivered(&self, id: Uuid) -> anyhow::Result<()> {
        let _ = id;
        // TODO: UPDATE webhook_deliveries SET status = 'delivered', delivered_at = NOW()
        Ok(())
    }

    /// Record a failed delivery attempt and schedule retry.
    pub async fn record_webhook_failure(
        &self,
        id: Uuid,
        error: &str,
        next_retry_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let _ = (id, error, next_retry_at);
        // TODO: UPDATE webhook_deliveries SET attempts = attempts + 1, last_error = $2,
        //       next_retry_at = $3, status = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff() {
        assert_eq!(backoff_seconds(0), 1);   // 2^0 = 1
        assert_eq!(backoff_seconds(1), 2);   // 2^1 = 2
        assert_eq!(backoff_seconds(2), 4);   // 2^2 = 4
        assert_eq!(backoff_seconds(3), 8);
        assert_eq!(backoff_seconds(8), 256);
        assert_eq!(backoff_seconds(9), 300); // capped at 300
        assert_eq!(backoff_seconds(20), 300);
    }
}
