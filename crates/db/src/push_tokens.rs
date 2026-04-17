//! Push notification device-token persistence.
//!
//! Before this module existed, the gateway's `/v1/user/devices`
//! routes accepted registrations and replied 201 without ever writing
//! to the DB — a user would register their phone, reboot the service,
//! and silently stop receiving notifications. This is that module.
//!
//! The table was created by migration 020. The schema uses a UNIQUE
//! constraint on `token`, so re-registering the same device upserts
//! (cheap way to rotate which user owns a given device token when
//! someone signs out and back in on a shared phone).

use chrono::{DateTime, Utc};
use custody_common::push_notifications::{DeviceToken, Platform};
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Register (or claim) a device token. On conflict with the UNIQUE
    /// constraint, the existing row is reassigned to the current user
    /// and its platform / last_used_at are refreshed. The returned
    /// `DeviceToken` reflects the row as it exists in the DB after
    /// the upsert, not just what the caller passed in.
    pub async fn upsert_device_token(
        &self,
        t: &DeviceToken,
    ) -> anyhow::Result<DeviceToken> {
        let row: DeviceTokenRow = sqlx::query_as(
            "INSERT INTO device_tokens (id, user_id, platform, token, created_at, last_used_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (token) DO UPDATE
             SET user_id      = EXCLUDED.user_id,
                 platform     = EXCLUDED.platform,
                 last_used_at = NOW()
             RETURNING id, user_id, platform, token, created_at, last_used_at",
        )
        .bind(t.id)
        .bind(t.user_id)
        .bind(platform_to_str(t.platform))
        .bind(&t.token)
        .bind(t.created_at)
        .bind(t.last_used_at)
        .fetch_one(&self.pool)
        .await?;
        row_to_token(row)
    }

    /// List every device registered by `user_id`, newest first.
    pub async fn list_device_tokens(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<DeviceToken>> {
        let rows: Vec<DeviceTokenRow> = sqlx::query_as(
            "SELECT id, user_id, platform, token, created_at, last_used_at
             FROM device_tokens
             WHERE user_id = $1
             ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_token).collect()
    }

    /// Remove a device token. Scoped by `(id, user_id)` so one user
    /// can't delete another's device even if they guess the UUID.
    /// Returns `true` when a row was removed.
    pub async fn delete_device_token(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> anyhow::Result<bool> {
        let r = sqlx::query(
            "DELETE FROM device_tokens WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// Stamp `last_used_at = NOW()` for the given device. Called by
    /// the push-send path after a successful delivery so the dashboard
    /// can show which devices are stale.
    pub async fn touch_device_token(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE device_tokens SET last_used_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Row mapping
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DeviceTokenRow {
    id: Uuid,
    user_id: Uuid,
    platform: String,
    token: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

fn row_to_token(r: DeviceTokenRow) -> anyhow::Result<DeviceToken> {
    Ok(DeviceToken {
        id: r.id,
        user_id: r.user_id,
        platform: str_to_platform(&r.platform)?,
        token: r.token,
        created_at: r.created_at,
        last_used_at: r.last_used_at,
    })
}

fn platform_to_str(p: Platform) -> &'static str {
    match p {
        Platform::Ios => "ios",
        Platform::Android => "android",
        Platform::Web => "web",
    }
}

fn str_to_platform(s: &str) -> anyhow::Result<Platform> {
    match s {
        "ios" => Ok(Platform::Ios),
        "android" => Ok(Platform::Android),
        "web" => Ok(Platform::Web),
        other => Err(anyhow::anyhow!("unknown device token platform: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_roundtrip() {
        for p in [Platform::Ios, Platform::Android, Platform::Web] {
            assert_eq!(str_to_platform(platform_to_str(p)).unwrap(), p);
        }
    }

    #[test]
    fn unknown_platform_rejected() {
        assert!(str_to_platform("desktop").is_err());
    }
}
