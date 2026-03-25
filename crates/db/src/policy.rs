//! Policy persistence layer — real SQL queries.

use uuid::Uuid;
use crate::CustodyDb;

impl CustodyDb {
    /// Get all enabled policy rules, ordered by priority.
    pub async fn get_policy_rules(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT row_to_json(r) FROM policy_rules r WHERE enabled = true ORDER BY priority",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    /// Insert a policy rule.
    pub async fn insert_policy_rule(&self, rule_json: &serde_json::Value) -> anyhow::Result<()> {
        let id = rule_json["id"].as_str().unwrap_or("");
        let name = rule_json["name"].as_str().unwrap_or("");
        let priority = rule_json["priority"].as_i64().unwrap_or(100) as i32;

        sqlx::query(
            "INSERT INTO policy_rules (id, name, priority, condition, action, enabled)
             VALUES ($1::uuid, $2, $3, $4, $5, true)
             ON CONFLICT (id) DO UPDATE SET name=$2, priority=$3, condition=$4, action=$5",
        )
        .bind(id)
        .bind(name)
        .bind(priority)
        .bind(&rule_json["condition"])
        .bind(&rule_json["action"])
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a policy rule.
    pub async fn delete_policy_rule(&self, rule_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM policy_rules WHERE id = $1")
            .bind(rule_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get whitelist entries for a chain.
    pub async fn get_whitelist(&self, chain: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT row_to_json(w) FROM address_whitelist w WHERE chain = $1",
        )
        .bind(chain)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    /// Insert a whitelist entry.
    pub async fn insert_whitelist_entry(&self, entry_json: &serde_json::Value) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO address_whitelist (id, address, chain, label, added_by)
             VALUES ($1::uuid, $2, $3, $4, $5)
             ON CONFLICT (chain, address) DO NOTHING",
        )
        .bind(entry_json["id"].as_str().unwrap_or(""))
        .bind(entry_json["address"].as_str().unwrap_or(""))
        .bind(entry_json["chain"].as_str().unwrap_or(""))
        .bind(entry_json["label"].as_str().unwrap_or(""))
        .bind(entry_json["added_by"].as_str().unwrap_or(""))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get pending approval requests.
    pub async fn get_pending_approvals(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT row_to_json(a) FROM approval_requests a
             WHERE status = 'pending' AND expires_at > NOW()
             ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    /// Insert an approval request.
    pub async fn insert_approval_request(&self, withdrawal_id: Uuid, required: i32, role: &str, expires_at: chrono::DateTime<chrono::Utc>) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO approval_requests (id, withdrawal_id, required_approvals, required_role, status, expires_at)
             VALUES ($1, $2, $3, $4, 'pending', $5)",
        )
        .bind(id)
        .bind(withdrawal_id)
        .bind(required)
        .bind(role)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Add an approval decision to a request.
    pub async fn add_approval_decision(&self, request_id: Uuid, approver: &str, approved: bool) -> anyhow::Result<()> {
        let decision = serde_json::json!({
            "approver": approver,
            "approved": approved,
            "decided_at": chrono::Utc::now().to_rfc3339(),
        });

        sqlx::query(
            "UPDATE approval_requests
             SET decisions = decisions || $1::jsonb
             WHERE id = $2 AND status = 'pending'",
        )
        .bind(decision)
        .bind(request_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update an approval request status.
    pub async fn update_approval_status(&self, request_id: Uuid, status: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE approval_requests SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(request_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Check if approval quorum is met for a withdrawal.
    pub async fn check_approval_quorum(&self, withdrawal_id: Uuid) -> anyhow::Result<Option<bool>> {
        let row: Option<(i32, serde_json::Value)> = sqlx::query_as(
            "SELECT required_approvals, decisions FROM approval_requests
             WHERE withdrawal_id = $1 AND status = 'pending'
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(withdrawal_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((required, decisions)) => {
                let approved_count = decisions.as_array()
                    .map(|arr| arr.iter().filter(|d| d["approved"] == true).count())
                    .unwrap_or(0);
                Ok(Some(approved_count >= required as usize))
            }
            None => Ok(None), // no pending approval request
        }
    }
}
