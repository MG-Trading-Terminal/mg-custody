//! Alerting system — Telegram, Slack, email.
//!
//! Self-hosted alternative to PagerDuty. Sends alerts on:
//! - Circuit breaker trips
//! - Sanctioned address detected
//! - Chain node failures
//! - Reconciliation discrepancies
//! - Stuck transactions

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Alert severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// An alert to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub source: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Alert {
    pub fn new(severity: AlertSeverity, title: &str, message: &str, source: &str) -> Self {
        Self {
            severity,
            title: title.into(),
            message: message.into(),
            source: source.into(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn critical(title: &str, message: &str, source: &str) -> Self {
        Self::new(AlertSeverity::Critical, title, message, source)
    }

    pub fn emergency(title: &str, message: &str, source: &str) -> Self {
        Self::new(AlertSeverity::Emergency, title, message, source)
    }
}

/// Telegram bot alerting.
pub struct TelegramAlerter {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramAlerter {
    pub fn new(bot_token: &str, chat_id: &str) -> Self {
        Self {
            bot_token: bot_token.into(),
            chat_id: chat_id.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Send an alert via Telegram.
    pub async fn send(&self, alert: &Alert) -> Result<()> {
        let emoji = match alert.severity {
            AlertSeverity::Info => "ℹ️",
            AlertSeverity::Warning => "⚠️",
            AlertSeverity::Critical => "🚨",
            AlertSeverity::Emergency => "🔴",
        };

        let text = format!(
            "{} *{}*\n\n{}\n\n_Source: {} | {}_",
            emoji, alert.title, alert.message, alert.source,
            alert.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        );

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        self.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": self.chat_id,
                "text": text,
                "parse_mode": "Markdown",
            }))
            .send()
            .await?;

        Ok(())
    }
}

/// Slack webhook alerting.
pub struct SlackAlerter {
    webhook_url: String,
    client: reqwest::Client,
}

impl SlackAlerter {
    pub fn new(webhook_url: &str) -> Self {
        Self {
            webhook_url: webhook_url.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Send an alert via Slack.
    pub async fn send(&self, alert: &Alert) -> Result<()> {
        let color = match alert.severity {
            AlertSeverity::Info => "#36a64f",
            AlertSeverity::Warning => "#ff9900",
            AlertSeverity::Critical => "#ff0000",
            AlertSeverity::Emergency => "#8b0000",
        };

        self.client
            .post(&self.webhook_url)
            .json(&serde_json::json!({
                "attachments": [{
                    "color": color,
                    "title": alert.title,
                    "text": alert.message,
                    "footer": format!("{} | {}", alert.source, alert.timestamp),
                }]
            }))
            .send()
            .await?;

        Ok(())
    }
}

/// Multi-channel alert dispatcher.
pub struct AlertDispatcher {
    telegram: Option<TelegramAlerter>,
    slack: Option<SlackAlerter>,
    /// Minimum severity to send (filter out noise).
    min_severity: AlertSeverity,
}

impl AlertDispatcher {
    pub fn new() -> Self {
        Self { telegram: None, slack: None, min_severity: AlertSeverity::Warning }
    }

    pub fn with_telegram(mut self, bot_token: &str, chat_id: &str) -> Self {
        self.telegram = Some(TelegramAlerter::new(bot_token, chat_id));
        self
    }

    pub fn with_slack(mut self, webhook_url: &str) -> Self {
        self.slack = Some(SlackAlerter::new(webhook_url));
        self
    }

    pub fn with_min_severity(mut self, severity: AlertSeverity) -> Self {
        self.min_severity = severity;
        self
    }

    /// Dispatch an alert to all configured channels.
    pub async fn dispatch(&self, alert: &Alert) -> Result<()> {
        if alert.severity < self.min_severity {
            return Ok(());
        }

        if let Some(ref tg) = self.telegram
            && let Err(e) = tg.send(alert).await {
                tracing::error!(error = %e, "Telegram alert failed");
            }

        if let Some(ref slack) = self.slack
            && let Err(e) = slack.send(alert).await {
                tracing::error!(error = %e, "Slack alert failed");
            }

        tracing::info!(
            severity = ?alert.severity,
            title = %alert.title,
            "Alert dispatched"
        );

        Ok(())
    }
}

impl Default for AlertDispatcher {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_creation() {
        let alert = Alert::critical("Circuit breaker tripped", "Daily limit exceeded: $51M", "withdrawal_pipeline");
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert!(alert.message.contains("$51M"));
    }

    #[test]
    fn severity_ordering() {
        assert!(AlertSeverity::Info < AlertSeverity::Warning);
        assert!(AlertSeverity::Warning < AlertSeverity::Critical);
        assert!(AlertSeverity::Critical < AlertSeverity::Emergency);
    }

    #[test]
    fn dispatcher_min_severity_filter() {
        let dispatcher = AlertDispatcher::new().with_min_severity(AlertSeverity::Critical);
        // Info alerts should be filtered
        let info = Alert::new(AlertSeverity::Info, "test", "test", "test");
        assert!(info.severity < dispatcher.min_severity);
    }

    #[test]
    fn alert_serialization() {
        let alert = Alert::emergency("SANCTIONS HIT", "Tornado Cash address detected", "compliance");
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("emergency"));
        assert!(json.contains("SANCTIONS HIT"));
    }
}
