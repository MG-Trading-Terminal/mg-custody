//! Vault policies, approval workflows, transaction notes persistence.

use chrono::{DateTime, Utc};
use custody_common::governance::*;
use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    // --- Vault policies ---

    pub async fn insert_vault_policy(&self, p: &VaultPolicy) -> anyhow::Result<()> {
        let rules = serde_json::to_value(&p.rules)?;
        sqlx::query(
            "INSERT INTO vault_policies (id, vault_id, name, rules, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(p.id).bind(p.vault_id).bind(&p.name)
        .bind(&rules).bind(p.enabled)
        .bind(p.created_at).bind(p.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_vault_policy(&self, id: Uuid) -> anyhow::Result<Option<VaultPolicy>> {
        let row: Option<VaultPolicyRow> = sqlx::query_as(
            "SELECT id, vault_id, name, rules, enabled, created_at, updated_at
             FROM vault_policies WHERE id = $1",
        ).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.and_then(|r| r.into_policy().ok()))
    }

    pub async fn list_vault_policies(&self, vault_id: Uuid) -> anyhow::Result<Vec<VaultPolicy>> {
        let rows: Vec<VaultPolicyRow> = sqlx::query_as(
            "SELECT id, vault_id, name, rules, enabled, created_at, updated_at
             FROM vault_policies WHERE vault_id = $1 ORDER BY created_at",
        ).bind(vault_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().filter_map(|r| r.into_policy().ok()).collect())
    }

    pub async fn update_vault_policy(
        &self, id: Uuid, name: Option<&str>, rules: Option<&[PolicyRule]>, enabled: Option<bool>,
    ) -> anyhow::Result<bool> {
        let rules_json: Option<serde_json::Value> = rules.map(|r| serde_json::to_value(r).unwrap());
        let result = sqlx::query(
            "UPDATE vault_policies SET
                name = COALESCE($2, name),
                rules = COALESCE($3, rules),
                enabled = COALESCE($4, enabled),
                updated_at = NOW()
             WHERE id = $1",
        ).bind(id).bind(name).bind(rules_json).bind(enabled)
        .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_vault_policy(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM vault_policies WHERE id = $1")
            .bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // --- Approval workflows ---

    pub async fn insert_approval_workflow(&self, w: &ApprovalWorkflow) -> anyhow::Result<()> {
        let decisions = serde_json::to_value(&w.decisions)?;
        sqlx::query(
            "INSERT INTO approval_workflows (id, withdrawal_id, vault_id, required_approvals, current_approvals, decisions, status, expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(w.id).bind(w.withdrawal_id).bind(w.vault_id)
        .bind(w.required_approvals as i32).bind(w.current_approvals as i32)
        .bind(&decisions).bind(w.status.to_string())
        .bind(w.expires_at).bind(w.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_approval_workflow(&self, withdrawal_id: Uuid) -> anyhow::Result<Option<ApprovalWorkflow>> {
        let row: Option<ApprovalWorkflowRow> = sqlx::query_as(
            "SELECT id, withdrawal_id, vault_id, required_approvals, current_approvals, decisions, status, expires_at, created_at
             FROM approval_workflows WHERE withdrawal_id = $1",
        ).bind(withdrawal_id).fetch_optional(&self.pool).await?;
        Ok(row.and_then(|r| r.into_workflow().ok()))
    }

    pub async fn list_pending_approvals_for_vault(&self, vault_id: Uuid) -> anyhow::Result<Vec<ApprovalWorkflow>> {
        let rows: Vec<ApprovalWorkflowRow> = sqlx::query_as(
            "SELECT id, withdrawal_id, vault_id, required_approvals, current_approvals, decisions, status, expires_at, created_at
             FROM approval_workflows WHERE vault_id = $1 AND status = 'pending' ORDER BY created_at",
        ).bind(vault_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().filter_map(|r| r.into_workflow().ok()).collect())
    }

    pub async fn add_approval_decision_to_workflow(
        &self, id: Uuid, decision: &ApprovalDecision,
    ) -> anyhow::Result<()> {
        let decision_json = serde_json::to_value(decision)?;
        sqlx::query(
            "UPDATE approval_workflows SET
                decisions = decisions || $2::jsonb,
                current_approvals = current_approvals + CASE WHEN $3 = 'approve' THEN 1 ELSE 0 END,
                status = CASE
                    WHEN $3 = 'reject' THEN 'rejected'
                    WHEN current_approvals + 1 >= required_approvals THEN 'approved'
                    ELSE 'pending'
                END
             WHERE id = $1",
        )
        .bind(id)
        .bind(decision_json)
        .bind(match decision.decision { Decision::Approve => "approve", Decision::Reject => "reject" })
        .execute(&self.pool).await?;
        Ok(())
    }

    // --- Transaction notes ---

    pub async fn insert_transaction_note(&self, n: &TransactionNote) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO transaction_notes (id, transaction_id, note, author, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(n.id).bind(n.transaction_id).bind(&n.note)
        .bind(&n.author).bind(n.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_transaction_notes(&self, transaction_id: Uuid) -> anyhow::Result<Vec<TransactionNote>> {
        let rows: Vec<TransactionNoteRow> = sqlx::query_as(
            "SELECT id, transaction_id, note, author, created_at
             FROM transaction_notes WHERE transaction_id = $1 ORDER BY created_at",
        ).bind(transaction_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| r.into_note()).collect())
    }
}

// --- Row types ---

#[derive(sqlx::FromRow)]
struct VaultPolicyRow {
    id: Uuid, vault_id: Uuid, name: String,
    rules: serde_json::Value, enabled: bool,
    created_at: DateTime<Utc>, updated_at: DateTime<Utc>,
}

impl VaultPolicyRow {
    fn into_policy(self) -> anyhow::Result<VaultPolicy> {
        let rules: Vec<PolicyRule> = serde_json::from_value(self.rules)?;
        Ok(VaultPolicy {
            id: self.id, vault_id: self.vault_id, name: self.name,
            rules, enabled: self.enabled,
            created_at: self.created_at, updated_at: self.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ApprovalWorkflowRow {
    id: Uuid, withdrawal_id: Uuid, vault_id: Uuid,
    required_approvals: i32, current_approvals: i32,
    decisions: serde_json::Value, status: String,
    expires_at: DateTime<Utc>, created_at: DateTime<Utc>,
}

impl ApprovalWorkflowRow {
    fn into_workflow(self) -> anyhow::Result<ApprovalWorkflow> {
        let decisions: Vec<ApprovalDecision> = serde_json::from_value(self.decisions)?;
        let status = match self.status.as_str() {
            "approved" => ApprovalStatus::Approved,
            "rejected" => ApprovalStatus::Rejected,
            "expired" => ApprovalStatus::Expired,
            _ => ApprovalStatus::Pending,
        };
        Ok(ApprovalWorkflow {
            id: self.id, withdrawal_id: self.withdrawal_id, vault_id: self.vault_id,
            required_approvals: self.required_approvals as u32,
            current_approvals: self.current_approvals as u32,
            decisions, status,
            expires_at: self.expires_at, created_at: self.created_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct TransactionNoteRow {
    id: Uuid, transaction_id: Uuid,
    note: String, author: String,
    created_at: DateTime<Utc>,
}

impl TransactionNoteRow {
    fn into_note(self) -> TransactionNote {
        TransactionNote {
            id: self.id, transaction_id: self.transaction_id,
            note: self.note, author: self.author,
            created_at: self.created_at,
        }
    }
}
