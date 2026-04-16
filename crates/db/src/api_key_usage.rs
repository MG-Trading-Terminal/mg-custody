//! API key usage tracking — daily counters, expiry detection.
//!
//! Each API call that authenticates via an API key should call
//! `record_api_key_usage` after successful auth. The upsert increments the
//! daily counter atomically.

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::CustodyDb;

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

/// Daily usage record for a single API key.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct DailyUsage {
    pub key_id: Uuid,
    pub date: NaiveDate,
    pub request_count: i32,
    pub last_endpoint: Option<String>,
    pub last_ip: Option<String>,
}

/// API key summary with expiry info (for expiry notifications).
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ApiKeyInfo {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    /// Days until expiry (negative if already expired).
    pub days_until_expiry: Option<i64>,
}

// ---------------------------------------------------------------------------
// CustodyDb impl
// ---------------------------------------------------------------------------

impl CustodyDb {
    /// Record a single API key request. Upserts the daily counter.
    ///
    /// Idempotent in the sense that concurrent calls increment atomically.
    pub async fn record_api_key_usage(
        &self,
        key_id: Uuid,
        endpoint: &str,
        ip: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO api_key_usage (key_id, date, request_count, last_endpoint, last_ip)
            VALUES ($1, CURRENT_DATE, 1, $2, $3)
            ON CONFLICT (key_id, date) DO UPDATE SET
                request_count = api_key_usage.request_count + 1,
                last_endpoint = EXCLUDED.last_endpoint,
                last_ip       = EXCLUDED.last_ip
            "#,
        )
        .bind(key_id)
        .bind(endpoint)
        .bind(ip)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get daily usage records for a key within a date range (inclusive).
    pub async fn get_api_key_usage(
        &self,
        key_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<DailyUsage>> {
        let rows: Vec<DailyUsage> = sqlx::query_as(
            "SELECT key_id, date, request_count, last_endpoint, last_ip
             FROM api_key_usage
             WHERE key_id = $1 AND date >= $2 AND date <= $3
             ORDER BY date",
        )
        .bind(key_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Total request count for a key over a date range.
    pub async fn total_api_key_requests(
        &self,
        key_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<i64> {
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(request_count) FROM api_key_usage
             WHERE key_id = $1 AND date >= $2 AND date <= $3",
        )
        .bind(key_id)
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0.unwrap_or(0))
    }

    /// Get API keys expiring within `within_days` days.
    ///
    /// Includes keys that are already expired (within_days=0 → expiring today).
    pub async fn get_expiring_keys(&self, within_days: i32) -> anyhow::Result<Vec<ApiKeyInfo>> {
        let rows: Vec<ApiKeyInfo> = sqlx::query_as(
            r#"
            SELECT
                id,
                name,
                enabled,
                created_at,
                expires_at,
                last_used_at,
                EXTRACT(DAY FROM (expires_at - NOW()))::BIGINT AS days_until_expiry
            FROM api_keys
            WHERE expires_at IS NOT NULL
              AND expires_at <= NOW() + ($1 || ' days')::INTERVAL
            ORDER BY expires_at ASC
            "#,
        )
        .bind(within_days)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Prune usage records older than `retain_days` days.
    pub async fn cleanup_old_api_key_usage(&self, retain_days: i32) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM api_key_usage WHERE date < CURRENT_DATE - ($1 || ' days')::INTERVAL",
        )
        .bind(retain_days)
        .execute(&self.pool)
        .await?;
        let n = result.rows_affected();
        if n > 0 {
            tracing::info!(cleaned = n, retain_days, "Old api_key_usage rows pruned");
        }
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// Tests (unit, no DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn daily_usage_fields() {
        let u = DailyUsage {
            key_id: Uuid::nil(),
            date: NaiveDate::from_ymd_opt(2026, 3, 27).unwrap(),
            request_count: 42,
            last_endpoint: Some("/v1/wallets/ETH".into()),
            last_ip: Some("10.0.0.1".into()),
        };
        assert_eq!(u.request_count, 42);
        assert_eq!(u.date, NaiveDate::from_ymd_opt(2026, 3, 27).unwrap());
    }

    #[test]
    fn daily_usage_serializes() {
        let u = DailyUsage {
            key_id: Uuid::nil(),
            date: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            request_count: 100,
            last_endpoint: None,
            last_ip: None,
        };
        let json = serde_json::to_string(&u).unwrap();
        assert!(json.contains("request_count"));
        assert!(json.contains("100"));
    }

    #[test]
    fn api_key_info_fields() {
        let info = ApiKeyInfo {
            id: Uuid::nil(),
            name: "exchange-read".into(),
            enabled: true,
            created_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(25)),
            last_used_at: None,
            days_until_expiry: Some(25),
        };
        assert_eq!(info.name, "exchange-read");
        assert!(info.enabled);
        assert_eq!(info.days_until_expiry, Some(25));
    }

    #[test]
    fn api_key_info_serializes() {
        let info = ApiKeyInfo {
            id: Uuid::nil(),
            name: "test-key".into(),
            enabled: true,
            created_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
            days_until_expiry: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-key"));
        assert!(json.contains("enabled"));
    }

    #[test]
    fn usage_aggregation_example() {
        // Simulate daily aggregation logic used in the API handler.
        let usages = [DailyUsage { key_id: Uuid::nil(), date: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(), request_count: 100, last_endpoint: None, last_ip: None },
            DailyUsage { key_id: Uuid::nil(), date: NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(), request_count: 200, last_endpoint: None, last_ip: None },
            DailyUsage { key_id: Uuid::nil(), date: NaiveDate::from_ymd_opt(2026, 3, 3).unwrap(), request_count: 50,  last_endpoint: None, last_ip: None }];
        let total: i32 = usages.iter().map(|u| u.request_count).sum();
        assert_eq!(total, 350);
    }

    #[test]
    fn expiry_detection_logic() {
        let now = Utc::now();
        let expiring_soon = ApiKeyInfo {
            id: Uuid::nil(),
            name: "expiring-key".into(),
            enabled: true,
            created_at: now,
            expires_at: Some(now + chrono::Duration::days(5)),
            last_used_at: None,
            days_until_expiry: Some(5),
        };
        // Key expiring in 5 days should be flagged when within_days=30.
        assert!(expiring_soon.days_until_expiry.unwrap() < 30);
    }

    #[test]
    fn already_expired_key_negative_days() {
        let now = Utc::now();
        let expired = ApiKeyInfo {
            id: Uuid::nil(),
            name: "old-key".into(),
            enabled: false,
            created_at: now - chrono::Duration::days(100),
            expires_at: Some(now - chrono::Duration::days(1)),
            last_used_at: None,
            days_until_expiry: Some(-1),
        };
        assert!(expired.days_until_expiry.unwrap() < 0);
    }
}
