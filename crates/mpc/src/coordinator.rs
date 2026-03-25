//! MPC session coordinator.
//!
//! Manages the lifecycle of MPC keygen and signing sessions.
//! Routes messages between parties (for local testing, this is in-memory;
//! for production, messages flow through a secure channel to the HSM).

use anyhow::Result;
use uuid::Uuid;

/// A unique MPC session identifier.
pub type SessionId = String;

/// State of an MPC session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Session created, waiting for parties to join.
    Pending,
    /// Protocol rounds in progress.
    InProgress,
    /// Protocol completed successfully.
    Completed,
    /// Protocol failed.
    Failed(String),
}

/// An MPC session (keygen or signing).
#[derive(Debug)]
pub struct MpcSession {
    pub id: SessionId,
    pub session_type: SessionType,
    pub state: SessionState,
    pub threshold: u16,
    pub parties: u16,
}

/// Type of MPC session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionType {
    EcdsaKeygen,
    EcdsaSign { key_id: String },
    FrostKeygen,
    FrostSign { key_id: String },
}

impl MpcSession {
    /// Create a new MPC session.
    pub fn new(session_type: SessionType, threshold: u16, parties: u16) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_type,
            state: SessionState::Pending,
            threshold,
            parties,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_creation() {
        let session = MpcSession::new(SessionType::EcdsaKeygen, 2, 3);
        assert_eq!(session.state, SessionState::Pending);
        assert_eq!(session.threshold, 2);
        assert_eq!(session.parties, 3);
        assert!(!session.id.is_empty());
    }

    #[test]
    fn session_types() {
        let keygen = SessionType::EcdsaKeygen;
        let sign = SessionType::EcdsaSign { key_id: "key1".into() };
        assert_ne!(keygen, sign);
    }
}
