//! Custody statement persistence.
//!
//! Statements are generated on demand and stored for fast retrieval.
//! The attestation hash is stored alongside the full JSON so recipients
//! can re-verify integrity without access to the database.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::CustodyDb;

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

/// Lightweight statement metadata (for listing endpoints).
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct StatementMeta {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub year: i32,
    pub month: i32,
    pub generated_at: DateTime<Utc>,
    pub total_aum_usd: Option<Decimal>,
    pub attestation_hash: String,
}

/// Full stored statement (metadata + JSON blob).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StatementRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub year: i32,
    pub month: i32,
    pub generated_at: DateTime<Utc>,
    pub total_aum_usd: Option<Decimal>,
    pub attestation_hash: String,
    pub statement_json: String,
}

// ---------------------------------------------------------------------------
// CustodyDb impl
// ---------------------------------------------------------------------------

impl CustodyDb {
    /// Persist a generated statement. Idempotent on (workspace_id, year, month).
    pub async fn upsert_statement(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        year: i32,
        month: i32,
        total_aum_usd: Decimal,
        attestation_hash: &str,
        statement_json: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO custody_statements
                (id, workspace_id, year, month, generated_at, total_aum_usd, attestation_hash, statement_json)
            VALUES ($1, $2, $3, $4, NOW(), $5, $6, $7)
            ON CONFLICT (workspace_id, year, month) DO UPDATE SET
                id               = EXCLUDED.id,
                generated_at     = EXCLUDED.generated_at,
                total_aum_usd    = EXCLUDED.total_aum_usd,
                attestation_hash = EXCLUDED.attestation_hash,
                statement_json   = EXCLUDED.statement_json
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(year)
        .bind(month)
        .bind(total_aum_usd)
        .bind(attestation_hash)
        .bind(statement_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all statements for a workspace, ordered newest first.
    pub async fn list_statements(&self, workspace_id: Uuid) -> anyhow::Result<Vec<StatementMeta>> {
        let rows: Vec<StatementMeta> = sqlx::query_as(
            r#"
            SELECT id, workspace_id, year, month, generated_at, total_aum_usd, attestation_hash
            FROM custody_statements
            WHERE workspace_id = $1
            ORDER BY year DESC, month DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Get a specific statement by (workspace_id, year, month).
    pub async fn get_statement(
        &self,
        workspace_id: Uuid,
        year: i32,
        month: i32,
    ) -> anyhow::Result<Option<StatementRow>> {
        let row: Option<StatementRow> = sqlx::query_as(
            r#"
            SELECT id, workspace_id, year, month, generated_at, total_aum_usd, attestation_hash, statement_json
            FROM custody_statements
            WHERE workspace_id = $1 AND year = $2 AND month = $3
            "#,
        )
        .bind(workspace_id)
        .bind(year)
        .bind(month)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Delete a statement (admin only).
    pub async fn delete_statement(
        &self,
        workspace_id: Uuid,
        year: i32,
        month: i32,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "DELETE FROM custody_statements WHERE workspace_id = $1 AND year = $2 AND month = $3",
        )
        .bind(workspace_id)
        .bind(year)
        .bind(month)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// Tests (unit, no DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_meta_fields() {
        let meta = StatementMeta {
            id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            year: 2026,
            month: 3,
            generated_at: Utc::now(),
            total_aum_usd: Some(rust_decimal_macros::dec!(1000000)),
            attestation_hash: "a".repeat(64),
        };
        assert_eq!(meta.year, 2026);
        assert_eq!(meta.month, 3);
        assert_eq!(meta.attestation_hash.len(), 64);
    }
}
