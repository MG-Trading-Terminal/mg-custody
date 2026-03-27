//! HTTP-based MPC transport for real network communication.
//!
//! Each MPC party runs an HTTP endpoint. Messages are sent via POST
//! with HMAC-SHA256 authentication on every message.
//!
//! Architecture:
//! - Each party has a known endpoint (e.g., "http://signer-1:9100/mpc")
//! - `send()` POSTs JSON-serialized MpcMessage to the target party
//! - `broadcast()` POSTs to all other parties
//! - `receive()` reads from a local inbox (populated by companion receiver)
//! - All messages are HMAC-signed before sending, verified on receive

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::transport::{
    MpcMessage, MpcParty, MpcTransport, TransportError,
    sign_message, verify_message,
};

/// Maximum message payload size (1 MB).
const MAX_MESSAGE_SIZE: usize = 1_048_576;

/// HTTP-based transport for MPC inter-party communication.
///
/// Each party creates one `HttpTransport` instance. Messages are sent
/// via HTTP POST to other parties' endpoints and received into a shared inbox.
pub struct HttpTransport {
    /// This party's index in the session.
    my_party_index: u16,
    /// All parties in the session (including self).
    parties: Vec<MpcParty>,
    /// Shared HMAC secret for message authentication.
    hmac_secret: Vec<u8>,
    /// Local inbox for received messages.
    inbox: Arc<Mutex<Vec<MpcMessage>>>,
    /// HTTP client for sending messages.
    client: reqwest::Client,
}

impl HttpTransport {
    /// Create a new HTTP transport for a party.
    ///
    /// - `my_party_index`: this party's index
    /// - `parties`: all parties with their HTTP endpoints
    /// - `hmac_secret`: shared secret for HMAC authentication
    pub fn new(
        my_party_index: u16,
        parties: Vec<MpcParty>,
        hmac_secret: Vec<u8>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            my_party_index,
            parties,
            hmac_secret,
            inbox: Arc::new(Mutex::new(Vec::new())),
            client,
        }
    }

    /// Get a reference to the inbox for use by a receiver handler.
    ///
    /// The companion HTTP server pushes received messages into this inbox.
    pub fn inbox(&self) -> Arc<Mutex<Vec<MpcMessage>>> {
        Arc::clone(&self.inbox)
    }

    /// Deliver a message into this transport's inbox.
    /// Called by the receiver HTTP handler when a message arrives.
    ///
    /// Returns `Err` if HMAC verification fails.
    pub fn deliver(&self, msg: MpcMessage) -> Result<(), TransportError> {
        // Verify HMAC
        if !verify_message(&self.hmac_secret, &msg) {
            tracing::warn!(
                from = msg.from_party,
                session = %msg.session_id,
                "Rejected MPC message: HMAC verification failed"
            );
            return Err(TransportError::AuthenticationFailed);
        }

        // Check payload size
        if msg.payload.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge);
        }

        self.inbox.lock().unwrap().push(msg);
        Ok(())
    }

    /// Send a signed message to a specific party via HTTP POST.
    async fn send_to_party(&self, party: &MpcParty, msg: &MpcMessage) -> Result<(), TransportError> {
        let url = format!("{}/mpc/message", party.endpoint);

        let resp = self.client
            .post(&url)
            .json(msg)
            .send()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("{}: {}", party.endpoint, e)))?;

        if !resp.status().is_success() {
            return Err(TransportError::ConnectionFailed(format!(
                "{}: HTTP {}",
                party.endpoint,
                resp.status()
            )));
        }

        Ok(())
    }

    /// Number of messages in inbox.
    pub fn inbox_count(&self) -> usize {
        self.inbox.lock().unwrap().len()
    }
}

