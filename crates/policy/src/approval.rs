//! Multi-approver workflow for large withdrawals.
//!
//! When a policy rule requires approval, an ApprovalRequest is created.
//! Approvers with the required role submit decisions.
//! When quorum is reached, the withdrawal proceeds.
//! Requests expire after a configurable timeout.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rbac::Role;

/// Status of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Waiting for approvals.
    Pending,
    /// Quorum reached — approved.
    Approved,
    /// Explicitly rejected by an approver.
    Rejected,
    /// Timed out without reaching quorum.
    Expired,
}

/// A single approval decision from one approver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approver_id: String,
    pub approver_role: Role,
    pub approved: bool,
    pub reason: Option<String>,
    pub decided_at: DateTime<Utc>,
}

/// An approval request for a pending withdrawal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub required_approvals: u16,
    pub required_role: Role,
    pub decisions: Vec<ApprovalDecision>,
    pub status: ApprovalStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Default approval expiry: 24 hours.
pub const DEFAULT_EXPIRY_HOURS: i64 = 24;

impl ApprovalRequest {
    /// Create a new approval request.
    pub fn new(
        withdrawal_id: Uuid,
        required_approvals: u16,
        required_role: Role,
        expiry_hours: Option<i64>,
    ) -> Self {
        let now = Utc::now();
        let expiry = expiry_hours.unwrap_or(DEFAULT_EXPIRY_HOURS);
        Self {
            id: Uuid::new_v4(),
            withdrawal_id,
            required_approvals,
            required_role,
            decisions: Vec::new(),
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at: now + Duration::hours(expiry),
        }
    }

    /// Submit an approval decision.
    ///
    /// Returns the updated status. Rejects duplicate approvals from the same approver.
    pub fn submit_decision(&mut self, decision: ApprovalDecision) -> ApprovalStatus {
        // Check expiry
        if Utc::now() > self.expires_at {
            self.status = ApprovalStatus::Expired;
            return self.status;
        }

        // Already resolved
        if self.status != ApprovalStatus::Pending {
            return self.status;
        }

        // Check role
        if !decision.approver_role.has_at_least(&self.required_role) {
            return self.status; // Insufficient role, ignore
        }

        // Check duplicate
        if self.decisions.iter().any(|d| d.approver_id == decision.approver_id) {
            return self.status; // Already voted
        }

        // If rejected, the whole request is rejected
        if !decision.approved {
            self.decisions.push(decision);
            self.status = ApprovalStatus::Rejected;
            return self.status;
        }

        self.decisions.push(decision);

        // Check if quorum reached
        let approval_count = self.decisions.iter().filter(|d| d.approved).count();
        if approval_count >= self.required_approvals as usize {
            self.status = ApprovalStatus::Approved;
        }

        self.status
    }

    /// Check if the request has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Number of approvals received so far.
    pub fn approval_count(&self) -> usize {
        self.decisions.iter().filter(|d| d.approved).count()
    }

    /// Number of approvals still needed.
    pub fn approvals_remaining(&self) -> u16 {
        let count = self.approval_count() as u16;
        self.required_approvals.saturating_sub(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_decision(approver: &str, role: Role, approved: bool) -> ApprovalDecision {
        ApprovalDecision {
            approver_id: approver.to_string(),
            approver_role: role,
            approved,
            reason: None,
            decided_at: Utc::now(),
        }
    }

    #[test]
    fn single_approval_quorum() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 1, Role::Signer, Some(24));
        assert_eq!(req.status, ApprovalStatus::Pending);
        assert_eq!(req.approvals_remaining(), 1);

        let status = req.submit_decision(make_decision("signer1", Role::Signer, true));
        assert_eq!(status, ApprovalStatus::Approved);
        assert_eq!(req.approvals_remaining(), 0);
    }

    #[test]
    fn two_of_three_quorum() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 2, Role::Admin, Some(24));

        req.submit_decision(make_decision("admin1", Role::Admin, true));
        assert_eq!(req.status, ApprovalStatus::Pending);
        assert_eq!(req.approvals_remaining(), 1);

        req.submit_decision(make_decision("admin2", Role::Admin, true));
        assert_eq!(req.status, ApprovalStatus::Approved);
    }

    #[test]
    fn rejection_immediately_rejects() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 2, Role::Signer, Some(24));

        req.submit_decision(make_decision("signer1", Role::Signer, true));
        assert_eq!(req.status, ApprovalStatus::Pending);

        req.submit_decision(make_decision("signer2", Role::Signer, false));
        assert_eq!(req.status, ApprovalStatus::Rejected);
    }

    #[test]
    fn duplicate_approver_ignored() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 2, Role::Signer, Some(24));

        req.submit_decision(make_decision("signer1", Role::Signer, true));
        req.submit_decision(make_decision("signer1", Role::Signer, true)); // duplicate
        assert_eq!(req.status, ApprovalStatus::Pending); // still need 1 more
        assert_eq!(req.approval_count(), 1);
    }

    #[test]
    fn insufficient_role_ignored() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 1, Role::Admin, Some(24));

        // Editor doesn't have Admin role
        req.submit_decision(make_decision("editor1", Role::Editor, true));
        assert_eq!(req.status, ApprovalStatus::Pending); // not counted
    }

    #[test]
    fn owner_counts_for_admin_approval() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 1, Role::Admin, Some(24));

        req.submit_decision(make_decision("owner", Role::Owner, true));
        assert_eq!(req.status, ApprovalStatus::Approved); // Owner >= Admin
    }

    #[test]
    fn expired_request() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 1, Role::Signer, Some(24));
        // Manually expire
        req.expires_at = Utc::now() - Duration::hours(1);

        let status = req.submit_decision(make_decision("signer1", Role::Signer, true));
        assert_eq!(status, ApprovalStatus::Expired);
    }

    #[test]
    fn already_approved_ignores_new_decisions() {
        let mut req = ApprovalRequest::new(Uuid::new_v4(), 1, Role::Signer, Some(24));
        req.submit_decision(make_decision("signer1", Role::Signer, true));
        assert_eq!(req.status, ApprovalStatus::Approved);

        // New decision after approval
        let status = req.submit_decision(make_decision("signer2", Role::Signer, false));
        assert_eq!(status, ApprovalStatus::Approved); // unchanged
    }
}
