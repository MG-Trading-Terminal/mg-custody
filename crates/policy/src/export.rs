//! Policy export/import (JSON/TOML).
//!
//! Allows exporting the full policy configuration for backup,
//! review, or migration between environments.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::rule::PolicyRule;
use crate::whitelist::WhitelistEntry;
use crate::api_keys::DelayConfig;

/// Complete policy export bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyExport {
    pub version: String,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    pub rules: Vec<PolicyRule>,
    pub whitelist: Vec<WhitelistEntry>,
    pub delay_config: DelayConfig,
}

impl PolicyExport {
    /// Export to JSON string.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Import from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    /// Export to TOML string.
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Import from TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        Ok(toml::from_str(toml_str)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{PolicyCondition, PolicyAction};

    fn sample_export() -> PolicyExport {
        PolicyExport {
            version: "1.0".into(),
            exported_at: chrono::Utc::now(),
            rules: vec![PolicyRule::new("test", 1, PolicyCondition::default(), PolicyAction::Allow)],
            whitelist: vec![],
            delay_config: DelayConfig::default(),
        }
    }

    #[test]
    fn json_roundtrip() {
        let export = sample_export();
        let json = export.to_json().unwrap();
        let imported = PolicyExport::from_json(&json).unwrap();
        assert_eq!(imported.rules.len(), 1);
        assert_eq!(imported.rules[0].name, "test");
    }

    #[test]
    fn toml_roundtrip() {
        let export = sample_export();
        let toml = export.to_toml().unwrap();
        let imported = PolicyExport::from_toml(&toml).unwrap();
        assert_eq!(imported.rules.len(), 1);
    }
}
