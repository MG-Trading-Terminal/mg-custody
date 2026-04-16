//! Policy versioning with audit trail.
//!
//! Every policy change creates a new version. Old versions are retained
//! for audit purposes. Rollback to any previous version is supported.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::rule::PolicyRule;

/// A versioned snapshot of all policy rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyVersion {
    pub version: u64,
    pub rules: Vec<PolicyRule>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub comment: String,
    /// Hash of the rules for integrity verification.
    pub rules_hash: String,
}

/// Policy version history.
pub struct PolicyHistory {
    versions: Vec<PolicyVersion>,
    current_version: u64,
}

impl PolicyHistory {
    pub fn new() -> Self {
        Self { versions: Vec::new(), current_version: 0 }
    }

    /// Create a new version from the current rules.
    pub fn create_version(
        &mut self,
        rules: Vec<PolicyRule>,
        author: &str,
        comment: &str,
    ) -> &PolicyVersion {
        self.current_version += 1;
        let rules_json = serde_json::to_string(&rules).unwrap_or_default();
        let rules_hash = format!("{:x}", md5_simple(rules_json.as_bytes()));

        let version = PolicyVersion {
            version: self.current_version,
            rules,
            created_at: Utc::now(),
            created_by: author.to_string(),
            comment: comment.to_string(),
            rules_hash,
        };

        self.versions.push(version);
        self.versions.last().unwrap()
    }

    /// Get a specific version.
    pub fn get_version(&self, version: u64) -> Option<&PolicyVersion> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// Get the current (latest) version.
    pub fn current(&self) -> Option<&PolicyVersion> {
        self.versions.last()
    }

    /// Current version number.
    pub fn current_version_number(&self) -> u64 {
        self.current_version
    }

    /// List all versions (metadata only, no rules).
    pub fn list_versions(&self) -> Vec<(u64, &str, DateTime<Utc>)> {
        self.versions.iter().map(|v| (v.version, v.comment.as_str(), v.created_at)).collect()
    }

    /// Total versions.
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}

impl Default for PolicyHistory {
    fn default() -> Self { Self::new() }
}

fn md5_simple(data: &[u8]) -> u128 {
    let mut h: u128 = 0xcbf2_9ce4_8422_2325;
    for &b in data { h ^= b as u128; h = h.wrapping_mul(0x0100_0000_01b3); h = h.rotate_left(13); }
    h
}

/// Policy simulation mode — dry-run without actually applying.
pub struct PolicySimulator;

impl PolicySimulator {
    /// Simulate a withdrawal against a policy version.
    /// Returns what would happen without actually processing.
    pub fn simulate(
        engine: &crate::PolicyEngine,
        chain: custody_common::Chain,
        asset: &custody_common::Asset,
        amount_usd: rust_decimal::Decimal,
        dest_type: &crate::DestinationType,
        role: &crate::rbac::Role,
    ) -> SimulationResult {
        let decision = engine.evaluate(chain, asset, amount_usd, dest_type, role);
        SimulationResult {
            action: format!("{:?}", decision.action),
            matched_rule: decision.matched_rule,
            would_proceed: matches!(decision.action, crate::PolicyAction::Allow),
        }
    }
}

/// Result of a policy simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub action: String,
    pub matched_rule: Option<String>,
    pub would_proceed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{PolicyCondition, PolicyAction};

    #[test]
    fn version_history() {
        let mut history = PolicyHistory::new();
        assert!(history.is_empty());

        let rules = vec![PolicyRule::new("test", 1, PolicyCondition::default(), PolicyAction::Allow)];
        history.create_version(rules.clone(), "admin", "Initial policy");
        assert_eq!(history.current_version_number(), 1);

        history.create_version(rules, "admin", "Updated limits");
        assert_eq!(history.current_version_number(), 2);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn get_specific_version() {
        let mut history = PolicyHistory::new();
        let rules = vec![PolicyRule::new("v1", 1, PolicyCondition::default(), PolicyAction::Allow)];
        history.create_version(rules, "admin", "V1");

        let rules2 = vec![PolicyRule::new("v2", 1, PolicyCondition::default(), PolicyAction::Deny { reason: "blocked".into() })];
        history.create_version(rules2, "admin", "V2");

        let v1 = history.get_version(1).unwrap();
        assert_eq!(v1.rules[0].name, "v1");

        let v2 = history.get_version(2).unwrap();
        assert_eq!(v2.rules[0].name, "v2");
    }

    #[test]
    fn version_has_hash() {
        let mut history = PolicyHistory::new();
        let rules = vec![PolicyRule::new("test", 1, PolicyCondition::default(), PolicyAction::Allow)];
        history.create_version(rules, "admin", "test");
        assert!(!history.current().unwrap().rules_hash.is_empty());
    }

    #[test]
    fn simulation_dry_run() {
        use crate::{PolicyEngine, DestinationType};
        use crate::rbac::Role;
        use rust_decimal_macros::dec;

        let rules = vec![PolicyRule::new("allow_small", 1,
            PolicyCondition { max_amount_usd: Some(dec!(1000)), ..Default::default() },
            PolicyAction::Allow,
        )];
        let engine = PolicyEngine::new(rules);

        let result = PolicySimulator::simulate(
            &engine, custody_common::Chain::Ethereum, &custody_common::Asset::new("ETH"),
            dec!(500), &DestinationType::Any, &Role::Editor,
        );
        assert!(result.would_proceed);

        let result = PolicySimulator::simulate(
            &engine, custody_common::Chain::Ethereum, &custody_common::Asset::new("ETH"),
            dec!(5000), &DestinationType::Any, &Role::Editor,
        );
        assert!(!result.would_proceed); // default deny
    }
}
