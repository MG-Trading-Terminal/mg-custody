//! Security incident response plan types.
//!
//! Structured incident handling: detect → classify → respond → resolve → post-mortem.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Incident severity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Low,      // Minor issue, no fund risk
    Medium,   // Degraded service, potential risk
    High,     // Active fund risk, immediate response needed
    Critical, // Confirmed fund loss or compromise
}

/// Incident status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Detected,
    Investigating,
    Mitigating,
    Resolved,
    PostMortem,
}

/// A security incident record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: Uuid,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub title: String,
    pub description: String,
    pub detected_by: String,
    pub assigned_to: Option<String>,
    pub actions_taken: Vec<String>,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub post_mortem_url: Option<String>,
}

/// Automated incident response actions.
pub fn auto_response_actions(severity: IncidentSeverity) -> Vec<&'static str> {
    match severity {
        IncidentSeverity::Low => vec![
            "Log incident",
            "Notify on-call via Slack",
        ],
        IncidentSeverity::Medium => vec![
            "Log incident",
            "Notify on-call via Slack + Telegram",
            "Increase monitoring frequency",
        ],
        IncidentSeverity::High => vec![
            "Log incident",
            "Page on-call via Telegram (emergency)",
            "Pause affected chain withdrawals",
            "Increase monitoring to real-time",
            "Notify management",
        ],
        IncidentSeverity::Critical => vec![
            "EMERGENCY FREEZE all operations",
            "Page all security team members",
            "Disable all API keys",
            "Snapshot all database state",
            "Notify legal + management",
            "Begin key rotation procedure",
        ],
    }
}

/// Bug bounty program types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugBountyProgram {
    pub active: bool,
    pub scope: Vec<String>,
    pub rewards: BountyRewards,
    pub safe_harbor_policy: String,
    pub contact_email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyRewards {
    pub critical_usd: u64,
    pub high_usd: u64,
    pub medium_usd: u64,
    pub low_usd: u64,
}

impl Default for BountyRewards {
    fn default() -> Self {
        Self { critical_usd: 50000, high_usd: 10000, medium_usd: 2500, low_usd: 500 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_response_critical() {
        let actions = auto_response_actions(IncidentSeverity::Critical);
        assert!(actions.iter().any(|a| a.contains("FREEZE")));
        assert!(actions.len() >= 5);
    }

    #[test]
    fn severity_ordering() {
        assert!(IncidentSeverity::Low < IncidentSeverity::Critical);
    }

    #[test]
    fn bounty_rewards_default() {
        let rewards = BountyRewards::default();
        assert_eq!(rewards.critical_usd, 50000);
    }
}
