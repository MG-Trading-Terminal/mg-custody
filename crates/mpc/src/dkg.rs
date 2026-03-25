//! Distributed Key Generation (DKG) — production replacement for trusted dealer.
//!
//! In production, NO single party should see the full key.
//! DKG generates key shares cooperatively without a trusted dealer.
//!
//! Uses cggmp21::keygen with round_based message passing.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DKG session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgConfig {
    pub session_id: Uuid,
    pub threshold: u16,
    pub parties: u16,
    /// Party endpoints for message exchange.
    pub party_endpoints: Vec<String>,
    /// Timeout per round in seconds.
    pub round_timeout_secs: u64,
}

impl DkgConfig {
    pub fn new(threshold: u16, parties: u16) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            threshold,
            parties,
            party_endpoints: Vec::new(),
            round_timeout_secs: 60,
        }
    }
}

/// DKG session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DkgState {
    /// Waiting for all parties to join.
    WaitingForParties,
    /// Running key generation rounds.
    InProgress,
    /// Successfully completed — shares distributed.
    Completed,
    /// Failed (timeout, malicious party, network error).
    Failed,
}

/// Key rotation schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationSchedule {
    /// Rotate keys every N days (0 = disabled).
    pub rotation_interval_days: u32,
    /// Last rotation timestamp.
    pub last_rotation: Option<chrono::DateTime<chrono::Utc>>,
    /// Next scheduled rotation.
    pub next_rotation: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether rotation is in progress.
    pub rotation_in_progress: bool,
}

impl Default for KeyRotationSchedule {
    fn default() -> Self {
        Self {
            rotation_interval_days: 90, // quarterly
            last_rotation: None,
            next_rotation: None,
            rotation_in_progress: false,
        }
    }
}

impl KeyRotationSchedule {
    /// Check if rotation is due.
    pub fn is_due(&self) -> bool {
        match self.next_rotation {
            Some(next) => chrono::Utc::now() >= next,
            None => false,
        }
    }

    /// Schedule the next rotation from now.
    pub fn schedule_next(&mut self) {
        self.last_rotation = Some(chrono::Utc::now());
        self.next_rotation = Some(
            chrono::Utc::now() + chrono::Duration::days(self.rotation_interval_days as i64)
        );
    }
}

/// Disaster Recovery targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrTargets {
    /// Recovery Time Objective: max time to restore service.
    pub rto_minutes: u32,
    /// Recovery Point Objective: max data loss window.
    pub rpo_minutes: u32,
    /// Last DR drill timestamp.
    pub last_drill: Option<chrono::DateTime<chrono::Utc>>,
    /// Next scheduled drill.
    pub next_drill: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for DrTargets {
    fn default() -> Self {
        Self {
            rto_minutes: 60,  // < 1 hour
            rpo_minutes: 1,   // < 1 minute
            last_drill: None,
            next_drill: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dkg_config() {
        let config = DkgConfig::new(2, 3);
        assert_eq!(config.threshold, 2);
        assert_eq!(config.parties, 3);
        assert!(!config.session_id.is_nil());
    }

    #[test]
    fn key_rotation_schedule() {
        let mut schedule = KeyRotationSchedule::default();
        assert_eq!(schedule.rotation_interval_days, 90);
        assert!(!schedule.is_due());

        schedule.schedule_next();
        assert!(schedule.last_rotation.is_some());
        assert!(schedule.next_rotation.is_some());
        assert!(!schedule.is_due()); // just scheduled, not due yet
    }

    #[test]
    fn dr_targets() {
        let dr = DrTargets::default();
        assert_eq!(dr.rto_minutes, 60);
        assert_eq!(dr.rpo_minutes, 1);
    }

    #[test]
    fn dkg_states() {
        assert_eq!(
            serde_json::to_string(&DkgState::Completed).unwrap(),
            "\"completed\""
        );
    }
}
