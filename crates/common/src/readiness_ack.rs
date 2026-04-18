//! In-memory incident acknowledgments.
//!
//! When a feature flips to Degraded, oncall usually wants to signal
//! "I'm on it" so the rest of the team stops paging for the same
//! incident. That's what this module provides: a per-feature flag
//! set by an admin endpoint, surfaced on `/v1/health/features` and
//! the dashboard as an "ACK by <user>" badge.
//!
//! Acks are **ephemeral**:
//!   - No DB persistence. A service restart clears every ack.
//!   - Auto-cleared when the refresh task observes the feature
//!     return to Ready — the incident is over, the ack is stale.
//!
//! This matches how Slack / PD handle acks: the signal is about
//! *humans coordinating in real time*, not long-term state. The
//! audit trail (who silenced what, when) lives in the audit log,
//! not here.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct IncidentAcks {
    inner: Arc<RwLock<HashMap<String, IncidentAck>>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IncidentAck {
    pub feature: String,
    pub acknowledged_by: String,
    pub acknowledged_at: chrono::DateTime<chrono::Utc>,
    pub message: String,
}

impl IncidentAcks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ack(
        &self,
        feature: impl Into<String>,
        acknowledged_by: impl Into<String>,
        message: impl Into<String>,
    ) {
        let feature = feature.into();
        let mut map = self.inner.write().expect("ack lock poisoned");
        map.insert(
            feature.clone(),
            IncidentAck {
                feature,
                acknowledged_by: acknowledged_by.into(),
                acknowledged_at: chrono::Utc::now(),
                message: message.into(),
            },
        );
    }

    /// Remove an ack. Returns true when one was actually removed —
    /// the endpoint uses this to report 404 on an ack-that-never-was.
    pub fn clear(&self, feature: &str) -> bool {
        let mut map = self.inner.write().expect("ack lock poisoned");
        map.remove(feature).is_some()
    }

    pub fn get(&self, feature: &str) -> Option<IncidentAck> {
        self.inner
            .read()
            .expect("ack lock poisoned")
            .get(feature)
            .cloned()
    }

    /// Snapshot every live ack.
    pub fn snapshot(&self) -> HashMap<String, IncidentAck> {
        self.inner.read().expect("ack lock poisoned").clone()
    }

    /// Drop any ack whose feature no longer has a live incident.
    /// Called by the refresh task after replacing the report —
    /// keeps stale acks from sticking around after recovery.
    pub fn prune_recovered(&self, live_incident_features: &std::collections::HashSet<String>) {
        let mut map = self.inner.write().expect("ack lock poisoned");
        map.retain(|feature, _| live_incident_features.contains(feature));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_round_trip() {
        let acks = IncidentAcks::new();
        acks.ack("scanner.ethereum", "oncall@example.com", "looking");
        let got = acks.get("scanner.ethereum").unwrap();
        assert_eq!(got.acknowledged_by, "oncall@example.com");
        assert_eq!(got.message, "looking");
    }

    #[test]
    fn clear_returns_false_when_absent() {
        let acks = IncidentAcks::new();
        assert!(!acks.clear("nope"));
        acks.ack("scanner.ethereum", "x", "x");
        assert!(acks.clear("scanner.ethereum"));
        assert!(!acks.clear("scanner.ethereum"));
    }

    #[test]
    fn prune_removes_recovered_features() {
        let acks = IncidentAcks::new();
        acks.ack("scanner.ethereum", "a", "a");
        acks.ack("push.apns", "b", "b");
        // Only scanner.ethereum is still broken.
        let live: std::collections::HashSet<String> =
            ["scanner.ethereum".to_string()].into_iter().collect();
        acks.prune_recovered(&live);
        assert!(acks.get("scanner.ethereum").is_some());
        assert!(acks.get("push.apns").is_none());
    }
}
