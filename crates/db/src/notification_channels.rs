//! Notification channel persistence.

use chrono::{DateTime, Utc};
use custody_common::alerts::AlertSeverity;
use custody_common::connectivity::{ChannelConfig, ChannelType, NotificationChannel};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    pub async fn insert_notification_channel(&self, ch: &NotificationChannel) -> anyhow::Result<()> {
        let config_json = serde_json::to_value(&ch.config)?;
        let severity = serde_json::to_string(&ch.min_severity)?;

        sqlx::query(
            "INSERT INTO notification_channels (id, name, channel_type, config, min_severity, enabled, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(ch.id)
        .bind(&ch.name)
        .bind(serde_json::to_string(&ch.channel_type)?.trim_matches('"'))
        .bind(&config_json)
        .bind(severity.trim_matches('"'))
        .bind(ch.enabled)
        .bind(ch.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_notification_channel(&self, id: Uuid) -> anyhow::Result<Option<NotificationChannel>> {
        let row: Option<NotifChannelRow> = sqlx::query_as(
            "SELECT id, name, channel_type, config, min_severity, enabled, created_at
             FROM notification_channels WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|r| r.into_channel().ok()))
    }

    pub async fn list_notification_channels(&self) -> anyhow::Result<Vec<NotificationChannel>> {
        let rows: Vec<NotifChannelRow> = sqlx::query_as(
            "SELECT id, name, channel_type, config, min_severity, enabled, created_at
             FROM notification_channels ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().filter_map(|r| r.into_channel().ok()).collect())
    }

    pub async fn delete_notification_channel(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM notification_channels WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn toggle_notification_channel(&self, id: Uuid, enabled: bool) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE notification_channels SET enabled = $2 WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct NotifChannelRow {
    id: Uuid,
    name: String,
    channel_type: String,
    config: serde_json::Value,
    min_severity: String,
    enabled: bool,
    created_at: DateTime<Utc>,
}

impl NotifChannelRow {
    fn into_channel(self) -> anyhow::Result<NotificationChannel> {
        let channel_type = match self.channel_type.as_str() {
            "telegram" => ChannelType::Telegram,
            "slack" => ChannelType::Slack,
            "email" => ChannelType::Email,
            "webhook" => ChannelType::Webhook,
            "pager_duty" => ChannelType::PagerDuty,
            _ => ChannelType::Webhook,
        };
        let config: ChannelConfig = serde_json::from_value(self.config)?;
        let min_severity = match self.min_severity.as_str() {
            "info" => AlertSeverity::Info,
            "critical" => AlertSeverity::Critical,
            "emergency" => AlertSeverity::Emergency,
            _ => AlertSeverity::Warning,
        };

        Ok(NotificationChannel {
            id: self.id,
            name: self.name,
            channel_type,
            config,
            min_severity,
            enabled: self.enabled,
            created_at: self.created_at,
        })
    }
}
