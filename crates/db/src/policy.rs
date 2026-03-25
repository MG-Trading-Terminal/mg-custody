//! Policy persistence layer.
//!
//! Stores policy rules, whitelists, approval requests, and user roles.

use uuid::Uuid;

use crate::CustodyDb;

impl CustodyDb {
    /// Get all enabled policy rules, ordered by priority.
    pub async fn get_policy_rules(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        // TODO: SELECT * FROM policy_rules WHERE enabled = true ORDER BY priority
        Ok(vec![])
    }

    /// Insert a policy rule.
    pub async fn insert_policy_rule(&self, rule_json: &serde_json::Value) -> anyhow::Result<()> {
        let _ = rule_json;
        // TODO: INSERT INTO policy_rules
        Ok(())
    }

    /// Delete a policy rule.
    pub async fn delete_policy_rule(&self, rule_id: Uuid) -> anyhow::Result<bool> {
        let _ = rule_id;
        // TODO: DELETE FROM policy_rules WHERE id = $1
        Ok(false)
    }

    /// Get whitelist entries for a chain.
    pub async fn get_whitelist(&self, chain: &str) -> anyhow::Result<Vec<serde_json::Value>> {
        let _ = chain;
        // TODO: SELECT * FROM address_whitelist WHERE chain = $1
        Ok(vec![])
    }

    /// Insert a whitelist entry.
    pub async fn insert_whitelist_entry(&self, entry_json: &serde_json::Value) -> anyhow::Result<()> {
        let _ = entry_json;
        // TODO: INSERT INTO address_whitelist
        Ok(())
    }

    /// Get pending approval requests.
    pub async fn get_pending_approvals(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        // TODO: SELECT * FROM approval_requests WHERE status = 'pending'
        Ok(vec![])
    }

    /// Insert an approval request.
    pub async fn insert_approval_request(&self, request_json: &serde_json::Value) -> anyhow::Result<()> {
        let _ = request_json;
        // TODO: INSERT INTO approval_requests
        Ok(())
    }

    /// Update an approval request status.
    pub async fn update_approval_status(&self, request_id: Uuid, status: &str) -> anyhow::Result<()> {
        let _ = (request_id, status);
        // TODO: UPDATE approval_requests SET status = $2 WHERE id = $1
        Ok(())
    }
}
