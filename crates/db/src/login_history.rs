//! Login history persistence.
//!
//! Records every authentication attempt — successful and failed — so users can
//! audit recent access from the dashboard and so the brute-force lockout logic
//! has a reliable count of recent failures.
//!
//! # Brute-force protection
//! The login handler calls `count_failed_logins(user_id, since)` after a failed
//! attempt.  If the count reaches 5 within 15 minutes, the account is considered
//! temporarily locked.  The handler returns 429 with a `Retry-After` header and
//! records a `failure_reason = 'account_locked'` row so the event is auditable.
//!
//! # Cleanup
//! `cleanup_old_logins(days)` deletes rows older than `days` days.  Intended to
//! run as a nightly background task to keep the table bounded.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::CustodyDb;

/// A single login attempt record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginHistoryEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub ip_address: String,
    pub user_agent: String,
    /// Authentication method: `"password"`, `"totp"`, `"fido2"`, `"backup_code"`, `"email_otp"`.
    pub method: String,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LoginHistoryRow {
    id: Uuid,
    user_id: Uuid,
    ip_address: String,
    user_agent: String,
    method: String,
    success: bool,
    failure_reason: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<LoginHistoryRow> for LoginHistoryEntry {
    fn from(r: LoginHistoryRow) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            ip_address: r.ip_address,
            user_agent: r.user_agent,
            method: r.method,
            success: r.success,
            failure_reason: r.failure_reason,
            created_at: r.created_at,
        }
    }
}

impl CustodyDb {
    /// Record a login attempt.
    ///
    /// `method` is a free-form string identifying the auth step that produced
    /// this event (e.g. `"password"`, `"totp"`, `"fido2"`, `"email_otp"`).
    ///
    /// `failure_reason` is `None` on success; otherwise a machine-readable
    /// slug such as `"invalid_password"` or `"account_locked"`.
    pub async fn record_login(
        &self,
        user_id: Uuid,
        ip: &str,
        user_agent: &str,
        method: &str,
        success: bool,
        failure_reason: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO login_history
                (user_id, ip_address, user_agent, method, success, failure_reason)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(user_id)
        .bind(ip)
        .bind(user_agent)
        .bind(method)
        .bind(success)
        .bind(failure_reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List the most recent `limit` login history entries for `user_id`, newest first.
    ///
    /// `limit` is capped at 200 to prevent unbounded result sets.
    pub async fn list_login_history(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<LoginHistoryEntry>> {
        let capped_limit = limit.clamp(1, 200);
        let rows: Vec<LoginHistoryRow> = sqlx::query_as(
            "SELECT id, user_id, ip_address, user_agent, method, success, failure_reason, created_at
             FROM login_history
             WHERE user_id = $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(user_id)
        .bind(capped_limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LoginHistoryEntry::from).collect())
    }

    /// Count failed login attempts for `user_id` since `since`.
    ///
    /// Used by brute-force detection: if this returns ≥ 5 within the last 15
    /// minutes, the login handler rejects the attempt.
    pub async fn count_failed_logins(
        &self,
        user_id: Uuid,
        since: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM login_history
             WHERE user_id = $1 AND success = false AND created_at >= $2",
        )
        .bind(user_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Delete login history rows older than `older_than_days` days.
    ///
    /// Returns the number of rows deleted.  Intended for a nightly maintenance task.
    pub async fn cleanup_old_logins(&self, older_than_days: i32) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM login_history
             WHERE created_at < NOW() - ($1 || ' days')::interval",
        )
        .bind(older_than_days)
        .execute(&self.pool)
        .await?;
        let count = result.rows_affected();
        if count > 0 {
            tracing::info!(deleted = count, "Login history cleaned up");
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// Construct a LoginHistoryEntry directly — no DB needed.
    fn make_entry(success: bool, failure_reason: Option<&str>, minutes_ago: i64) -> LoginHistoryEntry {
        LoginHistoryEntry {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            ip_address: "1.2.3.4".into(),
            user_agent: "TestAgent/1.0".into(),
            method: "password".into(),
            success,
            failure_reason: failure_reason.map(|s| s.to_string()),
            created_at: Utc::now() - Duration::minutes(minutes_ago),
        }
    }

    #[test]
    fn entry_serialization_success() {
        let entry = make_entry(true, None, 0);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"failure_reason\":null"));
        assert!(json.contains("\"method\":\"password\""));
    }

    #[test]
    fn entry_serialization_failure() {
        let entry = make_entry(false, Some("invalid_password"), 1);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("invalid_password"));
    }

    #[test]
    fn brute_force_threshold_logic() {
        // Simulate counting recent failures to verify the lockout logic.
        let now = Utc::now();
        let window = Duration::minutes(15);
        let since = now - window;

        // Entries within the window
        let within: Vec<LoginHistoryEntry> = (0..5)
            .map(|i| make_entry(false, Some("invalid_password"), i))
            .collect();
        let count_within = within.iter().filter(|e| e.created_at >= since).count();
        assert_eq!(count_within, 5, "5 failures within window should trigger lockout");

        // Entry outside the window
        let old = make_entry(false, Some("invalid_password"), 20);
        assert!(old.created_at < since, "old entry is before the window");
    }

    #[test]
    fn limit_cap_at_200() {
        // Verify the clamp used in list_login_history.
        let requested: i64 = 1000;
        let capped = requested.clamp(1, 200);
        assert_eq!(capped, 200);
    }

    #[test]
    fn limit_minimum_at_1() {
        let requested: i64 = 0;
        let capped = requested.clamp(1, 200);
        assert_eq!(capped, 1);
    }

    #[test]
    fn method_variants_roundtrip_serialization() {
        for method in &["password", "totp", "fido2", "backup_code", "email_otp"] {
            let entry = LoginHistoryEntry {
                id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                ip_address: "127.0.0.1".into(),
                user_agent: "test".into(),
                method: method.to_string(),
                success: true,
                failure_reason: None,
                created_at: Utc::now(),
            };
            let json = serde_json::to_string(&entry).unwrap();
            assert!(json.contains(method), "method {method} must appear in JSON");
        }
    }
}
