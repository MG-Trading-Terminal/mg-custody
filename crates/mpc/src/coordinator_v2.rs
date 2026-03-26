//! Multi-process MPC party coordination.
//!
//! Coordinates key generation and signing across multiple independent processes
//! (each running on a separate host). Uses the MpcTransport for communication.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::transport::{MpcMessage, MpcSessionConfig, MpcSessionType, MpcTransport, sign_message};

/// Coordinator state machine for an MPC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCoordinator {
    pub config: MpcSessionConfig,
    pub state: CoordinatorState,
    pub current_round: u32,
    pub rounds_completed: Vec<u32>,
    pub started_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinatorState {
    WaitingForParties,
    RoundInProgress,
    RoundComplete,
    SessionComplete,
    Failed,
    TimedOut,
}

impl SessionCoordinator {
    /// Create a new coordinator for an MPC session.
    pub fn new(config: MpcSessionConfig) -> Self {
        let timeout = config.timeout_secs;
        Self {
            config,
            state: CoordinatorState::WaitingForParties,
            current_round: 0,
            rounds_completed: Vec::new(),
            started_at: Utc::now(),
            deadline: Utc::now() + Duration::seconds(timeout as i64),
        }
    }

    /// Check if all required parties are online.
    pub fn all_parties_ready(&self) -> bool {
        let online = self.config.parties.iter().filter(|p| p.is_online).count();
        online >= self.config.threshold as usize
    }

    /// Start the next round.
    pub fn start_round(&mut self) -> Result<u32> {
        if self.is_timed_out() {
            self.state = CoordinatorState::TimedOut;
            anyhow::bail!("Session timed out");
        }
        if !self.all_parties_ready() {
            anyhow::bail!("Not enough parties online ({} needed)", self.config.threshold);
        }

        self.current_round += 1;
        self.state = CoordinatorState::RoundInProgress;
        tracing::info!(
            session = %self.config.session_id,
            round = self.current_round,
            "Starting MPC round"
        );
        Ok(self.current_round)
    }

    /// Mark current round as complete.
    pub fn complete_round(&mut self) {
        self.rounds_completed.push(self.current_round);
        self.state = CoordinatorState::RoundComplete;
        tracing::info!(
            session = %self.config.session_id,
            round = self.current_round,
            "MPC round completed"
        );
    }

    /// Mark the entire session as complete.
    pub fn complete_session(&mut self) {
        self.state = CoordinatorState::SessionComplete;
        tracing::info!(
            session = %self.config.session_id,
            rounds = self.rounds_completed.len(),
            "MPC session completed successfully"
        );
    }

    /// Mark session as failed.
    pub fn fail(&mut self, reason: &str) {
        self.state = CoordinatorState::Failed;
        tracing::error!(
            session = %self.config.session_id,
            reason = reason,
            "MPC session failed"
        );
    }

    pub fn is_timed_out(&self) -> bool {
        Utc::now() > self.deadline
    }

    pub fn is_complete(&self) -> bool {
        self.state == CoordinatorState::SessionComplete
    }

    /// Expected number of rounds for this session type.
    pub fn expected_rounds(&self) -> u32 {
        match self.config.session_type {
            MpcSessionType::EcdsaKeygen => 6,    // CGGMP21 keygen
            MpcSessionType::EcdsaSign => 4,      // CGGMP21 presign + sign
            MpcSessionType::FrostKeygen => 2,    // FROST DKG (2 rounds)
            MpcSessionType::FrostSign => 2,      // FROST sign (2 rounds)
            MpcSessionType::KeyRotation => 8,    // Full re-sharing
        }
    }

    /// Progress as percentage.
    pub fn progress_percent(&self) -> u32 {
        let expected = self.expected_rounds();
        if expected == 0 { return 100; }
        ((self.rounds_completed.len() as u32 * 100) / expected).min(100)
    }
}