impl MpcTransport for HttpTransport {
    fn send(&self, msg: &MpcMessage) -> Result<(), TransportError> {
        let to_party = msg.to_party.ok_or(TransportError::ConnectionFailed(
            "send() requires to_party, use broadcast() for all".into(),
        ))?;

        let party = self.parties.iter()
            .find(|p| p.index == to_party)
            .ok_or(TransportError::PartyOffline(to_party))?;

        // Sign the message
        let mut signed_msg = msg.clone();
        signed_msg.hmac = sign_message(&self.hmac_secret, &signed_msg);

        // Use tokio runtime to execute async send
        let party = party.clone();
        let client = self.client.clone();
        let url = format!("{}/mpc/message", party.endpoint);

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                client
                    .post(&url)
                    .json(&signed_msg)
                    .send()
                    .await
                    .map_err(|e| TransportError::ConnectionFailed(format!("{}: {}", party.endpoint, e)))?;
                Ok(())
            })
        })
    }

    fn broadcast(&self, msg: &MpcMessage) -> Result<(), TransportError> {
        let mut signed_msg = msg.clone();
        signed_msg.hmac = sign_message(&self.hmac_secret, &signed_msg);

        let other_parties: Vec<MpcParty> = self.parties.iter()
            .filter(|p| p.index != self.my_party_index)
            .cloned()
            .collect();

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                for party in &other_parties {
                    if let Err(e) = self.send_to_party(party, &signed_msg).await {
                        tracing::warn!(
                            party = party.index,
                            error = %e,
                            "Failed to send to party"
                        );
                        // Continue sending to other parties
                    }
                }
                Ok(())
            })
        })
    }

    fn receive(
        &self,
        session_id: Uuid,
        round: u32,
        timeout_ms: u64,
    ) -> Result<Vec<MpcMessage>, TransportError> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let expected_count = self.parties.len() - 1; // messages from all other parties

        loop {
            let msgs: Vec<MpcMessage> = {
                let inbox = self.inbox.lock().unwrap();
                inbox.iter()
                    .filter(|m| m.session_id == session_id && m.round == round)
                    .cloned()
                    .collect()
            };

            // Return early if we have all expected messages
            if msgs.len() >= expected_count {
                return Ok(msgs);
            }

            // Return what we have when deadline is reached
            if Instant::now() >= deadline {
                return Ok(msgs);
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Create an inbox delivery function for use in an HTTP handler.
///
/// Returns a closure that accepts JSON-serialized `MpcMessage` and delivers
/// it to the transport's inbox after HMAC verification.
///
/// Example axum handler:
/// ```ignore
/// let inbox = transport.inbox();
/// let secret = hmac_secret.clone();
///
/// async fn mpc_handler(
///     State((inbox, secret)): State<(Arc<Mutex<Vec<MpcMessage>>>, Vec<u8>)>,
///     Json(msg): Json<MpcMessage>,
/// ) -> StatusCode {
///     if !verify_message(&secret, &msg) {
///         return StatusCode::UNAUTHORIZED;
///     }
///     inbox.lock().unwrap().push(msg);
///     StatusCode::OK
/// }
/// ```
pub fn deliver_to_inbox(
    inbox: &Arc<Mutex<Vec<MpcMessage>>>,
    hmac_secret: &[u8],
    msg: MpcMessage,
) -> Result<(), TransportError> {
    if !verify_message(hmac_secret, &msg) {
        return Err(TransportError::AuthenticationFailed);
    }
    if msg.payload.len() > MAX_MESSAGE_SIZE {
        return Err(TransportError::MessageTooLarge);
    }
    inbox.lock().unwrap().push(msg);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::sign_message;
    use chrono::Utc;

    fn test_parties() -> Vec<MpcParty> {
        vec![
            MpcParty { id: Uuid::new_v4(), index: 0, endpoint: "http://localhost:9100".into(), public_key: "pk0".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
            MpcParty { id: Uuid::new_v4(), index: 1, endpoint: "http://localhost:9101".into(), public_key: "pk1".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
            MpcParty { id: Uuid::new_v4(), index: 2, endpoint: "http://localhost:9102".into(), public_key: "pk2".into(), is_online: true, last_heartbeat: Some(Utc::now()) },
        ]
    }

    #[test]
    fn http_transport_creation() {
        let transport = HttpTransport::new(0, test_parties(), b"secret".to_vec());
        assert_eq!(transport.my_party_index, 0);
        assert_eq!(transport.parties.len(), 3);
        assert_eq!(transport.inbox_count(), 0);
    }

    #[test]
    fn inbox_delivery_with_valid_hmac() {
        let secret = b"shared_secret".to_vec();
        let transport = HttpTransport::new(1, test_parties(), secret.clone());
        let session_id = Uuid::new_v4();

        let mut msg = MpcMessage {
            session_id,
            round: 1,
            from_party: 0,
            to_party: Some(1),
            payload: vec![0xAA, 0xBB, 0xCC],
            hmac: String::new(),
            timestamp: Utc::now(),
        };
        msg.hmac = sign_message(&secret, &msg);

        // Delivery should succeed
        transport.deliver(msg).unwrap();
        assert_eq!(transport.inbox_count(), 1);

        // Receive should return the message
        let received = transport.receive(session_id, 1, 100).unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].payload, vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn inbox_delivery_rejects_bad_hmac() {
        let transport = HttpTransport::new(1, test_parties(), b"secret".to_vec());

        let msg = MpcMessage {
            session_id: Uuid::new_v4(),
            round: 1,
            from_party: 0,
            to_party: Some(1),
            payload: vec![0xDE, 0xAD],
            hmac: "bad_hmac".to_string(),
            timestamp: Utc::now(),
        };

        let result = transport.deliver(msg);
        assert!(matches!(result, Err(TransportError::AuthenticationFailed)));
        assert_eq!(transport.inbox_count(), 0);
    }

    #[test]
    fn inbox_delivery_rejects_oversized_message() {
        let secret = b"secret".to_vec();
        let transport = HttpTransport::new(1, test_parties(), secret.clone());

        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(),
            round: 1,
            from_party: 0,
            to_party: Some(1),
            payload: vec![0u8; MAX_MESSAGE_SIZE + 1],
            hmac: String::new(),
            timestamp: Utc::now(),
        };
        msg.hmac = sign_message(&secret, &msg);

        let result = transport.deliver(msg);
        assert!(matches!(result, Err(TransportError::MessageTooLarge)));
    }

    #[test]
    fn receive_filters_by_session_and_round() {
        let secret = b"secret".to_vec();
        let transport = HttpTransport::new(1, test_parties(), secret.clone());
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();

        // Deliver messages from different sessions/rounds
        for (sid, round, payload) in [(s1, 1, vec![1u8]), (s1, 2, vec![2]), (s2, 1, vec![3])] {
            let mut msg = MpcMessage {
                session_id: sid, round, from_party: 0, to_party: Some(1),
                payload, hmac: String::new(), timestamp: Utc::now(),
            };
            msg.hmac = sign_message(&secret, &msg);
            transport.deliver(msg).unwrap();
        }

        let r = transport.receive(s1, 1, 100).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload, vec![1]);

        let r = transport.receive(s1, 2, 100).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload, vec![2]);

        let r = transport.receive(s2, 1, 100).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload, vec![3]);
    }

    #[test]
    fn deliver_to_inbox_function() {
        let secret = b"key".to_vec();
        let inbox = Arc::new(Mutex::new(Vec::new()));

        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(), round: 1, from_party: 2, to_party: Some(0),
            payload: vec![0xFF], hmac: String::new(), timestamp: Utc::now(),
        };
        msg.hmac = sign_message(&secret, &msg);

        deliver_to_inbox(&inbox, &secret, msg).unwrap();
        assert_eq!(inbox.lock().unwrap().len(), 1);
    }

    #[test]
    fn deliver_to_inbox_rejects_tampered() {
        let secret = b"key".to_vec();
        let inbox = Arc::new(Mutex::new(Vec::new()));

        let mut msg = MpcMessage {
            session_id: Uuid::new_v4(), round: 1, from_party: 2, to_party: Some(0),
            payload: vec![0xAA], hmac: String::new(), timestamp: Utc::now(),
        };
        msg.hmac = sign_message(&secret, &msg);
        msg.payload = vec![0xBB]; // tamper after signing

        let result = deliver_to_inbox(&inbox, &secret, msg);
        assert!(matches!(result, Err(TransportError::AuthenticationFailed)));
        assert_eq!(inbox.lock().unwrap().len(), 0);
    }

    #[test]
    fn multiple_parties_deliver_to_same_inbox() {
        let secret = b"shared".to_vec();
        let transport = HttpTransport::new(0, test_parties(), secret.clone());
        let session_id = Uuid::new_v4();

        // Party 1 and Party 2 both send to Party 0
        for from_party in [1u16, 2] {
            let mut msg = MpcMessage {
                session_id, round: 1, from_party, to_party: Some(0),
                payload: vec![from_party as u8], hmac: String::new(), timestamp: Utc::now(),
            };
            msg.hmac = sign_message(&secret, &msg);
            transport.deliver(msg).unwrap();
        }

        let received = transport.receive(session_id, 1, 100).unwrap();
        assert_eq!(received.len(), 2);
        // Both parties' messages should be present
        let senders: Vec<u16> = received.iter().map(|m| m.from_party).collect();
        assert!(senders.contains(&1));
        assert!(senders.contains(&2));
    }
}
