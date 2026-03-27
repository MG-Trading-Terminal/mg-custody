//! Payment flow and transaction label persistence.

use chrono::{DateTime, Utc};
use custody_common::payment_flows::{PaymentFlow, PaymentFlowStatus};
use custody_common::analytics::TransactionLabel;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    // --- Payment flows ---

    pub async fn insert_payment_flow(&self, flow: &PaymentFlow) -> anyhow::Result<()> {
        let steps = serde_json::to_value(&flow.steps)?;
        let status = serde_json::to_string(&flow.status)?;
        sqlx::query(
            "INSERT INTO payment_flows (id, name, status, steps, created_at, executed_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(flow.id).bind(&flow.name).bind(status.trim_matches('"'))
        .bind(&steps).bind(flow.created_at).bind(flow.executed_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_payment_flow(&self, id: Uuid) -> anyhow::Result<Option<PaymentFlow>> {
        let row: Option<(Uuid, String, String, serde_json::Value, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, name, status, steps, created_at, executed_at FROM payment_flows WHERE id = $1",
        ).bind(id).fetch_optional(&self.pool).await?;

        Ok(row.map(|(id, name, status, steps, created_at, executed_at)| {
            let status = match status.as_str() {
                "pending" => PaymentFlowStatus::Pending,
                "executing" => PaymentFlowStatus::Executing,
                "completed" => PaymentFlowStatus::Completed,
                "failed" => PaymentFlowStatus::Failed,
                _ => PaymentFlowStatus::Draft,
            };
            let steps = serde_json::from_value(steps).unwrap_or_default();
            PaymentFlow { id, name, status, steps, created_at, executed_at }
        }))
    }

    pub async fn list_payment_flows(&self) -> anyhow::Result<Vec<PaymentFlow>> {
        let rows: Vec<(Uuid, String, String, serde_json::Value, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, name, status, steps, created_at, executed_at FROM payment_flows ORDER BY created_at DESC LIMIT 100",
        ).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|(id, name, status, steps, created_at, executed_at)| {
            let status = match status.as_str() {
                "pending" => PaymentFlowStatus::Pending, "executing" => PaymentFlowStatus::Executing,
                "completed" => PaymentFlowStatus::Completed, "failed" => PaymentFlowStatus::Failed,
                _ => PaymentFlowStatus::Draft,
            };
            PaymentFlow { id, name, status, steps: serde_json::from_value(steps).unwrap_or_default(), created_at, executed_at }
        }).collect())
    }

    // --- Transaction labels ---

    pub async fn upsert_transaction_label(&self, label: &TransactionLabel) -> anyhow::Result<()> {
        let labels: Vec<&str> = label.labels.iter().map(|s| s.as_str()).collect();
        sqlx::query(
            "INSERT INTO transaction_labels (transaction_id, labels, department, cost_center, counterparty_name, purpose, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (transaction_id) DO UPDATE SET
                labels = $2, department = $3, cost_center = $4,
                counterparty_name = $5, purpose = $6, updated_at = $7",
        )
        .bind(label.transaction_id).bind(&labels)
        .bind(&label.department).bind(&label.cost_center)
        .bind(&label.counterparty_name).bind(&label.purpose)
        .bind(label.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_transaction_label(&self, tx_id: Uuid) -> anyhow::Result<Option<TransactionLabel>> {
        let row: Option<(Uuid, Vec<String>, Option<String>, Option<String>, Option<String>, Option<String>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT transaction_id, labels, department, cost_center, counterparty_name, purpose, updated_at
             FROM transaction_labels WHERE transaction_id = $1",
        ).bind(tx_id).fetch_optional(&self.pool).await?;

        Ok(row.map(|(tx_id, labels, dept, cc, cpty, purpose, updated)| {
            TransactionLabel { transaction_id: tx_id, labels, department: dept, cost_center: cc, counterparty_name: cpty, purpose, updated_at: updated }
        }))
    }
}
