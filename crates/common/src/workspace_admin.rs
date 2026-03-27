//! Workspace administration — settings, console users, audit, freeze.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workspace settings (admin-configurable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    pub workspace_id: Uuid,
    pub name: String,
    pub is_frozen: bool,
    pub auto_fuel_enabled: bool,
    pub default_approval_threshold: u32,
    pub ip_allowlist: Vec<String>,
    pub allowed_chains: Vec<String>,
    pub max_single_tx_usd: rust_decimal::Decimal,
    pub webhook_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Console user (admin panel access).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleUser {
    pub id: Uuid,
    pub email: String,
    pub role: ConsoleRole,
    pub status: ConsoleUserStatus,
    pub mfa_enabled: bool,
    pub fido2_registered: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleRole {
    Owner,
    Admin,
    Signer,
    Approver,
    Editor,
    Viewer,
    Auditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleUserStatus { Active, Suspended, Invited, Deactivated }

/// Admin quorum — require multiple admins for sensitive operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminQuorum {
    pub operation: AdminOperation,
    pub required_approvers: u32,
    pub current_approvers: u32,
    pub approvers: Vec<String>,
    pub status: QuorumStatus,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminOperation {
    FreezeWorkspace,
    UnfreezeWorkspace,
    AddAdmin,
    RemoveAdmin,
    ChangePolicy,
    RotateKeys,
    DeleteVault,
    UpdateCircuitBreaker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuorumStatus { Pending, Approved, Rejected, Expired }

/// Workspace audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceAuditEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor_email: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Freeze workspace — halt all operations.
pub fn freeze_workspace_event(workspace_id: Uuid, actor: &str, reason: &str) -> WorkspaceAuditEntry {
    WorkspaceAuditEntry {
        id: Uuid::new_v4(), workspace_id,
        actor_email: actor.into(),
        action: "workspace.frozen".into(),
        resource_type: "workspace".into(),
        resource_id: Some(workspace_id.to_string()),
        ip_address: None, user_agent: None,
        details: serde_json::json!({"reason": reason}),
        timestamp: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_role_serialization() {
        assert_eq!(serde_json::to_string(&ConsoleRole::Auditor).unwrap(), r#""auditor""#);
        assert_eq!(serde_json::to_string(&ConsoleRole::Approver).unwrap(), r#""approver""#);
    }

    #[test]
    fn admin_operation_serialization() {
        assert_eq!(serde_json::to_string(&AdminOperation::FreezeWorkspace).unwrap(), r#""freeze_workspace""#);
        assert_eq!(serde_json::to_string(&AdminOperation::RotateKeys).unwrap(), r#""rotate_keys""#);
    }

    #[test]
    fn freeze_workspace_audit() {
        let entry = freeze_workspace_event(Uuid::nil(), "admin@co.com", "security incident");
        assert_eq!(entry.action, "workspace.frozen");
        assert!(entry.details["reason"].as_str().unwrap().contains("security"));
    }

    #[test]
    fn workspace_settings() {
        let settings = WorkspaceSettings {
            workspace_id: Uuid::nil(), name: "Production".into(),
            is_frozen: false, auto_fuel_enabled: true,
            default_approval_threshold: 2,
            ip_allowlist: vec!["10.0.0.0/8".into()],
            allowed_chains: vec!["ethereum".into(), "bitcoin".into()],
            max_single_tx_usd: rust_decimal::Decimal::new(500_000, 0),
            webhook_url: Some("https://exchange.com/webhook".into()),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("Production"));
        assert!(json.contains("10.0.0.0"));
    }

    #[test]
    fn console_user_structure() {
        let user = ConsoleUser {
            id: Uuid::nil(), email: "admin@co.com".into(),
            role: ConsoleRole::Admin, status: ConsoleUserStatus::Active,
            mfa_enabled: true, fido2_registered: true,
            last_login_at: Some(Utc::now()), created_at: Utc::now(),
        };
        assert!(user.mfa_enabled);
        assert!(user.fido2_registered);
    }

    #[test]
    fn admin_quorum() {
        let quorum = AdminQuorum {
            operation: AdminOperation::ChangePolicy,
            required_approvers: 2, current_approvers: 1,
            approvers: vec!["admin1@co.com".into()],
            status: QuorumStatus::Pending,
            expires_at: Utc::now(),
        };
        assert!(quorum.current_approvers < quorum.required_approvers);
    }
}
