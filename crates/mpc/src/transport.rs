//! MPC network transport — authenticated channels between signing parties.
//!
//! Phase 4 completion: algorithms work (CGGMP21/FROST), but need network
//! transport for real distributed key generation and signing.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// A participant in an MPC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpcParty {
    pub id: Uuid,
    pub index: u16,
    pub endpoint: String,
    pub public_key: String,
    pub is_online: bool,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

/// MPC session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpcSessionConfig {
    pub session_id: Uuid,
    pub session_type: MpcSessionType,
    pub threshold: u16,
    pub total_parties: u16,
    pub parties: Vec<MpcParty>,
    pub timeout_secs: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MpcSessionType {
    EcdsaKeygen,
    EcdsaSign,
    FrostKeygen,
    FrostSign,
    KeyRotation,
}

/// A message sent between MPC parties during a protocol round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpcMessage {
    pub session_id: Uuid,
    pub round: u32,
    pub from_party: u16,
    pub to_party: Option<u16>, // None = broadcast
    pub payload: Vec<u8>,
    pub hmac: String,
    pub timestamp: DateTime<Utc>,
}

/// Transport trait for MPC communication.
pub trait MpcTransport: Send + Sync {
    /// Send a message to a specific party.
    fn send(&self, msg: &MpcMessage) -> Result<(), TransportError>;
    /// Broadcast a message to all parties.
    fn broadcast(&self, msg: &MpcMessage) -> Result<(), TransportError>;
    /// Receive messages for the current round (blocking with timeout).
    fn receive(&self, session_id: Uuid, round: u32, timeout_ms: u64) -> Result<Vec<MpcMessage>, TransportError>;
}

#[derive(Debug, Clone)]
pub enum TransportError {
    ConnectionFailed(String),
    Timeout,
    AuthenticationFailed,
    MessageTooLarge,
    SessionNotFound,
    PartyOffline(u16),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {msg}"),
            Self::Timeout => write!(f, "Transport timeout"),
            Self::AuthenticationFailed => write!(f, "Authentication failed"),
            Self::MessageTooLarge => write!(f, "Message exceeds size limit"),
            Self::SessionNotFound => write!(f, "Session not found"),
            Self::PartyOffline(id) => write!(f, "Party {id} is offline"),
        }
    }
}

/// In-memory transport for testing (simulates network between parties in same process).
pub struct InMemoryTransport {
    messages: std::sync::Mutex<Vec<MpcMessage>>,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self { messages: std::sync::Mutex::new(Vec::new()) }
    }

    pub fn message_count(&self) -> usize {
        self.messages.lock().unwrap().len()
    }
}

impl MpcTransport for InMemoryTransport {
    fn send(&self, msg: &MpcMessage) -> Result<(), TransportError> {
        self.messages.lock().unwrap().push(msg.clone());
        Ok(())
    }

    fn broadcast(&self, msg: &MpcMessage) -> Result<(), TransportError> {
        self.messages.lock().unwrap().push(msg.clone());
        Ok(())
    }

    fn receive(&self, session_id: Uuid, round: u32, _timeout_ms: u64) -> Result<Vec<MpcMessage>, TransportError> {
        let msgs = self.messages.lock().unwrap();
        Ok(msgs.iter()
            .filter(|m| m.session_id == session_id && m.round == round)
            .cloned()
            .collect())
    }
}

