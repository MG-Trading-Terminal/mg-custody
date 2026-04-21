//! HMAC-signed webhook dispatch to the exchange.
//!
//! Sends custody events (deposit confirmed, withdrawal broadcast/confirmed/failed)
//! to the exchange's webhook endpoint with HMAC-SHA256 authentication.
//!
//! Delivery is at-least-once: failed deliveries are retried with exponential backoff.

use anyhow::Result;
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

use custody_common::CustodyEvent;
use custody_db::CustodyDb;
use custody_db::webhook_deliveries::{DeliveryStatus, WebhookDelivery, backoff_seconds};

type HmacSha256 = Hmac<Sha256>;

/// Sends custody events to the exchange webhook endpoint.
pub struct WebhookSender {
    client: reqwest::Client,
    webhook_url: String,
    hmac_secret: Vec<u8>,
    db: CustodyDb,
    max_attempts: u32,
}

impl WebhookSender {
    pub fn new(
        webhook_url: String,
        hmac_secret: Vec<u8>,
        db: CustodyDb,
        max_attempts: u32,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            webhook_url,
            hmac_secret,
            db,
            max_attempts,
        }
    }

    /// Send a custody event to the exchange.
    ///
    /// Creates a delivery record and attempts immediate delivery.
    /// On failure, the delivery is scheduled for retry.
    pub async fn send(&self, event: &CustodyEvent) -> Result<()> {
        let payload = serde_json::to_string(event)?;
        let event_type = event.event_type();

        let delivery = WebhookDelivery {
            id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            payload: payload.clone(),
            attempts: 0,
            max_attempts: self.max_attempts,
            status: DeliveryStatus::Pending,
            next_retry_at: None,
            last_error: None,
            created_at: Utc::now(),
            delivered_at: None,
        };

        self.db.insert_webhook_delivery(&delivery).await?;

        // Attempt immediate delivery
        match self.deliver(&payload).await {
            Ok(()) => {
                self.db.mark_webhook_delivered(delivery.id).await?;
                tracing::info!(event_type, delivery_id = %delivery.id, "Webhook delivered");
            }
            Err(e) => {
                let next_retry = Utc::now()
                    + chrono::Duration::seconds(backoff_seconds(1) as i64);
                self.db
                    .record_webhook_failure(delivery.id, &e.to_string(), next_retry)
                    .await?;
                tracing::warn!(
                    event_type,
                    delivery_id = %delivery.id,
                    error = %e,
                    "Webhook delivery failed, will retry"
                );
            }
        }

        Ok(())
    }

    /// Retry all pending deliveries that are due.
    pub async fn retry_pending(&self) -> Result<()> {
        let pending = self.db.pending_webhook_deliveries().await?;

        for delivery in pending {
            match self.deliver(&delivery.payload).await {
                Ok(()) => {
                    self.db.mark_webhook_delivered(delivery.id).await?;
                    tracing::info!(delivery_id = %delivery.id, "Webhook retry succeeded");
                }
                Err(e) => {
                    let next_attempt = delivery.attempts + 1;
                    let next_retry = Utc::now()
                        + chrono::Duration::seconds(backoff_seconds(next_attempt) as i64);
                    self.db
                        .record_webhook_failure(delivery.id, &e.to_string(), next_retry)
                        .await?;
                    tracing::warn!(
                        delivery_id = %delivery.id,
                        attempt = next_attempt,
                        error = %e,
                        "Webhook retry failed"
                    );
                }
            }
        }

        Ok(())
    }

    /// Deliver a payload to the exchange webhook endpoint.
    async fn deliver(&self, payload: &str) -> Result<()> {
        let timestamp = Utc::now().timestamp().to_string();

        // Sign the PATH, not the full URL. CLAUDE.md
        // §API Contract fixes the HMAC convention as
        // `method + path + timestamp + body`, and
        // gateway/auth.rs::verify_signature (the inbound half
        // of the same protocol) uses `path`. Previously this
        // signed `self.webhook_url` (a full URL including
        // scheme + host), which meant any exchange-side
        // verifier following the documented convention
        // produced a different HMAC and rejected every
        // outbound webhook. Retries then ran the delivery up
        // to max_attempts and landed it in the dead-letter
        // queue — deposits/withdrawals confirmed on-chain but
        // the exchange never learned about them. Extracting
        // the path here aligns the two sides of the protocol.
        let path = reqwest::Url::parse(&self.webhook_url)
            .map(|u| u.path().to_string())
            .map_err(|e| anyhow::anyhow!(
                "Webhook URL {} is not parseable: {e} — \
                 refusing to sign; operator must supply a valid URL",
                self.webhook_url
            ))?;

        let signature = compute_hmac(
            &self.hmac_secret,
            "POST",
            &path,
            &timestamp,
            payload,
        );

        let response = self
            .client
            .post(&self.webhook_url)
            .header("Content-Type", "application/json")
            .header("X-Custody-Timestamp", &timestamp)
            .header("X-Custody-Signature", &signature)
            .body(payload.to_string())
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!(
                "Webhook returned HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
        }
    }
}

/// Compute HMAC-SHA256 signature for webhook authentication.
///
/// Signs: method + url + timestamp + body
pub fn compute_hmac(
    secret: &[u8],
    method: &str,
    url: &str,
    timestamp: &str,
    body: &str,
) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(method.as_bytes());
    mac.update(url.as_bytes());
    mac.update(timestamp.as_bytes());
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Helper to extract event type name from CustodyEvent.
trait EventType {
    fn event_type(&self) -> &'static str;
}

impl EventType for CustodyEvent {
    fn event_type(&self) -> &'static str {
        match self {
            CustodyEvent::DepositConfirmed { .. } => "deposit_confirmed",
            CustodyEvent::WithdrawalBroadcast { .. } => "withdrawal_broadcast",
            CustodyEvent::WithdrawalConfirmed { .. } => "withdrawal_confirmed",
            CustodyEvent::WithdrawalFailed { .. } => "withdrawal_failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_signature_deterministic() {
        let sig1 = compute_hmac(b"secret", "POST", "/webhook", "12345", r#"{"test":true}"#);
        let sig2 = compute_hmac(b"secret", "POST", "/webhook", "12345", r#"{"test":true}"#);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn hmac_signature_changes_with_body() {
        let sig1 = compute_hmac(b"secret", "POST", "/webhook", "12345", r#"{"a":1}"#);
        let sig2 = compute_hmac(b"secret", "POST", "/webhook", "12345", r#"{"a":2}"#);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn hmac_signature_changes_with_secret() {
        let sig1 = compute_hmac(b"secret1", "POST", "/webhook", "12345", "body");
        let sig2 = compute_hmac(b"secret2", "POST", "/webhook", "12345", "body");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn hmac_signature_is_hex() {
        let sig = compute_hmac(b"secret", "POST", "/url", "ts", "body");
        assert_eq!(sig.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
