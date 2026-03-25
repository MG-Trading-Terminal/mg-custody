//! Policy evaluation engine.
//!
//! Evaluates rules top-down by priority. First matching rule wins.
//! If no rule matches, the default action is DENY (secure by default).

use custody_common::{Asset, Chain};
use rust_decimal::Decimal;

use crate::rbac::Role;
use crate::rule::{DestinationType, PolicyAction, PolicyRule};

/// The policy evaluation engine.
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

/// Result of policy evaluation.
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    /// The action to take.
    pub action: PolicyAction,
    /// Which rule matched (None if default deny).
    pub matched_rule: Option<String>,
}

impl PolicyEngine {
    /// Create a new policy engine with the given rules.
    ///
    /// Rules are sorted by priority (lowest number first).
    pub fn new(mut rules: Vec<PolicyRule>) -> Self {
        rules.sort_by_key(|r| r.priority);
        Self { rules }
    }

    /// Evaluate a withdrawal against the policy rules.
    ///
    /// Returns the action from the first matching, enabled rule.
    /// If no rule matches, returns Deny (secure by default).
    pub fn evaluate(
        &self,
        chain: Chain,
        asset: &Asset,
        amount_usd: Decimal,
        destination_type: &DestinationType,
        initiator_role: &Role,
    ) -> PolicyDecision {
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if rule.condition.matches(chain, asset, amount_usd, destination_type, initiator_role) {
                return PolicyDecision {
                    action: rule.action.clone(),
                    matched_rule: Some(rule.name.clone()),
                };
            }
        }

        // Default: deny (secure by default)
        PolicyDecision {
            action: PolicyAction::Deny {
                reason: "No matching policy rule — default deny".to_string(),
            },
            matched_rule: None,
        }
    }

    /// Number of rules in the engine.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Add a rule to the engine (re-sorts by priority).
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.priority);
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&mut self, rule_id: uuid::Uuid) -> bool {
        let len_before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() < len_before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::PolicyCondition;
    use rust_decimal_macros::dec;

    fn make_engine() -> PolicyEngine {
        let rules = vec![
            // Rule 1: Small whitelisted → auto-approve
            PolicyRule::new(
                "small_whitelisted",
                10,
                PolicyCondition {
                    max_amount_usd: Some(dec!(1000)),
                    destination_type: Some(DestinationType::Whitelisted),
                    ..Default::default()
                },
                PolicyAction::Allow,
            ),
            // Rule 2: Medium → require 1 approval
            PolicyRule::new(
                "medium_approval",
                20,
                PolicyCondition {
                    min_amount_usd: Some(dec!(1000)),
                    max_amount_usd: Some(dec!(100000)),
                    ..Default::default()
                },
                PolicyAction::RequireApproval {
                    quorum: 1,
                    approver_role: Role::Signer,
                },
            ),
            // Rule 3: Large → require 2 approvals
            PolicyRule::new(
                "large_approval",
                30,
                PolicyCondition {
                    min_amount_usd: Some(dec!(100000)),
                    ..Default::default()
                },
                PolicyAction::RequireApproval {
                    quorum: 2,
                    approver_role: Role::Admin,
                },
            ),
        ];

        PolicyEngine::new(rules)
    }

    #[test]
    fn small_whitelisted_auto_approved() {
        let engine = make_engine();
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(500),
            &DestinationType::Whitelisted,
            &Role::Editor,
        );
        assert!(matches!(decision.action, PolicyAction::Allow));
        assert_eq!(decision.matched_rule.unwrap(), "small_whitelisted");
    }

    #[test]
    fn small_non_whitelisted_denied() {
        let engine = make_engine();
        // $500 but NOT whitelisted — doesn't match rule 1 (requires whitelisted)
        // $500 < $1000 — doesn't match rule 2 (min $1000)
        // → default deny
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(500),
            &DestinationType::Any,
            &Role::Editor,
        );
        assert!(matches!(decision.action, PolicyAction::Deny { .. }));
    }

    #[test]
    fn medium_requires_approval() {
        let engine = make_engine();
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(5000),
            &DestinationType::Whitelisted,
            &Role::Editor,
        );
        // $5000 > $1000 → doesn't match rule 1
        // $5000 in [$1000, $100000) → matches rule 2
        assert!(matches!(
            decision.action,
            PolicyAction::RequireApproval { quorum: 1, .. }
        ));
    }

    #[test]
    fn large_requires_two_approvals() {
        let engine = make_engine();
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(200000),
            &DestinationType::Whitelisted,
            &Role::Editor,
        );
        assert!(matches!(
            decision.action,
            PolicyAction::RequireApproval { quorum: 2, .. }
        ));
        assert_eq!(decision.matched_rule.unwrap(), "large_approval");
    }

    #[test]
    fn first_match_wins() {
        // Rule 1 (priority 10) should win over rule 2 (priority 20)
        // for $500 whitelisted
        let engine = make_engine();
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(500),
            &DestinationType::Whitelisted,
            &Role::Editor,
        );
        assert!(matches!(decision.action, PolicyAction::Allow));
    }

    #[test]
    fn disabled_rules_skipped() {
        let mut rules = vec![PolicyRule::new(
            "disabled_allow",
            1,
            PolicyCondition::default(),
            PolicyAction::Allow,
        )];
        rules[0].enabled = false;
        let engine = PolicyEngine::new(rules);

        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(100),
            &DestinationType::Any,
            &Role::Editor,
        );
        // Disabled rule should be skipped → default deny
        assert!(matches!(decision.action, PolicyAction::Deny { .. }));
    }

    #[test]
    fn empty_rules_default_deny() {
        let engine = PolicyEngine::new(vec![]);
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(100),
            &DestinationType::Any,
            &Role::Editor,
        );
        assert!(matches!(decision.action, PolicyAction::Deny { .. }));
        assert!(decision.matched_rule.is_none());
    }

    #[test]
    fn add_and_remove_rules() {
        let mut engine = PolicyEngine::new(vec![]);
        assert_eq!(engine.rule_count(), 0);

        let rule = PolicyRule::new("test", 1, PolicyCondition::default(), PolicyAction::Allow);
        let rule_id = rule.id;
        engine.add_rule(rule);
        assert_eq!(engine.rule_count(), 1);

        assert!(engine.remove_rule(rule_id));
        assert_eq!(engine.rule_count(), 0);
    }

    #[test]
    fn rules_sorted_by_priority() {
        let rules = vec![
            PolicyRule::new("low_priority", 100, PolicyCondition::default(), PolicyAction::Deny { reason: "low".into() }),
            PolicyRule::new("high_priority", 1, PolicyCondition::default(), PolicyAction::Allow),
        ];
        let engine = PolicyEngine::new(rules);

        // High priority (1) should match first → Allow
        let decision = engine.evaluate(
            Chain::Ethereum,
            &Asset::new("ETH"),
            dec!(100),
            &DestinationType::Any,
            &Role::Editor,
        );
        assert!(matches!(decision.action, PolicyAction::Allow));
        assert_eq!(decision.matched_rule.unwrap(), "high_priority");
    }
}
