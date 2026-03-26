//! Distributed DKG execution — runs DKG over MpcTransport.
//!
//! This connects the DKG protocol to the network transport layer,
//! enabling real multi-party key generation across independent processes.

use anyhow::Result;
use uuid::Uuid;

use crate::transport::{MpcMessage, InMemoryTransport};
use crate::coordinator_v2::{broadcast_round, collect_round};
use crate::dkg::DkgConfig;

/// Run a simulated distributed DKG using InMemoryTransport.
///
/// In production, this would use a real network transport (gRPC/noise).
/// Each party would be a separate process on a separate host.
pub fn run_simulated_dkg(
    config: &DkgConfig,
    secret: &[u8],
) -> Result<SimulatedDkgResult> {
    let transport = InMemoryTransport::new();

    // Simulate round 1: each party broadcasts commitment
    for party_idx in 0..config.parties {
        let commitment = generate_commitment(config.session_id, party_idx);
        broadcast_round(
            &transport, config.session_id, 1, party_idx,
            commitment, secret,
        )?;
    }

    // Collect round 1 messages
    let round1_msgs = collect_round(
        &transport, config.session_id, 1,
        config.parties as usize, 5000,
    )?;

    // Simulate round 2: each party broadcasts share
    for party_idx in 0..config.parties {
        let share_data = generate_share(config.session_id, party_idx, &round1_msgs);
        broadcast_round(
            &transport, config.session_id, 2, party_idx,
            share_data, secret,
        )?;
    }

    // Collect round 2 messages
    let round2_msgs = collect_round(
        &transport, config.session_id, 2,
        config.parties as usize, 5000,
    )?;

    // Derive the group public key from all commitments
    let group_public_key = derive_group_key(&round1_msgs);

    Ok(SimulatedDkgResult {
        session_id: config.session_id,
        threshold: config.threshold,
        parties: config.parties,
        group_public_key,
        rounds_completed: 2,
        messages_exchanged: round1_msgs.len() + round2_msgs.len(),
    })
}

/// Result of a simulated DKG run.
#[derive(Debug, Clone)]
pub struct SimulatedDkgResult {
    pub session_id: Uuid,
    pub threshold: u16,
    pub parties: u16,
    pub group_public_key: Vec<u8>,
    pub rounds_completed: u32,
    pub messages_exchanged: usize,
}

// --- Simulation helpers (would be real crypto in production) ---

fn generate_commitment(session_id: Uuid, party_idx: u16) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(&party_idx.to_le_bytes());
    hasher.update(b"commitment");
    hasher.finalize().to_vec()
}

fn generate_share(session_id: Uuid, party_idx: u16, _round1: &[MpcMessage]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(&party_idx.to_le_bytes());
    hasher.update(b"share");
    hasher.finalize().to_vec()
}

fn derive_group_key(commitments: &[MpcMessage]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for msg in commitments {
        hasher.update(&msg.payload);
    }
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::dkg::DkgConfig;

    #[test]
    fn simulated_dkg_2_of_3() {
        let config = DkgConfig::new(2, 3);
        let result = run_simulated_dkg(&config, b"test_secret").unwrap();

        assert_eq!(result.threshold, 2);
        assert_eq!(result.parties, 3);
        assert_eq!(result.rounds_completed, 2);
        assert_eq!(result.messages_exchanged, 6); // 3 round1 + 3 round2
        assert_eq!(result.group_public_key.len(), 32); // SHA-256
    }

    #[test]
    fn simulated_dkg_3_of_5() {
        let config = DkgConfig::new(3, 5);
        let result = run_simulated_dkg(&config, b"secret").unwrap();

        assert_eq!(result.threshold, 3);
        assert_eq!(result.parties, 5);
        assert_eq!(result.messages_exchanged, 10); // 5 + 5
    }

    #[test]
    fn simulated_dkg_different_sessions_different_keys() {
        let c1 = DkgConfig::new(2, 3); // new() generates random session_id
        let c2 = DkgConfig::new(2, 3);
        let r1 = run_simulated_dkg(&c1, b"s").unwrap();
        let r2 = run_simulated_dkg(&c2, b"s").unwrap();
        // Different session IDs → different keys
        assert_ne!(r1.group_public_key, r2.group_public_key);
    }

    #[test]
    fn commitment_unique_per_party() {
        let sid = Uuid::new_v4();
        let c0 = generate_commitment(sid, 0);
        let c1 = generate_commitment(sid, 1);
        assert_ne!(c0, c1);
    }

    #[test]
    fn group_key_depends_on_all_commitments() {
        let sid = Uuid::new_v4();
        let msg1 = MpcMessage {
            session_id: sid, round: 1, from_party: 0, to_party: None,
            payload: vec![0xAA], hmac: String::new(), timestamp: Utc::now(),
        };
        let msg2 = MpcMessage {
            session_id: sid, round: 1, from_party: 1, to_party: None,
            payload: vec![0xBB], hmac: String::new(), timestamp: Utc::now(),
        };

        let key_both = derive_group_key(&[msg1.clone(), msg2.clone()]);
        let key_one = derive_group_key(&[msg1]);
        assert_ne!(key_both, key_one);
    }
}
