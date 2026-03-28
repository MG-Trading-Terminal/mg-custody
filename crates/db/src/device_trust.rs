//! Device trust and concurrent session limit enforcement.
//!
//! Security properties:
//! - Device fingerprints are stored as-is (already a hash from the client).
//! - Trust records expire after `trusted_until`; background task prunes stale rows.
//! - Session limit: at most MAX_CONCURRENT_SESSIONS active sessions per user.
//!   When the limit is exceeded, the oldest session (by created_at) is revoked.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::CustodyDb;

/// Default maximum concurrent sessions per user.
pub const DEFAULT_MAX_CONCURRENT_SESSIONS: i64 = 5;

/// Default device trust duration in days.
pub const DEFAULT_TRUST_DURATION_DAYS: i64 = 30;

/// A trusted device record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrustedDevice {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_fingerprint: String,
    /// Human-readable name e.g. "Chrome 122 on macOS Ventura".
    pub name: Option<String>,
    pub trusted_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct TrustedDeviceRow {
    id: Uuid,
    user_id: Uuid,
    device_fingerprint: String,
    name: Option<String>,
    trusted_until: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl TrustedDeviceRow {
    fn into_device(self) -> TrustedDevice {
        TrustedDevice {
            id: self.id,
            user_id: self.user_id,
            device_fingerprint: self.device_fingerprint,
            name: self.name,
            trusted_until: self.trusted_until,
            created_at: self.created_at,
        }
    }
}

impl CustodyDb {
    // -----------------------------------------------------------------------
    // Device trust
    // -----------------------------------------------------------------------

    /// Check whether a device is currently trusted for the given user.
    ///
    /// Returns true if a non-expired trust record exists for (user_id, fingerprint).
    pub async fn is_device_trusted(
        &self,
        user_id: Uuid,
        fingerprint: &str,
    ) -> anyhow::Result<bool> {
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM trusted_devices
             WHERE user_id = $1 AND device_fingerprint = $2 AND trusted_until > NOW()
             LIMIT 1",
        )
        .bind(user_id)
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    /// Create or extend a device trust record.
    ///
    /// If the device was previously trusted but expired, this refreshes it.
    /// If it was never trusted, creates a new record.
    pub async fn trust_device(
        &self,
        user_id: Uuid,
        fingerprint: &str,
        name: Option<&str>,
        duration_days: i64,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        let trusted_until = Utc::now() + Duration::days(duration_days);

        sqlx::query(
            r#"
            INSERT INTO trusted_devices (id, user_id, device_fingerprint, name, trusted_until)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id, device_fingerprint) DO UPDATE SET
                name          = EXCLUDED.name,
                trusted_until = EXCLUDED.trusted_until
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(fingerprint)
        .bind(name)
        .bind(trusted_until)
        .execute(&self.pool)
        .await?;

        // Re-fetch the actual id in case ON CONFLICT updated an existing row.
        let actual_id: (Uuid,) = sqlx::query_as(
            "SELECT id FROM trusted_devices WHERE user_id = $1 AND device_fingerprint = $2",
        )
        .bind(user_id)
        .bind(fingerprint)
        .fetch_one(&self.pool)
        .await?;

        Ok(actual_id.0)
    }

    /// List all trusted devices for a user (active and expired).
    pub async fn list_trusted_devices(&self, user_id: Uuid) -> anyhow::Result<Vec<TrustedDevice>> {
        let rows: Vec<TrustedDeviceRow> = sqlx::query_as(
            "SELECT id, user_id, device_fingerprint, name, trusted_until, created_at
             FROM trusted_devices
             WHERE user_id = $1
             ORDER BY trusted_until DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into_device()).collect())
    }

    /// Revoke a specific trusted device by its ID.
    pub async fn revoke_trusted_device(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM trusted_devices WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Revoke all trusted devices for a user (e.g., on password change).
    pub async fn revoke_all_trusted_devices(&self, user_id: Uuid) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM trusted_devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Prune expired trust records.
    pub async fn cleanup_expired_trusted_devices(&self) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM trusted_devices WHERE trusted_until < NOW()")
            .execute(&self.pool)
            .await?;
        let n = result.rows_affected();
        if n > 0 {
            tracing::info!(cleaned = n, "Expired trusted_devices pruned");
        }
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Session limit enforcement
    // -----------------------------------------------------------------------

    /// Enforce a maximum number of concurrent active sessions per user.
    ///
    /// If the user currently has >= `max_sessions` active non-revoked sessions,
    /// the oldest ones (by `created_at ASC`) are revoked until the count is
    /// strictly below `max_sessions`. This is called *after* creating the new
    /// session so the new one is counted.
    ///
    /// Returns the number of sessions revoked.
    pub async fn enforce_session_limit(
        &self,
        user_id: Uuid,
        max_sessions: i64,
    ) -> anyhow::Result<u64> {
        // Count active sessions for this user.
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sessions
             WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > NOW()",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let excess = count.0 - max_sessions;
        if excess <= 0 {
            return Ok(0);
        }

        // Revoke the `excess` oldest sessions.
        let result = sqlx::query(
            r#"
            UPDATE sessions SET revoked_at = NOW()
            WHERE token_hash IN (
                SELECT token_hash FROM sessions
                WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > NOW()
                ORDER BY created_at ASC
                LIMIT $2
            )
            "#,
        )
        .bind(user_id)
        .bind(excess)
        .execute(&self.pool)
        .await?;

        let revoked = result.rows_affected();
        if revoked > 0 {
            tracing::info!(
                user_id = %user_id,
                revoked,
                max_sessions,
                "Enforced session limit: revoked oldest sessions"
            );
        }
        Ok(revoked)
    }

    /// Count active sessions for a user.
    pub async fn count_active_sessions(&self, user_id: Uuid) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sessions
             WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > NOW()",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}

// ---------------------------------------------------------------------------
// Tests (unit, no DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_max_sessions_is_five() {
        assert_eq!(DEFAULT_MAX_CONCURRENT_SESSIONS, 5);
    }

    #[test]
    fn default_trust_duration_is_30_days() {
        assert_eq!(DEFAULT_TRUST_DURATION_DAYS, 30);
    }

    #[test]
    fn trusted_device_fields() {
        let device = TrustedDevice {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            device_fingerprint: "abc123".into(),
            name: Some("Chrome on macOS".into()),
            trusted_until: Utc::now() + Duration::days(30),
            created_at: Utc::now(),
        };
        assert!(device.trusted_until > Utc::now());
        assert_eq!(device.device_fingerprint, "abc123");
    }

    #[test]
    fn trusted_device_serializes() {
        let device = TrustedDevice {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            device_fingerprint: "fp_hash".into(),
            name: None,
            trusted_until: Utc::now() + Duration::days(10),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&device).unwrap();
        assert!(json.contains("device_fingerprint"));
        assert!(json.contains("fp_hash"));
    }
}
