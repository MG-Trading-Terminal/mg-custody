//! Policy rule types.
//!
//! A rule consists of conditions (what transactions it matches)
//! and an action (what happens when it matches).

use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rbac::Role;

/// Destination type for policy matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationType {
    /// Address is in the whitelist.
    Whitelisted,
    /// Address belongs to our own vaults/wallets.
    Internal,
    /// Any address (including non-whitelisted).
    Any,
}

/// Conditions that a transaction must match for the rule to apply.
///
/// All `Some` fields must match (AND logic). `None` means "any value".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyCondition {
    /// Match specific chain(s).
    pub chain: Option<Chain>,
    /// Match specific asset(s).
    pub asset: Option<Asset>,
    /// Minimum USD amount (inclusive).
    pub min_amount_usd: Option<Decimal>,
    /// Maximum USD amount (exclusive).
    pub max_amount_usd: Option<Decimal>,
    /// Destination must be of this type.
    pub destination_type: Option<DestinationType>,
    /// Initiator must have this role (or higher).
    pub initiator_role: Option<Role>,
}

/// Action to take when a rule matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyAction {
    /// Allow the transaction immediately.
    Allow,
    /// Block the transaction.
    Deny { reason: String },
    /// Require M-of-N approval from users with the specified role.
    RequireApproval {
        quorum: u16,
        approver_role: Role,
    },
    /// Delay the transaction by N minutes before processing.
    Delay { minutes: u32 },
}

/// A policy rule: condition → action.
///
/// Rules are evaluated top-down by priority (lowest number = highest priority).
/// First matching rule wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: Uuid,
    pub name: String,
    /// Lower priority number = evaluated first.
    pub priority: u32,
    pub condition: PolicyCondition,
    pub action: PolicyAction,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl PolicyRule {
    /// Create a new rule with the given name, priority, condition, and action.
    pub fn new(name: &str, priority: u32, condition: PolicyCondition, action: PolicyAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            priority,
            condition,
            action,
            enabled: true,
            created_at: chrono::Utc::now(),
        }
    }
}

/// Check if a condition matches a withdrawal context.
impl PolicyCondition {
    pub fn matches(
        &self,
        chain: Chain,
        asset: &Asset,
        amount_usd: Decimal,
        destination_type: &DestinationType,
        initiator_role: &Role,
    ) -> bool {
        if let Some(ref c) = self.chain {
            if *c != chain {
                return false;
            }
        }
        if let Some(ref a) = self.asset {
            if a.as_str() != asset.as_str() {
                return false;
            }
        }
        if let Some(min) = self.min_amount_usd {
            if amount_usd < min {
                return false;
            }
        }
        if let Some(max) = self.max_amount_usd {
            if amount_usd >= max {
                return false;
            }
        }
        if let Some(ref dt) = self.destination_type {
            if *dt != DestinationType::Any && dt != destination_type {
                return false;
            }
        }
        if let Some(ref role) = self.initiator_role {
            if !initiator_role.has_at_least(role) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn condition_matches_all_none() {
        let cond = PolicyCondition::default();
        assert!(cond.matches(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(100),
            &DestinationType::Whitelisted,
            &Role::Editor,
        ));
    }

    #[test]
    fn condition_matches_chain() {
        let cond = PolicyCondition {
            chain: Some(Chain::Bitcoin),
            ..Default::default()
        };
        assert!(cond.matches(Chain::Bitcoin, &Asset::new("BTC"), dec!(1), &DestinationType::Any, &Role::Editor));
        assert!(!cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(1), &DestinationType::Any, &Role::Editor));
    }

    #[test]
    fn condition_matches_amount_range() {
        let cond = PolicyCondition {
            min_amount_usd: Some(dec!(1000)),
            max_amount_usd: Some(dec!(10000)),
            ..Default::default()
        };
        assert!(!cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(999), &DestinationType::Any, &Role::Editor));
        assert!(cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(1000), &DestinationType::Any, &Role::Editor));
        assert!(cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(5000), &DestinationType::Any, &Role::Editor));
        assert!(!cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(10000), &DestinationType::Any, &Role::Editor));
    }

    #[test]
    fn condition_matches_destination_type() {
        let cond = PolicyCondition {
            destination_type: Some(DestinationType::Whitelisted),
            ..Default::default()
        };
        assert!(cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(1), &DestinationType::Whitelisted, &Role::Editor));
        assert!(!cond.matches(Chain::Ethereum, &Asset::new("ETH"), dec!(1), &DestinationType::Internal, &Role::Editor));
    }

    #[test]
    fn policy_action_serialization() {
        let action = PolicyAction::RequireApproval {
            quorum: 2,
            approver_role: Role::Admin,
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("require_approval"));
        assert!(json.contains("\"quorum\":2"));
    }
}
