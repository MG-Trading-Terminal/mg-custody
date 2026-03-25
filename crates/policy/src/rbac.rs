//! Role-based access control.
//!
//! Defines user roles and permissions for the custody service.
//! Modeled after Fireblocks workspace roles.

use serde::{Deserialize, Serialize};

/// User role in the custody system.
///
/// Roles form a hierarchy: Owner > Admin > Signer > Editor > Viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read-only access to all data.
    Viewer = 0,
    /// Can initiate transactions (but not approve or sign).
    Editor = 1,
    /// Can approve and sign transactions.
    Signer = 2,
    /// Can manage users, policies, whitelists. Can do everything Signer can.
    Admin = 3,
    /// Full control. Cannot be removed. Only one per workspace.
    Owner = 4,
}

/// Permission types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ViewTransactions,
    InitiateWithdrawal,
    ApproveWithdrawal,
    SignTransaction,
    ManageWhitelist,
    ManagePolicies,
    ManageUsers,
    ManageConfig,
}

impl Role {
    /// Check if this role has at least the level of `required`.
    pub fn has_at_least(&self, required: &Role) -> bool {
        (*self as u8) >= (*required as u8)
    }

    /// Check if this role has a specific permission.
    pub fn has_permission(&self, permission: Permission) -> bool {
        match permission {
            Permission::ViewTransactions => true, // All roles
            Permission::InitiateWithdrawal => self.has_at_least(&Role::Editor),
            Permission::ApproveWithdrawal => self.has_at_least(&Role::Signer),
            Permission::SignTransaction => self.has_at_least(&Role::Signer),
            Permission::ManageWhitelist => self.has_at_least(&Role::Admin),
            Permission::ManagePolicies => self.has_at_least(&Role::Admin),
            Permission::ManageUsers => self.has_at_least(&Role::Admin),
            Permission::ManageConfig => self.has_at_least(&Role::Owner),
        }
    }
}

/// Check if an admin quorum is met.
///
/// For sensitive operations (policy changes, whitelist additions),
/// multiple admins must approve.
pub fn admin_quorum_met(approvals: &[Role], required: u16) -> bool {
    let admin_count = approvals
        .iter()
        .filter(|r| r.has_at_least(&Role::Admin))
        .count();
    admin_count >= required as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_hierarchy() {
        assert!(Role::Owner.has_at_least(&Role::Admin));
        assert!(Role::Admin.has_at_least(&Role::Signer));
        assert!(Role::Signer.has_at_least(&Role::Editor));
        assert!(Role::Editor.has_at_least(&Role::Viewer));
        assert!(!Role::Viewer.has_at_least(&Role::Editor));
        assert!(!Role::Editor.has_at_least(&Role::Admin));
    }

    #[test]
    fn role_permissions() {
        // Viewer
        assert!(Role::Viewer.has_permission(Permission::ViewTransactions));
        assert!(!Role::Viewer.has_permission(Permission::InitiateWithdrawal));

        // Editor
        assert!(Role::Editor.has_permission(Permission::InitiateWithdrawal));
        assert!(!Role::Editor.has_permission(Permission::ApproveWithdrawal));

        // Signer
        assert!(Role::Signer.has_permission(Permission::ApproveWithdrawal));
        assert!(Role::Signer.has_permission(Permission::SignTransaction));
        assert!(!Role::Signer.has_permission(Permission::ManagePolicies));

        // Admin
        assert!(Role::Admin.has_permission(Permission::ManagePolicies));
        assert!(Role::Admin.has_permission(Permission::ManageWhitelist));
        assert!(Role::Admin.has_permission(Permission::ManageUsers));
        assert!(!Role::Admin.has_permission(Permission::ManageConfig));

        // Owner
        assert!(Role::Owner.has_permission(Permission::ManageConfig));
    }

    #[test]
    fn admin_quorum() {
        let approvals = vec![Role::Admin, Role::Admin, Role::Signer];
        assert!(admin_quorum_met(&approvals, 2));
        assert!(!admin_quorum_met(&approvals, 3)); // only 2 admins

        // Owner counts as admin
        let approvals = vec![Role::Owner, Role::Admin];
        assert!(admin_quorum_met(&approvals, 2));
    }

    #[test]
    fn role_serialization() {
        assert_eq!(serde_json::to_string(&Role::Admin).unwrap(), "\"admin\"");
        assert_eq!(serde_json::to_string(&Role::Owner).unwrap(), "\"owner\"");
        let r: Role = serde_json::from_str("\"signer\"").unwrap();
        assert_eq!(r, Role::Signer);
    }
}
