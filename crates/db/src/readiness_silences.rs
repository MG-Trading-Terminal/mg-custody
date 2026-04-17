//! Readiness alert silences.
//!
//! Schema: see migration 037. An operator creates a silence during
//! planned maintenance so the refresh task's webhook + log alerting
//! stays quiet; the silence does NOT affect the live feature state
//! (handlers still 503 on a genuinely-degraded feature).
//!
//! Queries are tiny — one per minute on the refresh path, plus
//! whatever the dashboard asks — and the table is bounded by how
//! many features an operator silences simultaneously (handful),
//! so we don't need anything more sophisticated than a straight
//! SELECT under an indexed WHERE.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::CustodyDb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessSilence {
    pub id: i64,
    pub feature: String,
    /// When the silence window opens. For "silence now" this equals
    /// `created_at`; for scheduled maintenance it can be in the
    /// future and the alert suppression kicks in only after this
    /// moment.
    pub starts_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub reason: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl CustodyDb {
    pub async fn insert_readiness_silence(
        &self,
        feature: &str,
        starts_at: Option<DateTime<Utc>>,
        expires_at: DateTime<Utc>,
        reason: &str,
        created_by: &str,
    ) -> Result<ReadinessSilence, sqlx::Error> {
        // When `starts_at` is None, the DB default `clock_timestamp()`
        // kicks in — caller gets "silence from right now". RETURNING
        // gives us the exact value the DB stored.
        let row: (i64, DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
            "INSERT INTO readiness_silences (feature, starts_at, expires_at, reason, created_by) \
             VALUES ($1, COALESCE($2, clock_timestamp()), $3, $4, $5) \
             RETURNING id, starts_at, created_at",
        )
        .bind(feature)
        .bind(starts_at)
        .bind(expires_at)
        .bind(reason)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(ReadinessSilence {
            id: row.0,
            feature: feature.to_string(),
            starts_at: row.1,
            expires_at,
            reason: reason.to_string(),
            created_by: created_by.to_string(),
            created_at: row.2,
        })
    }

    /// Return every silence whose `expires_at` is still in the
    /// future. The refresh task calls this on every tick; the
    /// partial index on the table keeps the query cheap.
    pub async fn list_active_silences(&self) -> Result<Vec<ReadinessSilence>, sqlx::Error> {
        // "Active" = the silence window is open NOW: it started in
        // the past (or at clock_timestamp) and has not yet expired.
        // Scheduled silences whose `starts_at` is still in the
        // future are NOT included — they don't suppress alerts yet.
        let rows = sqlx::query_as::<_, (i64, String, DateTime<Utc>, DateTime<Utc>, String, String, DateTime<Utc>)>(
            "SELECT id, feature, starts_at, expires_at, reason, created_by, created_at \
             FROM readiness_silences \
             WHERE starts_at <= clock_timestamp() AND expires_at > clock_timestamp() \
             ORDER BY expires_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, feature, starts_at, expires_at, reason, created_by, created_at)| ReadinessSilence {
                id,
                feature,
                starts_at,
                expires_at,
                reason,
                created_by,
                created_at,
            })
            .collect())
    }

    /// Return every silence (active and expired), newest first,
    /// capped at `limit`. Used by postmortem tooling: "what was
    /// silenced during last Thursday's incident?".
    pub async fn list_silence_history(
        &self,
        limit: i64,
    ) -> Result<Vec<ReadinessSilence>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (i64, String, DateTime<Utc>, DateTime<Utc>, String, String, DateTime<Utc>)>(
            "SELECT id, feature, starts_at, expires_at, reason, created_by, created_at \
             FROM readiness_silences \
             ORDER BY created_at DESC \
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, feature, starts_at, expires_at, reason, created_by, created_at)| ReadinessSilence {
                id,
                feature,
                starts_at,
                expires_at,
                reason,
                created_by,
                created_at,
            })
            .collect())
    }

    /// End a silence early. Returns true when a live silence was
    /// actually ended (caller can report 404 on false).
    pub async fn end_readiness_silence(&self, feature: &str) -> Result<bool, sqlx::Error> {
        let r = sqlx::query(
            "UPDATE readiness_silences \
             SET expires_at = clock_timestamp() \
             WHERE feature = $1 AND expires_at > clock_timestamp()",
        )
        .bind(feature)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }
}
