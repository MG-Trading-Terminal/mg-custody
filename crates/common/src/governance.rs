//! Governance types — vault policies, approval workflows, network metadata,
//! transaction metadata, staking execution.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Vault policies (spending limits, velocity rules)
// ---------------------------------------------------------------------------

/// Policy attached to a vault controlling withdrawal behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPolicy {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub name: String,
    pub rules: Vec<PolicyRule>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single rule within a vault policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyRule {
    /// Maximum single withdrawal amount.
    MaxAmount { amount: Decimal, asset: Option<String> },
    /// Maximum total outflow in a time window.
    VelocityLimit { amount: Decimal, window_minutes: u32, asset: Option<String> },
    /// Require N approvals for any withdrawal.
    RequireApprovals { min_approvers: u32 },
    /// Only allow withdrawals to whitelisted addresses.
    WhitelistOnly,
    /// Time lock — withdrawals delayed by N minutes.
    TimeLock { delay_minutes: u32 },
    /// Block withdrawals entirely (emergency freeze).
    Frozen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultPolicyRequest {
    pub vault_id: Uuid,
    pub name: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateVaultPolicyRequest {
    pub name: Option<String>,
    pub rules: Option<Vec<PolicyRule>>,
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Approval workflow (multi-sig quorum)
// ---------------------------------------------------------------------------

/// A pending approval request for a withdrawal or operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalWorkflow {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub vault_id: Uuid,
    pub required_approvals: u32,
    pub current_approvals: u32,
    pub decisions: Vec<ApprovalDecision>,
    pub status: ApprovalStatus,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approver_id: Uuid,
    pub approver_email: String,
    pub decision: Decision,
    pub reason: Option<String>,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("pending"),
            Self::Approved => f.write_str("approved"),
            Self::Rejected => f.write_str("rejected"),
            Self::Expired => f.write_str("expired"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitApprovalRequest {
    pub approver_id: Uuid,
    pub approver_email: String,
    pub decision: Decision,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Supported networks / chain metadata
// ---------------------------------------------------------------------------

/// Metadata about a supported blockchain network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain: Chain,
    pub name: String,
    pub native_asset: String,
    pub native_decimals: u8,
    pub address_format: String,
    pub required_confirmations: u64,
    pub average_block_time_secs: u32,
    pub supports_eip1559: bool,
    pub supports_rbf: bool,
    pub supports_staking: bool,
    pub explorer_url: String,
    pub testnet_available: bool,
}

/// Token metadata on a network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkToken {
    pub symbol: String,
    pub name: String,
    pub chain: Chain,
    pub contract_address: Option<String>,
    pub decimals: u8,
    pub token_type: String,
}

// ---------------------------------------------------------------------------
// Transaction notes / metadata
// ---------------------------------------------------------------------------

/// User-attached metadata on a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionNote {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub note: String,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub note: String,
    pub author: Option<String>,
}

// ---------------------------------------------------------------------------
// Staking execution (wires to existing staking types)
// ---------------------------------------------------------------------------

/// Request to execute staking via API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub validator: String,
    pub amount: Decimal,
}

/// Request to unstake.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnstakeRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub validator: String,
    pub amount: Decimal,
}

/// Staking position summary returned by API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingPositionSummary {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub chain: Chain,
    pub validator: String,
    pub amount_staked: Decimal,
    pub rewards_earned: Decimal,
    pub status: String,
    pub staked_at: DateTime<Utc>,
    pub unbonding_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn policy_rule_max_amount() {
        let rule = PolicyRule::MaxAmount { amount: dec!(10000), asset: Some("ETH".into()) };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("max_amount"));
        assert!(json.contains("10000"));
    }

    #[test]
    fn policy_rule_velocity() {
        let rule = PolicyRule::VelocityLimit {
            amount: dec!(50000), window_minutes: 60, asset: None,
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("velocity_limit"));
        assert!(json.contains("window_minutes"));
    }

    #[test]
    fn policy_rule_frozen() {
        let rule = PolicyRule::Frozen;
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("frozen"));
    }

    #[test]
    fn policy_rule_time_lock() {
        let rule = PolicyRule::TimeLock { delay_minutes: 30 };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("time_lock"));
        assert!(json.contains("30"));
    }

    #[test]
    fn policy_rule_deserialization() {
        let json = r#"{"type":"require_approvals","min_approvers":3}"#;
        let rule: PolicyRule = serde_json::from_str(json).unwrap();
        match rule {
            PolicyRule::RequireApprovals { min_approvers } => assert_eq!(min_approvers, 3),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn approval_status_display() {
        assert_eq!(ApprovalStatus::Pending.to_string(), "pending");
        assert_eq!(ApprovalStatus::Approved.to_string(), "approved");
        assert_eq!(ApprovalStatus::Rejected.to_string(), "rejected");
    }

    #[test]
    fn decision_serialization() {
        assert_eq!(serde_json::to_string(&Decision::Approve).unwrap(), r#""approve""#);
        assert_eq!(serde_json::to_string(&Decision::Reject).unwrap(), r#""reject""#);
    }

    #[test]
    fn network_info_structure() {
        let info = NetworkInfo {
            chain: Chain::Ethereum,
            name: "Ethereum Mainnet".into(),
            native_asset: "ETH".into(),
            native_decimals: 18,
            address_format: "0x-prefixed hex, 42 chars".into(),
            required_confirmations: 12,
            average_block_time_secs: 12,
            supports_eip1559: true,
            supports_rbf: false,
            supports_staking: true,
            explorer_url: "https://etherscan.io".into(),
            testnet_available: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Ethereum Mainnet"));
        assert!(json.contains("etherscan"));
    }

    #[test]
    fn transaction_note() {
        let note = TransactionNote {
            id: Uuid::nil(),
            transaction_id: Uuid::nil(),
            note: "Salary payment Q1".into(),
            author: "finance@acme.com".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&note).unwrap();
        assert!(json.contains("Salary payment"));
    }

    #[test]
    fn staking_position_summary() {
        let pos = StakingPositionSummary {
            id: Uuid::nil(),
            vault_id: Uuid::nil(),
            chain: Chain::Ethereum,
            validator: "Lido".into(),
            amount_staked: dec!(32),
            rewards_earned: dec!(1.5),
            status: "active".into(),
            staked_at: Utc::now(),
            unbonding_at: None,
        };
        assert_eq!(pos.amount_staked, dec!(32));
    }

    #[test]
    fn approval_workflow_quorum() {
        let wf = ApprovalWorkflow {
            id: Uuid::nil(),
            withdrawal_id: Uuid::nil(),
            vault_id: Uuid::nil(),
            required_approvals: 2,
            current_approvals: 1,
            decisions: vec![ApprovalDecision {
                approver_id: Uuid::nil(),
                approver_email: "alice@co.com".into(),
                decision: Decision::Approve,
                reason: None,
                decided_at: Utc::now(),
            }],
            status: ApprovalStatus::Pending,
            expires_at: Utc::now(),
            created_at: Utc::now(),
        };
        assert_eq!(wf.decisions.len(), 1);
        assert!(wf.current_approvals < wf.required_approvals);
    }
}
