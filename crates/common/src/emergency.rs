//! Emergency freeze management.
//!
//! An `AtomicBool` flag (`FreezeFlag`) is stored in `AppState` and
//! checked at the start of every per-withdrawal iteration in the
//! pipeline (Received / Validated / Signed / Broadcast stages).
//!
//! Persistence: the audit_log is the system of record. The
//! freeze/unfreeze endpoints write the audit row BEFORE flipping
//! the in-memory flag, and startup hydrates the flag from the
//! most recent `EmergencyFreeze` / `EmergencyUnfreeze` row via
//! `CustodyDb::latest_freeze_state()`. That ordering means:
//!
//!   - A crash between audit-write and in-memory-flip leaves the
//!     DB as the authoritative record; the next startup re-
//!     hydrates the intended state.
//!   - A deploy / pod restart / OOM cannot silently clear a
//!     freeze set at 02:00 in response to a suspected compromise.
//!
//! If `latest_freeze_state()` itself errors on startup, the server
//! boots FROZEN fail-safe — an operator must explicitly unfreeze
//! once DB health is verified. Better to halt outflows on a
//! transient DB read error than drain funds under an unknown
//! state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Shared freeze flag.  `true` = all outbound operations frozen.
#[derive(Clone, Debug)]
pub struct FreezeFlag(pub Arc<AtomicBool>);

impl FreezeFlag {
    /// Create a new, unfrozen flag.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Returns `true` when the system is currently frozen.
    #[inline]
    pub fn is_frozen(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Freeze the system.  Returns `true` if the transition actually happened
    /// (i.e., the system was not already frozen).
    pub fn freeze(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }

    /// Unfreeze the system.  Returns `true` if the transition actually happened.
    pub fn unfreeze(&self) -> bool {
        self.0.swap(false, Ordering::SeqCst)
    }
}

impl Default for FreezeFlag {
    fn default() -> Self {
        Self::new()
    }
}

/// API response body for `/v1/admin/emergency/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeStatus {
    pub frozen: bool,
    pub checked_at: DateTime<Utc>,
}

/// Request body for `POST /v1/admin/emergency/freeze`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeRequest {
    /// Human-readable reason logged in the audit trail.
    pub reason: String,
}

/// Response body for freeze/unfreeze endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreezeResponse {
    /// Current state after the operation.
    pub frozen: bool,
    /// Whether a state transition actually occurred.
    pub transitioned: bool,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Request body for force-complete / force-cancel override endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideRequest {
    /// Mandatory operator justification — written to the audit trail.
    pub reason: String,
}

/// Response body for withdrawal override endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideResponse {
    pub withdrawal_id: uuid::Uuid,
    pub action: String,
    pub previous_status: String,
    pub new_status: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_unfrozen() {
        let flag = FreezeFlag::new();
        assert!(!flag.is_frozen());
    }

    #[test]
    fn freeze_transitions_once() {
        let flag = FreezeFlag::new();
        // First freeze returns true (transition happened).
        assert!(flag.freeze());
        assert!(flag.is_frozen());
        // Second freeze returns false (already frozen).
        assert!(!flag.freeze());
    }

    #[test]
    fn unfreeze_after_freeze() {
        let flag = FreezeFlag::new();
        flag.freeze();
        assert!(flag.is_frozen());
        // Unfreeze returns true.
        assert!(flag.unfreeze());
        assert!(!flag.is_frozen());
    }

    #[test]
    fn unfreeze_when_already_unfrozen() {
        let flag = FreezeFlag::new();
        // Returns false — no transition.
        assert!(!flag.unfreeze());
        assert!(!flag.is_frozen());
    }

    #[test]
    fn clone_shares_state() {
        let flag = FreezeFlag::new();
        let flag2 = flag.clone();
        flag.freeze();
        // Both views must reflect the same state.
        assert!(flag2.is_frozen());
    }

    #[test]
    fn freeze_status_serialization() {
        let s = FreezeStatus { frozen: true, checked_at: Utc::now() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"frozen\":true"));
    }

    #[test]
    fn freeze_response_serialization() {
        let r = FreezeResponse {
            frozen: true,
            transitioned: true,
            reason: Some("Security incident".into()),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"transitioned\":true"));
        assert!(json.contains("Security incident"));
    }

    #[test]
    fn override_response_serialization() {
        let r = OverrideResponse {
            withdrawal_id: uuid::Uuid::nil(),
            action: "force_cancel".into(),
            previous_status: "Broadcast".into(),
            new_status: "Failed".into(),
            reason: "Stuck for 48h".into(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("force_cancel"));
    }
}
