//! Key rotation scheduler.
//!
//! Monitors key age and triggers rotation when keys exceed the configured lifetime.
//! Key rotation creates new key shares via MPC ceremony while old keys remain
//! valid for a transition period.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Key rotation policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Maximum key age before rotation (default: 90 days).
    pub max_key_age_days: u32,
    /// Warning period before mandatory rotation (default: 14 days).
    pub warning_days: u32,
    /// Overlap period where both old and new keys are valid (default: 7 days).
    pub overlap_days: u32,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            max_key_age_days: 90,
            warning_days: 14,
            overlap_days: 7,
        }
    }
}

/// Status of a managed key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedKey {
    pub key_id: String,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub successor_key_id: Option<String>,
    pub status: KeyStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    Active,
    PendingRotation,
    Rotating,
    Retired,
    Compromised,
}

/// Key rotation scheduler — checks key ages and initiates rotations.
pub struct KeyRotationScheduler {
    policy: RotationPolicy,
    keys: Vec<ManagedKey>,
}

impl KeyRotationScheduler {
    pub fn new(policy: RotationPolicy) -> Self {
        Self { policy, keys: Vec::new() }
    }

    /// Register a key for rotation tracking.
    pub fn register_key(&mut self, key_id: &str, created_at: DateTime<Utc>) {
        self.keys.push(ManagedKey {
            key_id: key_id.to_string(),
            created_at,
            rotated_at: None,
            successor_key_id: None,
            status: KeyStatus::Active,
        });
    }

    /// Check all keys and return those needing rotation.
    pub fn check_rotations(&mut self) -> Vec<RotationAction> {
        let now = Utc::now();
        let mut actions = Vec::new();

        for key in &mut self.keys {
            if key.status != KeyStatus::Active { continue; }

            let age_days = (now - key.created_at).num_days() as u32;

            if age_days >= self.policy.max_key_age_days {
                // Mandatory rotation
                key.status = KeyStatus::PendingRotation;
                actions.push(RotationAction {
                    key_id: key.key_id.clone(),
                    reason: RotationReason::MaxAgeExceeded,
                    urgency: RotationUrgency::Mandatory,
                    new_key_id: Uuid::new_v4().to_string(),
                });
                tracing::warn!(
                    key_id = %key.key_id, age_days,
                    "Key rotation MANDATORY — max age exceeded"
                );
            } else if age_days >= self.policy.max_key_age_days - self.policy.warning_days {
                // Warning — rotation recommended
                actions.push(RotationAction {
                    key_id: key.key_id.clone(),
                    reason: RotationReason::ApproachingMaxAge,
                    urgency: RotationUrgency::Recommended,
                    new_key_id: Uuid::new_v4().to_string(),
                });
                tracing::info!(
                    key_id = %key.key_id, age_days,
                    days_until_mandatory = self.policy.max_key_age_days - age_days,
                    "Key rotation recommended"
                );
            }
        }

        actions
    }

    /// Mark a rotation as completed.
    pub fn complete_rotation(&mut self, old_key_id: &str, new_key_id: &str) {
        if let Some(key) = self.keys.iter_mut().find(|k| k.key_id == old_key_id) {
            key.status = KeyStatus::Retired;
            key.rotated_at = Some(Utc::now());
            key.successor_key_id = Some(new_key_id.to_string());
            tracing::info!(old = %old_key_id, new = %new_key_id, "Key rotation completed");
        }
    }

    /// Emergency revocation — immediately mark key as compromised.
    pub fn revoke_key(&mut self, key_id: &str, reason: &str) {
        if let Some(key) = self.keys.iter_mut().find(|k| k.key_id == key_id) {
            key.status = KeyStatus::Compromised;
            tracing::error!(key_id = %key_id, reason, "KEY COMPROMISED — emergency revocation");
        }
    }

    /// Get status of all managed keys.
    pub fn key_statuses(&self) -> &[ManagedKey] {
        &self.keys
    }
}

/// An action to take for key rotation.
#[derive(Debug, Clone)]
pub struct RotationAction {
    pub key_id: String,
    pub reason: RotationReason,
    pub urgency: RotationUrgency,
    pub new_key_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationReason {
    MaxAgeExceeded,
    ApproachingMaxAge,
    ManualRequest,
    SecurityIncident,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationUrgency {
    Recommended,
    Mandatory,
    Emergency,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_key_no_rotation_needed() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy::default());
        scheduler.register_key("key1", Utc::now());
        let actions = scheduler.check_rotations();
        assert!(actions.is_empty());
    }

    #[test]
    fn old_key_needs_rotation() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy {
            max_key_age_days: 90,
            warning_days: 14,
            overlap_days: 7,
        });

        // Key created 100 days ago
        let old_date = Utc::now() - chrono::Duration::days(100);
        scheduler.register_key("old_key", old_date);

        let actions = scheduler.check_rotations();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].urgency, RotationUrgency::Mandatory);
        assert_eq!(actions[0].reason, RotationReason::MaxAgeExceeded);
    }

    #[test]
    fn approaching_max_age_warning() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy {
            max_key_age_days: 90,
            warning_days: 14,
            overlap_days: 7,
        });

        // Key created 80 days ago (within warning period)
        let date = Utc::now() - chrono::Duration::days(80);
        scheduler.register_key("warning_key", date);

        let actions = scheduler.check_rotations();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].urgency, RotationUrgency::Recommended);
    }

    #[test]
    fn rotation_lifecycle() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy::default());
        scheduler.register_key("key1", Utc::now() - chrono::Duration::days(100));

        let actions = scheduler.check_rotations();
        assert_eq!(actions.len(), 1);

        // Complete rotation
        scheduler.complete_rotation("key1", "key2");

        let statuses = scheduler.key_statuses();
        assert_eq!(statuses[0].status, KeyStatus::Retired);
        assert_eq!(statuses[0].successor_key_id, Some("key2".to_string()));
    }

    #[test]
    fn emergency_revocation() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy::default());
        scheduler.register_key("compromised_key", Utc::now());

        scheduler.revoke_key("compromised_key", "Suspected breach");

        let statuses = scheduler.key_statuses();
        assert_eq!(statuses[0].status, KeyStatus::Compromised);
    }

    #[test]
    fn retired_key_not_checked() {
        let mut scheduler = KeyRotationScheduler::new(RotationPolicy::default());
        scheduler.register_key("old", Utc::now() - chrono::Duration::days(200));

        // First check triggers rotation
        let actions1 = scheduler.check_rotations();
        assert_eq!(actions1.len(), 1);

        // Mark as retired
        scheduler.complete_rotation("old", "new");

        // Second check — no actions (retired key skipped)
        let actions2 = scheduler.check_rotations();
        assert!(actions2.is_empty());
    }
}
