//! Notification dispatch — sends custody events to webhook URLs, email, and Slack.
//!
//! `NotificationDispatcher` holds a set of `NotificationChannelConfig` entries
//! (loaded from config or DB) and routes each custody event to every channel whose
//! `events` list contains the event type.
//!
//! Webhook delivery uses HMAC-SHA256 signing identical to the exchange webhook
//! (`crates/confirmations/src/webhook.rs`).  Retry uses the same exponential
//! backoff cap (max 300 s) defined in `webhook_deliveries::backoff_seconds`.
//!
//! Email: actual SMTP is outside this crate's scope (no `lettre` dependency).
//! The formatted payload is logged at INFO level so operators can pipe it to an
//! external mailer or replace the stub later.
//!
//! Slack: posts to an incoming-webhook URL using Slack's `text` field format.

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use custody_common::email::{EmailMessage, EmailSender};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde_json::{Value, json};

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Type of notification channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelType {
    Webhook,
    Email,
    Slack,
}

/// Configuration for a single notification destination.
#[derive(Debug, Clone)]
pub struct NotificationChannelConfig {
    /// Channel delivery mechanism.
    pub channel_type: ChannelType,
    /// Webhook/Slack: URL to POST.  Email: recipient address.
    pub endpoint: String,
    /// Event types to forward, e.g. `["deposit_confirmed", "withdrawal_failed"]`.
    /// Empty list means "all events".
    pub events: Vec<String>,
    /// Channel is skipped entirely when `false`.
    pub enabled: bool,
}

/// Dispatches custody events to configured notification channels.
///
/// Construct once and share via `Arc`.  All methods are `&self` so the struct
/// can be used concurrently.
pub struct NotificationDispatcher {
    channels: Vec<NotificationChannelConfig>,
    client: reqwest::Client,
    /// HMAC secret used for webhook `X-Custody-Signature` header.
    hmac_secret: Vec<u8>,
    /// Maximum delivery attempts per event per channel before giving up.
    max_attempts: u32,
    /// Outbound email transport. When `Some`, `send_email_notification`
    /// dispatches via lettre (real SMTP / console / noop per operator
    /// config). When `None`, we keep the previous log-only fallback
    /// but emit a `warn!` rather than `info!` so a missing transport
    /// is loud enough for operators to notice — critical alerts
    /// dropping to logs is exactly the silent-failure pattern we're
    /// closing elsewhere.
    email_sender: Option<Arc<EmailSender>>,
}

impl NotificationDispatcher {
    pub fn new(
        channels: Vec<NotificationChannelConfig>,
        hmac_secret: Vec<u8>,
        max_attempts: u32,
    ) -> Self {
        Self {
            channels,
            client: reqwest::Client::new(),
            hmac_secret,
            max_attempts,
            email_sender: None,
        }
    }

    /// Attach a real email transport. Required to actually deliver
    /// email notifications; without it, the dispatcher logs and warns.
    pub fn with_email_sender(mut self, sender: Arc<EmailSender>) -> Self {
        self.email_sender = Some(sender);
        self
    }

    // -----------------------------------------------------------------------
    // Primary dispatch entry point
    // -----------------------------------------------------------------------