/// Compute HMAC for MPC message authentication.
pub fn sign_message(secret: &[u8], msg: &MpcMessage) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret).unwrap();
    mac.update(msg.session_id.as_bytes());
    mac.update(&msg.round.to_le_bytes());
    mac.update(&msg.from_party.to_le_bytes());
    mac.update(&msg.payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Verify HMAC on received MPC message.
pub fn verify_message(secret: &[u8], msg: &MpcMessage) -> bool {
    let expected = sign_message(secret, msg);
    expected == msg.hmac
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session_config() -> MpcSessionConfig {
        MpcSessionConfig {
            session_id: Uuid::new_v4(),
            session_type: MpcSessionType::EcdsaKeygen,
            threshold: 2, total_parties: 3,
            parties: vec![
                MpcParty { id: Uuid::new_v4(), index: 0, endpoint: "localhost:9001".into(), public_key: "pk0".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
                MpcParty { id: Uuid::new_v4(), index: 1, endpoint: "localhost:9002".into(), public_key: "pk1".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
                MpcParty { id: Uuid::new_v4(), index: 2, endpoint: "localhost:9003".into(), public_key: "pk2".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
            ],
            timeout_secs: 30,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn in_memory_transport_send_receive() {
        let transport = InMemoryTransport::new();
        let session_id = Uuid::new_v4();

        let msg = MpcMessage {
            session_id, round: 1, from_party: 0, to_party: Some(1),
            payload: vec![0xAA, 0xBB], hmac: String::new(), timestamp: Utc::now(),
        };

        transport.send(&msg).unwrap();
        let received = transport.receive(session_id, 1, 1000).unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].payload, vec![0xAA, 0xBB]);
    }

    #[test]
    fn in_memory_transport_broadcast() {
        let transport = InMemoryTransport::new();
        let session_id = Uuid::new_v4();

        let msg = MpcMessage {
            session_id, round: 1, from_party: 0, to_party: None,
            payload: vec![0xCC], hmac: String::new(), timestamp: Utc::now(),
        };

        transport.broadcast(&msg).unwrap();
        assert_eq!(transport.message_count(), 1);
    }

    #[test]
    fn receive_filters_by_session_and_round() {
        let transport = InMemoryTransport::new();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        transport.send(&MpcMessage { session_id: s1, round: 1, from_party: 0, to_party: None, payload: vec![1], hmac: String::new(), timestamp: Utc::now() }).unwrap();
        transport.send(&MpcMessage { session_id: s1, round: 2, from_party: 0, to_party: None, payload: vec![2], hmac: String::new(), timestamp: Utc::now() }).unwrap();
        transport.send(&MpcMessage { session_id: s2, round: 1, from_party: 0, to_party: None, payload: vec![3], hmac: String::new(), timestamp: Utc::now() }).unwrap();

        let r = transport.receive(s1, 1, 100).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload, vec![1]);
    }

    #[test]
    fn message_hmac_sign_verify() {
        let secret = b"mpc_shared_secret";
        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(), round: 1, from_party: 0, to_party: Some(1),
            payload: vec![0xDE, 0xAD], hmac: String::new(), timestamp: Utc::now(),
        };
        msg.hmac = sign_message(secret, &msg);
        assert!(verify_message(secret, &msg));
    }

    #[test]
    fn message_hmac_wrong_secret() {
        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(), round: 1, from_party: 0, to_party: None,
            payload: vec![0xFF], hmac: String::new(), timestamp: Utc::now(),
        };
        msg.hmac = sign_message(b"correct", &msg);
        assert!(!verify_message(b"wrong", &msg));
    }

    #[test]
    fn message_hmac_tamper_detected() {
        let secret = b"secret";
        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(), round: 1, from_party: 0, to_party: None,
            payload: vec![0xAA], hmac: String::new(), timestamp: Utc::now(),
        };
        msg.hmac = sign_message(secret, &msg);
        msg.payload = vec![0xBB]; // tamper
        assert!(!verify_message(secret, &msg));
    }

    #[test]
    fn session_config_2_of_3() {
        let config = test_session_config();
        assert_eq!(config.threshold, 2);
        assert_eq!(config.total_parties, 3);
        assert_eq!(config.parties.len(), 3);
    }

    #[test]
    fn session_type_serialization() {
        assert_eq!(serde_json::to_string(&MpcSessionType::EcdsaKeygen).unwrap(), r#""ecdsa_keygen""#);
        assert_eq!(serde_json::to_string(&MpcSessionType::FrostSign).unwrap(), r#""frost_sign""#);
        assert_eq!(serde_json::to_string(&MpcSessionType::KeyRotation).unwrap(), r#""key_rotation""#);
    }

    #[test]
    fn transport_error_display() {
        assert!(TransportError::Timeout.to_string().contains("timeout"));
        assert!(TransportError::PartyOffline(2).to_string().contains("2"));
    }
}