/// Broadcast a round message to all parties via transport.
pub fn broadcast_round<T: MpcTransport>(
    transport: &T,
    session_id: Uuid,
    round: u32,
    from_party: u16,
    payload: Vec<u8>,
    secret: &[u8],
) -> Result<()> {
    let mut msg = MpcMessage {
        session_id, round, from_party,
        to_party: None,
        payload,
        hmac: String::new(),
        timestamp: Utc::now(),
    };
    msg.hmac = sign_message(secret, &msg);
    transport.broadcast(&msg).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

/// Collect messages from all parties for a round.
pub fn collect_round<T: MpcTransport>(
    transport: &T,
    session_id: Uuid,
    round: u32,
    expected_count: usize,
    timeout_ms: u64,
) -> Result<Vec<MpcMessage>> {
    let messages = transport.receive(session_id, round, timeout_ms)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if messages.len() < expected_count {
        anyhow::bail!(
            "Incomplete round {round}: got {} messages, expected {expected_count}",
            messages.len()
        );
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{InMemoryTransport, MpcParty};

    fn test_config() -> MpcSessionConfig {
        MpcSessionConfig {
            session_id: Uuid::new_v4(),
            session_type: MpcSessionType::FrostSign,
            threshold: 2, total_parties: 3,
            parties: vec![
                MpcParty { id: Uuid::new_v4(), index: 0, endpoint: "h1".into(), public_key: "pk0".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
                MpcParty { id: Uuid::new_v4(), index: 1, endpoint: "h2".into(), public_key: "pk1".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
                MpcParty { id: Uuid::new_v4(), index: 2, endpoint: "h3".into(), public_key: "pk2".into(), is_online: false, last_heartbeat: None },
            ],
            timeout_secs: 60,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn coordinator_creation() {
        let coord = SessionCoordinator::new(test_config());
        assert_eq!(coord.state, CoordinatorState::WaitingForParties);
        assert_eq!(coord.current_round, 0);
    }

    #[test]
    fn all_parties_ready_threshold() {
        let coord = SessionCoordinator::new(test_config());
        // 2 online, threshold = 2 → ready
        assert!(coord.all_parties_ready());
    }

    #[test]
    fn start_round() {
        let mut coord = SessionCoordinator::new(test_config());
        let round = coord.start_round().unwrap();
        assert_eq!(round, 1);
        assert_eq!(coord.state, CoordinatorState::RoundInProgress);
    }

    #[test]
    fn complete_round() {
        let mut coord = SessionCoordinator::new(test_config());
        coord.start_round().unwrap();
        coord.complete_round();
        assert_eq!(coord.state, CoordinatorState::RoundComplete);
        assert_eq!(coord.rounds_completed, vec![1]);
    }

    #[test]
    fn complete_session() {
        let mut coord = SessionCoordinator::new(test_config());
        coord.start_round().unwrap();
        coord.complete_round();
        coord.start_round().unwrap();
        coord.complete_round();
        coord.complete_session();
        assert!(coord.is_complete());
    }

    #[test]
    fn progress_percent() {
        let mut coord = SessionCoordinator::new(test_config());
        assert_eq!(coord.progress_percent(), 0);
        coord.start_round().unwrap();
        coord.complete_round();
        // FROST sign = 2 rounds expected, 1 done = 50%
        assert_eq!(coord.progress_percent(), 50);
    }

    #[test]
    fn expected_rounds_per_type() {
        let mut config = test_config();
        config.session_type = MpcSessionType::EcdsaKeygen;
        let coord = SessionCoordinator::new(config);
        assert_eq!(coord.expected_rounds(), 6);
    }

    #[test]
    fn broadcast_and_collect() {
        let transport = InMemoryTransport::new();
        let session_id = Uuid::new_v4();

        broadcast_round(&transport, session_id, 1, 0, vec![0xAA], b"secret").unwrap();
        broadcast_round(&transport, session_id, 1, 1, vec![0xBB], b"secret").unwrap();

        let msgs = collect_round(&transport, session_id, 1, 2, 1000).unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn collect_incomplete_fails() {
        let transport = InMemoryTransport::new();
        let session_id = Uuid::new_v4();
        broadcast_round(&transport, session_id, 1, 0, vec![0xAA], b"s").unwrap();

        let result = collect_round(&transport, session_id, 1, 3, 100);
        assert!(result.is_err());
    }

    #[test]
    fn coordinator_state_serialization() {
        assert_eq!(serde_json::to_string(&CoordinatorState::RoundInProgress).unwrap(), r#""round_in_progress""#);
        assert_eq!(serde_json::to_string(&CoordinatorState::SessionComplete).unwrap(), r#""session_complete""#);
    }
}
