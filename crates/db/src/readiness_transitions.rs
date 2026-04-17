//! Readiness transition history.
//!
//! Schema: see migration 036. The refresh task writes one row per
//! alertable transition (Ready → Degraded / Misconfigured / back to
//! Ready). The dashboard and postmortem tooling read from here to
//! reconstruct incident timelines.
//!
//! Writes are on a hot-ish path (every 60s from the refresh task,
//! typically 0 rows, sometimes 1-2). Reads come from an admin
//! dashboard — low cardinality.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::CustodyDb;

/// One historical transition. Kept serializable so the gateway can
/// return them verbatim from the history endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessTransition {
    pub id: i64,
    pub feature: String,
    pub kind: String,
    pub reason: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl CustodyDb {
    /// Insert one transition. Failures log a warning but don't
    /// propagate — the live readiness signal is the source of truth
    /// for handler gates; losing a history row is recoverable, while
    /// a crash-loop on DB hiccup is not.
    pub async fn insert_readiness_transition(
        &self,
        feature: &str,
        kind: &str,
        reason: Option<&str>,
    ) {
        let res = sqlx::query(
            "INSERT INTO readiness_transitions (feature, kind, reason) VALUES ($1, $2, $3)",
        )
        .bind(feature)
        .bind(kind)
        .bind(reason)
        .execute(&self.pool)
        .await;
        if let Err(e) = res {
            tracing::warn!(
                target: "custody::readiness::history",
                feature = %feature,
                kind = %kind,
                error = %e,
                "Failed to persist readiness transition — live state still accurate, history row lost"
            );
        }
    }

    /// Delete transitions older than `before`. Called periodically
    /// by the retention task; returns the number of rows pruned so
    /// the task can log progress.
    pub async fn prune_readiness_transitions(
        &self,
        before: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let r = sqlx::query("DELETE FROM readiness_transitions WHERE occurred_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    /// Return transitions newer than `since`, newest first, capped
    /// at `limit`. `since = None` and `limit = 200` give the
    /// dashboard's default "recent incidents" view.
    pub async fn list_readiness_transitions(
        &self,
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<ReadinessTransition>, sqlx::Error> {
        let rows = if let Some(since) = since {
            sqlx::query_as::<_, (i64, String, String, Option<String>, DateTime<Utc>)>(
                "SELECT id, feature, kind, reason, occurred_at \
                 FROM readiness_transitions \
                 WHERE occurred_at >= $1 \
                 ORDER BY occurred_at DESC \
                 LIMIT $2",
            )
            .bind(since)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, (i64, String, String, Option<String>, DateTime<Utc>)>(
                "SELECT id, feature, kind, reason, occurred_at \
                 FROM readiness_transitions \
                 ORDER BY occurred_at DESC \
                 LIMIT $1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows
            .into_iter()
            .map(|(id, feature, kind, reason, occurred_at)| ReadinessTransition {
                id,
                feature,
                kind,
                reason,
                occurred_at,
            })
            .collect())
    }
}