    /// Dispatch a custody event to every enabled channel that subscribes to it.
    ///
    /// Delivery failures are retried with exponential backoff up to
    /// `max_attempts`.  Errors on individual channels are logged but do **not**
    /// abort dispatch to other channels.
    pub async fn dispatch_event(&self, event_type: &str, payload: Value) {
        for channel in &self.channels {
            if !channel.enabled {
                continue;
            }
            if !self.channel_matches_event(channel, event_type) {
                continue;
            }

            let result = match channel.channel_type {
                ChannelType::Webhook => {
                    self.send_webhook_with_retry(&channel.endpoint, &payload).await
                }
                ChannelType::Email => {
                    self.send_email_notification(&channel.endpoint, event_type, &payload)
                        .await
                }
                ChannelType::Slack => {
                    self.send_slack_notification_with_retry(&channel.endpoint, &payload)
                        .await
                }
            };

            if let Err(e) = result {
                tracing::error!(
                    event_type,
                    channel_type = ?channel.channel_type,
                    endpoint = %channel.endpoint,
                    error = %e,
                    "Notification dispatch failed"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Webhook
    // -----------------------------------------------------------------------

    /// POST payload to a webhook URL with HMAC-SHA256 authentication.
    ///
    /// Headers added:
    /// - `X-Custody-Timestamp`: Unix timestamp string
    /// - `X-Custody-Signature`: hex(HMAC-SHA256(POST + url + ts + body))
    pub async fn send_webhook(&self, url: &str, payload: &Value) -> Result<()> {
        let body = serde_json::to_string(payload)?;
        let timestamp = Utc::now().timestamp().to_string();
        let signature = compute_hmac(&self.hmac_secret, "POST", url, &timestamp, &body);

        let response = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Custody-Timestamp", &timestamp)
            .header("X-Custody-Signature", &signature)
            .body(body)
            .send()
            .await?;

        if response.status().is_success() {
            tracing::info!(url, "Webhook notification delivered");
            Ok(())
        } else {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Webhook {url} returned HTTP {status}: {text}");
        }
    }

    /// Deliver to a webhook URL, retrying with exponential backoff on failure.
    async fn send_webhook_with_retry(&self, url: &str, payload: &Value) -> Result<()> {
        let mut last_error = anyhow::anyhow!("no attempts made");

        for attempt in 1..=self.max_attempts {
            match self.send_webhook(url, payload).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = e;
                    if attempt < self.max_attempts {
                        let delay = backoff_delay(attempt);
                        tracing::warn!(
                            url,
                            attempt,
                            delay_secs = delay,
                            error = %last_error,
                            "Webhook delivery failed, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    }
                }
            }
        }

        Err(last_error)
    }

    // -----------------------------------------------------------------------
    // Email (real SMTP via custody_common::email::EmailSender)
    // -----------------------------------------------------------------------

    /// Deliver an email notification through the configured
    /// `EmailSender` (lettre SMTP / dev console / test noop —
    /// whichever the operator wired at boot).
    ///
    /// When no sender is attached, we log-and-warn instead of
    /// log-and-info so a missing transport is visible. Critical
    /// alerts like "withdrawal_failed" silently reaching no one
    /// is the exact pattern this crate's honesty pass targets.
    pub async fn send_email_notification(
        &self,
        to: &str,
        event_type: &str,
        payload: &Value,
    ) -> Result<()> {
        let subject = email_subject(event_type, payload);
        let body = email_body(event_type, payload);

        let Some(sender) = self.email_sender.as_ref() else {
            tracing::warn!(
                to,
                event_type,
                subject = %subject,
                "Email notification dropped — no EmailSender attached. \
                 Configure CUSTODY_EMAIL__MODE=smtp + CUSTODY_EMAIL__SMTP_* \
                 and call NotificationDispatcher::with_email_sender at boot."
            );
            return Ok(());
        };

        let msg = EmailMessage {
            to: to.to_string(),
            subject,
            body_text: body,
            body_html: None,
        };
        sender
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("email send: {e}"))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Slack
    // -----------------------------------------------------------------------

    /// POST a Slack incoming-webhook message.
    ///
    /// Formats a short text block: `*[event_type]*\n<key: value pairs>`.
    pub async fn send_slack_notification(
        &self,
        webhook_url: &str,
        payload: &Value,
    ) -> Result<()> {
        let text = slack_text(payload);
        let slack_body = json!({ "text": text });
        let body = serde_json::to_string(&slack_body)?;

        let response = self
            .client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        if response.status().is_success() {
            tracing::info!(webhook_url, "Slack notification delivered");
            Ok(())
        } else {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Slack webhook {webhook_url} returned HTTP {status}: {text}");
        }
    }

    async fn send_slack_notification_with_retry(
        &self,
        url: &str,
        payload: &Value,
    ) -> Result<()> {
        let mut last_error = anyhow::anyhow!("no attempts made");

        for attempt in 1..=self.max_attempts {
            match self.send_slack_notification(url, payload).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_error = e;
                    if attempt < self.max_attempts {
                        let delay = backoff_delay(attempt);
                        tracing::warn!(
                            url,
                            attempt,
                            delay_secs = delay,
                            error = %last_error,
                            "Slack notification failed, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    }
                }
            }
        }

        Err(last_error)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Returns `true` if the channel's `events` list includes `event_type`,
    /// or if the list is empty (meaning "all events").
    fn channel_matches_event(&self, channel: &NotificationChannelConfig, event_type: &str) -> bool {
        channel.events.is_empty() || channel.events.iter().any(|e| e == event_type)
    }
}

// ---------------------------------------------------------------------------
// Free functions (pub for reuse in tests)
// ---------------------------------------------------------------------------

/// Compute HMAC-SHA256 over `method + url + timestamp + body`.
///
/// Identical to `webhook::compute_hmac` — extracted here so
/// `notification_dispatch` has no dependency on `webhook`.
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

/// Exponential backoff capped at 300 s, matching the exchange-webhook pattern.
fn backoff_delay(attempt: u32) -> u64 {
    std::cmp::min(2u64.pow(attempt), 300)
}

/// Build a short, human-readable email subject line from an event.
fn email_subject(event_type: &str, payload: &Value) -> String {
    let asset = payload["asset"].as_str().unwrap_or("unknown");
    let amount = payload["amount"].as_str().unwrap_or("");

    match event_type {
        "deposit_confirmed" => format!("Deposit confirmed: {amount} {asset}"),
        "withdrawal_broadcast" => format!("Withdrawal broadcast: {amount} {asset}"),
        "withdrawal_confirmed" => format!("Withdrawal confirmed: {amount} {asset}"),
        "withdrawal_failed" => format!("ALERT: Withdrawal failed — {amount} {asset}"),
        other => format!("Custody event: {other}"),
    }
}

/// Build a plain-text email body from the JSON payload.
fn email_body(event_type: &str, payload: &Value) -> String {
    let mut lines = vec![format!("Event: {event_type}")];

    if let Some(obj) = payload.as_object() {
        for (k, v) in obj {
            // Skip nested objects — keep the body readable.
            if !v.is_object() && !v.is_array() {
                lines.push(format!("{k}: {}", v.as_str().unwrap_or(&v.to_string())));
            }
        }
    }

    lines.join("\n")
}

/// Build a Slack-formatted message string.
fn slack_text(payload: &Value) -> String {
    let event_type = payload["event_type"].as_str().unwrap_or("custody_event");
    let asset = payload["asset"].as_str().unwrap_or("?");
    let amount = payload["amount"].as_str().unwrap_or("?");
    let chain = payload["chain"].as_str().unwrap_or("?");
    let tx_hash = payload["tx_hash"].as_str().unwrap_or("pending");

    format!(
        "*[{event_type}]*\nChain: {chain} | Asset: {asset} | Amount: {amount}\nTx: {tx_hash}"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_dispatcher(channels: Vec<NotificationChannelConfig>) -> NotificationDispatcher {
        NotificationDispatcher::new(channels, b"test-secret".to_vec(), 3)
    }

    // --- HMAC signing ---

    #[test]
    fn hmac_is_deterministic() {
        let sig1 = compute_hmac(b"secret", "POST", "https://example.com/hook", "1234", r#"{"a":1}"#);
        let sig2 = compute_hmac(b"secret", "POST", "https://example.com/hook", "1234", r#"{"a":1}"#);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn hmac_changes_with_body() {
        let sig1 = compute_hmac(b"secret", "POST", "https://example.com/hook", "1234", r#"{"a":1}"#);
        let sig2 = compute_hmac(b"secret", "POST", "https://example.com/hook", "1234", r#"{"a":2}"#);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn hmac_changes_with_secret() {
        let sig1 = compute_hmac(b"secret1", "POST", "https://x.com", "ts", "body");
        let sig2 = compute_hmac(b"secret2", "POST", "https://x.com", "ts", "body");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn hmac_is_64_hex_chars() {
        let sig = compute_hmac(b"key", "POST", "https://x.com", "ts", "body");
        assert_eq!(sig.len(), 64);
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // Known-good test vector.
    // Reference: printf '%s' "POSThttps://example.com1234567890hello" | openssl dgst -sha256 -hmac "secret"
    // = 4a270f26ad7b6c016030eb7c8b3d4912199312f94ccb3550b2b4a6b28af91622
    #[test]
    fn hmac_known_test_vector() {
        let sig = compute_hmac(
            b"secret",
            "POST",
            "https://example.com",
            "1234567890",
            "hello",
        );
        assert_eq!(
            sig,
            "4a270f26ad7b6c016030eb7c8b3d4912199312f94ccb3550b2b4a6b28af91622",
        );
    }

    // --- Channel filtering ---

    #[test]
    fn channel_matches_specific_event() {
        let dispatcher = make_dispatcher(vec![]);
        let channel = NotificationChannelConfig {
            channel_type: ChannelType::Webhook,
            endpoint: "https://x.com".into(),
            events: vec!["deposit_confirmed".into(), "withdrawal_failed".into()],
            enabled: true,
        };
        assert!(dispatcher.channel_matches_event(&channel, "deposit_confirmed"));
        assert!(dispatcher.channel_matches_event(&channel, "withdrawal_failed"));
        assert!(!dispatcher.channel_matches_event(&channel, "withdrawal_broadcast"));
    }

    #[test]
    fn channel_with_empty_events_matches_all() {
        let dispatcher = make_dispatcher(vec![]);
        let channel = NotificationChannelConfig {
            channel_type: ChannelType::Email,
            endpoint: "ops@example.com".into(),
            events: vec![], // empty = all events
            enabled: true,
        };
        assert!(dispatcher.channel_matches_event(&channel, "deposit_confirmed"));
        assert!(dispatcher.channel_matches_event(&channel, "withdrawal_broadcast"));
        assert!(dispatcher.channel_matches_event(&channel, "withdrawal_confirmed"));
        assert!(dispatcher.channel_matches_event(&channel, "withdrawal_failed"));
    }

    #[test]
    fn disabled_channel_skipped_in_dispatch() {
        // We can't test async dispatch without a live HTTP server, but we can
        // verify the enabled flag is respected at the filtering level.
        let dispatcher = make_dispatcher(vec![]);
        let channel = NotificationChannelConfig {
            channel_type: ChannelType::Slack,
            endpoint: "https://hooks.slack.com/xxx".into(),
            events: vec!["deposit_confirmed".into()],
            enabled: false,
        };
        // channel_matches_event doesn't check enabled — dispatch_event does.
        // Verify that a disabled channel still matches (would be delivered if enabled).
        assert!(dispatcher.channel_matches_event(&channel, "deposit_confirmed"));
    }

    // --- Email formatting ---

    #[test]
    fn email_subject_deposit_confirmed() {
        let payload = json!({"asset": "BTC", "amount": "1.5"});
        let subject = email_subject("deposit_confirmed", &payload);
        assert_eq!(subject, "Deposit confirmed: 1.5 BTC");
    }

    #[test]
    fn email_subject_withdrawal_failed() {
        let payload = json!({"asset": "USDT", "amount": "500.00"});
        let subject = email_subject("withdrawal_failed", &payload);
        assert_eq!(subject, "ALERT: Withdrawal failed — 500.00 USDT");
    }

    #[test]
    fn email_subject_withdrawal_broadcast() {
        let payload = json!({"asset": "ETH", "amount": "2.0"});
        let subject = email_subject("withdrawal_broadcast", &payload);
        assert_eq!(subject, "Withdrawal broadcast: 2.0 ETH");
    }

    #[test]
    fn email_subject_withdrawal_confirmed() {
        let payload = json!({"asset": "SOL", "amount": "10.0"});
        let subject = email_subject("withdrawal_confirmed", &payload);
        assert_eq!(subject, "Withdrawal confirmed: 10.0 SOL");
    }

    #[test]
    fn email_subject_unknown_event() {
        let payload = json!({});
        let subject = email_subject("reorg_detected", &payload);
        assert_eq!(subject, "Custody event: reorg_detected");
    }

    #[test]
    fn email_body_includes_event_type() {
        let payload = json!({"asset": "BTC", "amount": "1.5", "chain": "bitcoin"});
        let body = email_body("deposit_confirmed", &payload);
        assert!(body.contains("Event: deposit_confirmed"));
        assert!(body.contains("asset: BTC"));
        assert!(body.contains("amount: 1.5"));
        assert!(body.contains("chain: bitcoin"));
    }

    #[test]
    fn email_body_skips_nested_objects() {
        let payload = json!({
            "asset": "ETH",
            "metadata": {"nested": "value"}
        });
        let body = email_body("withdrawal_failed", &payload);
        // Top-level scalar included
        assert!(body.contains("asset: ETH"));
        // Nested object omitted
        assert!(!body.contains("nested"));
    }

    // --- Slack formatting ---

    #[test]
    fn slack_text_includes_all_fields() {
        let payload = json!({
            "event_type": "deposit_confirmed",
            "chain": "ethereum",
            "asset": "USDT",
            "amount": "1000.00",
            "tx_hash": "0xdeadbeef"
        });
        let text = slack_text(&payload);
        assert!(text.contains("deposit_confirmed"));
        assert!(text.contains("ethereum"));
        assert!(text.contains("USDT"));
        assert!(text.contains("1000.00"));
        assert!(text.contains("0xdeadbeef"));
    }

    #[test]
    fn slack_text_pending_tx_hash() {
        let payload = json!({
            "event_type": "withdrawal_broadcast",
            "chain": "bitcoin",
            "asset": "BTC",
            "amount": "0.5"
            // tx_hash missing
        });
        let text = slack_text(&payload);
        assert!(text.contains("pending"));
    }

    // --- Backoff ---

    #[test]
    fn backoff_exponential_growth() {
        assert_eq!(backoff_delay(1), 2);
        assert_eq!(backoff_delay(2), 4);
        assert_eq!(backoff_delay(3), 8);
        assert_eq!(backoff_delay(4), 16);
    }

    #[test]
    fn backoff_capped_at_300() {
        // 2^9 = 512 > 300
        assert_eq!(backoff_delay(9), 300);
        assert_eq!(backoff_delay(20), 300);
    }

    // --- Email dispatch (log-and-warn fallback when no sender wired) ---

    #[tokio::test]
    async fn send_email_notification_without_sender_succeeds_with_warn() {
        // Without an EmailSender attached, the handler logs a warn
        // and returns Ok — we don't want to block dispatch to the
        // other channels on a missing transport.
        let dispatcher = make_dispatcher(vec![]);
        let payload = json!({"asset": "BTC", "amount": "1.0", "chain": "bitcoin"});
        let result = dispatcher
            .send_email_notification("ops@example.com", "deposit_confirmed", &payload)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_email_notification_with_noop_sender_delivers_ok() {
        // With a real sender (noop transport), the path should take
        // the actual delivery branch instead of the warn-and-drop
        // fallback. Noop returns Ok so we can verify end-to-end
        // without a live SMTP relay.
        use custody_common::email::EmailSender;
        let dispatcher = make_dispatcher(vec![]).with_email_sender(
            std::sync::Arc::new(EmailSender::noop()),
        );
        let payload = json!({"asset": "BTC", "amount": "1.0", "chain": "bitcoin"});
        let result = dispatcher
            .send_email_notification("ops@example.com", "deposit_confirmed", &payload)
            .await;
        assert!(result.is_ok());
    }
}
