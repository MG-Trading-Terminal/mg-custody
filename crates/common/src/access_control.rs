//! Fine-grained access control — resource-level permissions, ABAC policies.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Resource types that can be access-controlled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Vault, VaultAccount, Transaction, Address, Webhook,
    Policy, ApiKey, User, Workspace, Contract, PaymentFlow,
    StakingPosition, AuditLog, SystemConfig,
}

/// Actions on resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Create, Read, Update, Delete, List, Sign, Approve, Reject, Export, Admin,
}

/// Access control policy (ABAC — Attribute-Based Access Control).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPolicy {
    pub id: Uuid,
    pub name: String,
    pub rules: Vec<AccessRule>,
    pub default_action: PolicyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRule {
    pub resource: Resource,
    pub action: Action,
    pub conditions: Vec<Condition>,
    pub effect: PolicyAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    RoleIs { role: String },
    AmountBelow { usd: rust_decimal::Decimal },
    AmountAbove { usd: rust_decimal::Decimal },
    ChainIs { chain: String },
    IpInRange { cidr: String },
    TimeWindow { start_hour: u8, end_hour: u8 },
    VaultIs { vault_id: Uuid },
    MfaVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction { Allow, Deny, RequireMfa, RequireApproval }

/// Evaluate an access request against a policy.
pub fn evaluate_access(policy: &AccessPolicy, resource: Resource, action: Action, context: &AccessContext) -> PolicyAction {
    for rule in &policy.rules {
        if rule.resource == resource && rule.action == action
            && evaluate_conditions(&rule.conditions, context) {
                return rule.effect;
            }
    }
    policy.default_action
}

#[derive(Debug, Clone)]
pub struct AccessContext {
    pub role: String,
    pub amount_usd: Option<rust_decimal::Decimal>,
    pub chain: Option<String>,
    pub ip: Option<String>,
    pub mfa_verified: bool,
    pub vault_id: Option<Uuid>,
}

fn evaluate_conditions(conditions: &[Condition], ctx: &AccessContext) -> bool {
    conditions.iter().all(|c| match c {
        Condition::RoleIs { role } => ctx.role == *role,
        Condition::MfaVerified => ctx.mfa_verified,
        Condition::AmountBelow { usd } => ctx.amount_usd.map(|a| a < *usd).unwrap_or(true),
        Condition::AmountAbove { usd } => ctx.amount_usd.map(|a| a >= *usd).unwrap_or(false),
        Condition::ChainIs { chain } => ctx.chain.as_deref() == Some(chain.as_str()),
        Condition::VaultIs { vault_id } => ctx.vault_id == Some(*vault_id),
        _ => true, // IP/time not implemented yet
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn test_policy() -> AccessPolicy {
        AccessPolicy {
            id: Uuid::nil(), name: "Default".into(),
            rules: vec![
                AccessRule { resource: Resource::Transaction, action: Action::Sign, conditions: vec![Condition::RoleIs { role: "signer".into() }], effect: PolicyAction::Allow },
                AccessRule { resource: Resource::Transaction, action: Action::Sign, conditions: vec![Condition::AmountAbove { usd: dec!(100000) }], effect: PolicyAction::RequireApproval },
                AccessRule { resource: Resource::SystemConfig, action: Action::Update, conditions: vec![Condition::RoleIs { role: "admin".into() }, Condition::MfaVerified], effect: PolicyAction::Allow },
            ],
            default_action: PolicyAction::Deny,
        }
    }

    #[test]
    fn signer_can_sign() {
        let ctx = AccessContext { role: "signer".into(), amount_usd: Some(dec!(1000)), chain: None, ip: None, mfa_verified: false, vault_id: None };
        assert_eq!(evaluate_access(&test_policy(), Resource::Transaction, Action::Sign, &ctx), PolicyAction::Allow);
    }

    #[test]
    fn viewer_cannot_sign() {
        let ctx = AccessContext { role: "viewer".into(), amount_usd: Some(dec!(1000)), chain: None, ip: None, mfa_verified: false, vault_id: None };
        assert_eq!(evaluate_access(&test_policy(), Resource::Transaction, Action::Sign, &ctx), PolicyAction::Deny);
    }

    #[test]
    fn large_amount_requires_approval() {
        let ctx = AccessContext { role: "trader".into(), amount_usd: Some(dec!(200000)), chain: None, ip: None, mfa_verified: false, vault_id: None };
        assert_eq!(evaluate_access(&test_policy(), Resource::Transaction, Action::Sign, &ctx), PolicyAction::RequireApproval);
    }

    #[test]
    fn admin_config_needs_mfa() {
        let ctx = AccessContext { role: "admin".into(), amount_usd: None, chain: None, ip: None, mfa_verified: true, vault_id: None };
        assert_eq!(evaluate_access(&test_policy(), Resource::SystemConfig, Action::Update, &ctx), PolicyAction::Allow);
    }

    #[test]
    fn admin_config_without_mfa_denied() {
        let ctx = AccessContext { role: "admin".into(), amount_usd: None, chain: None, ip: None, mfa_verified: false, vault_id: None };
        assert_eq!(evaluate_access(&test_policy(), Resource::SystemConfig, Action::Update, &ctx), PolicyAction::Deny);
    }

    #[test]
    fn resource_serialization() {
        assert_eq!(serde_json::to_string(&Resource::StakingPosition).unwrap(), r#""staking_position""#);
    }

    #[test]
    fn action_serialization() {
        assert_eq!(serde_json::to_string(&Action::Approve).unwrap(), r#""approve""#);
    }

    #[test]
    fn condition_chain_match() {
        let conditions = vec![Condition::ChainIs { chain: "ethereum".into() }];
        let ctx = AccessContext { role: "".into(), amount_usd: None, chain: Some("ethereum".into()), ip: None, mfa_verified: false, vault_id: None };
        assert!(evaluate_conditions(&conditions, &ctx));
    }
}
