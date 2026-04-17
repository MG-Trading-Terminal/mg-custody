//! Chaos testing for the readiness pipeline.
//!
//! The alerting system is itself a system — probes, transitions,
//! cooldowns, webhooks. None of it means anything if we don't have
//! evidence it actually fires on real degradations. Running a
//! genuine chaos experiment (pull the RPC cable, kill the HSM
//! daemon) is slow and scary; this module provides the cheap
//! alternative: an in-memory override map that replaces a feature's
//! state after the real probe runs.
//!
//! Flow:
//!   1. Operator hits `POST /v1/admin/chaos/overrides`
//!      with `{ feature: "scanner.ethereum", state: "degraded", reason: "test" }`
//!   2. The refresh task on its next tick:
//!        - probes for real (scanner.ethereum comes back Ready)
//!        - applies the override (flips to Degraded with reason)
//!        - diffs against previous (Ready → Degraded transition)
//!        - fires alerts + persists history row
//!   3. Oncall sees the page, confirms the channel works.
//!   4. Operator hits `DELETE /v1/admin/chaos/overrides/:feature`
//!      which clears the override.
//!   5. Next refresh: real probe (Ready) vs stored (Degraded) =
//!      Recovery transition. Webhook fires the "recovered" signal.
//!
//! Overrides are NOT persisted — a service restart clears them.
//! That's intentional: a forgotten chaos override is a production
//! incident, restart-recovery is the safety valve.

use crate::readiness::{FeatureState, ReadinessReport};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Shareable handle for the override map. Cloneable so the gateway
/// (mutates via admin endpoints) and the refresh task (reads on
/// every tick) can both hold one.
#[derive(Clone, Default)]
pub struct ChaosOverrides {
    inner: Arc<RwLock<HashMap<String, ChaosOverride>>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChaosOverride {
    pub state: FeatureState,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
}

impl ChaosOverrides {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace an override.
    pub fn set(&self, feature: impl Into<String>, state: FeatureState, reason: impl Into<String>, created_by: impl Into<String>) {
        let mut map = self.inner.write().expect("chaos lock poisoned");
        map.insert(
            feature.into(),
            ChaosOverride {
                state,
                reason: reason.into(),
                created_at: chrono::Utc::now(),
                created_by: created_by.into(),
            },
        );
    }

    /// Remove an override. Returns true when one was actually removed.
    pub fn clear(&self, feature: &str) -> bool {
        let mut map = self.inner.write().expect("chaos lock poisoned");
        map.remove(feature).is_some()
    }

    /// Return a snapshot of every active override. Serializable so
    /// the GET endpoint can return them verbatim.
    pub fn snapshot(&self) -> HashMap<String, ChaosOverride> {
        self.inner.read().expect("chaos lock poisoned").clone()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().expect("chaos lock poisoned").is_empty()
    }

    /// Apply the override map to a freshly-built report. Features
    /// present in the override map have their state replaced (with
    /// the override reason appended to the report's state-reason so
    /// operators looking at `/health/features` see both the underlying
    /// probe result and the chaos note). Features not in the map
    /// pass through unchanged.
    pub fn apply(&self, report: &mut ReadinessReport) {
        let map = self.inner.read().expect("chaos lock poisoned");
        if map.is_empty() {
            return;
        }
        for feat in &mut report.features {
            let Some(over) = map.get(&feat.name) else {
                continue;
            };
            feat.state = match &over.state {
                FeatureState::Ready => FeatureState::Ready,
                FeatureState::Disabled => FeatureState::Disabled,
                FeatureState::Degraded { .. } => FeatureState::Degraded {
                    reason: format!("[chaos] {}", over.reason),
                },
                FeatureState::Misconfigured { missing } => FeatureState::Misconfigured {
                    missing: missing.clone(),
                },
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readiness::FeatureStatus;

    fn report_with(features: Vec<FeatureStatus>) -> ReadinessReport {
        let mut r = ReadinessReport::new();
        for f in features {
            r.push(f);
        }
        r
    }

    #[test]
    fn apply_overrides_flips_feature_state() {
        let overrides = ChaosOverrides::new();
        overrides.set(
            "scanner.ethereum",
            FeatureState::Degraded {
                reason: "testing".into(),
            },
            "oncall drill",
            "ops@example.com",
        );
        let mut report = report_with(vec![FeatureStatus::ready(
            "scanner.ethereum",
            "ETH scanner",
        )]);
        overrides.apply(&mut report);
        let f = &report.features[0];
        assert!(matches!(f.state, FeatureState::Degraded { .. }));
        // Override reason is visible in the persisted state.
        assert!(f.state.reason().contains("oncall drill"));
    }

    #[test]
    fn apply_without_overrides_is_noop() {
        let overrides = ChaosOverrides::new();
        let mut report = report_with(vec![FeatureStatus::ready(
            "scanner.ethereum",
            "ETH scanner",
        )]);
        overrides.apply(&mut report);
        assert!(matches!(report.features[0].state, FeatureState::Ready));
    }

    #[test]
    fn clear_returns_false_for_missing_feature() {
        let overrides = ChaosOverrides::new();
        assert!(!overrides.clear("scanner.ethereum"));
        overrides.set(
            "scanner.ethereum",
            FeatureState::Degraded {
                reason: "x".into(),
            },
            "x",
            "x",
        );
        assert!(overrides.clear("scanner.ethereum"));
        assert!(!overrides.clear("scanner.ethereum"));
    }

    #[test]
    fn override_on_unknown_feature_is_silent() {
        // Setting an override for a feature the report doesn't
        // contain shouldn't error — the refresh task may drop a
        // feature between the endpoint hit and the next tick.
        let overrides = ChaosOverrides::new();
        overrides.set(
            "does.not.exist",
            FeatureState::Degraded {
                reason: "x".into(),
            },
            "x",
            "x",
        );
        let mut report = report_with(vec![FeatureStatus::ready("real.feature", "x")]);
        overrides.apply(&mut report);
        assert!(matches!(report.features[0].state, FeatureState::Ready));
    }
}
