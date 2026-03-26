//! IPC protocol — request/response types between custody and custody-signer.
//!
//! All messages are HMAC-authenticated to prevent IPC socket hijacking.

use custody_common::Chain;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A signing request from the main custody process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequest {
    pub request_id: Uuid,
    pub chain: Chain,
    pub operation: SignOperation,
    pub hmac: String,
}

/// Types of signing operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignOperation {
    /// Sign an Ethereum transaction.
    EthTransaction {
        derivation_index: u32,
        unsigned_tx: String, // hex-encoded
    },
    /// Sign a Bitcoin transaction.
    BtcTransaction {
        derivation_index: u32,
        unsigned_tx: String,
    },
    /// Sign a Tron transaction.
    TrxTransaction {
        derivation_index: u32,
        unsigned_tx: String,
    },
    /// Sign a Solana transaction.
    SolTransaction {
        derivation_index: u32,
        unsigned_tx: String,
    },
    /// Raw message signing (EIP-191, EIP-712, etc).
    RawSign {
        derivation_index: u32,
        message_hash: String, // hex-encoded 32-byte hash
    },
    /// Get public key / address for a derivation index.
    GetPublicKey {
        derivation_index: u32,
    },
}

/// Response from the signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignResponse {
    pub request_id: Uuid,
    pub result: SignResult,
    pub hmac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SignResult {
    /// Signing succeeded.
    Success {
        signed_data: String, // hex-encoded
        public_key: Option<String>,
    },
    /// Signing failed.
    Error {
        code: u32,
        message: String,
    },
}

/// Compute HMAC for a request (prevents IPC socket hijacking).
pub fn compute_hmac(secret: &[u8], request_id: &Uuid, operation_json: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret).unwrap();
    mac.update(request_id.as_bytes());
    mac.update(operation_json.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify HMAC on a request.
pub fn verify_hmac(secret: &[u8], request_id: &Uuid, operation_json: &str, claimed_hmac: &str) -> bool {
    let expected = compute_hmac(secret, request_id, operation_json);
    // Constant-time comparison
    expected.len() == claimed_hmac.len()
        && expected.as_bytes().iter().zip(claimed_hmac.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_roundtrip() {
        let secret = b"test_secret";
        let id = Uuid::new_v4();
        let op = r#"{"type":"get_public_key","derivation_index":0}"#;

        let mac = compute_hmac(secret, &id, op);
        assert!(verify_hmac(secret, &id, op, &mac));
    }

    #[test]
    fn hmac_wrong_secret_fails() {
        let id = Uuid::new_v4();
        let op = r#"{"type":"raw_sign","derivation_index":0,"message_hash":"aa"}"#;

        let mac = compute_hmac(b"correct", &id, op);
        assert!(!verify_hmac(b"wrong", &id, op, &mac));
    }

    #[test]
    fn hmac_tampered_operation_fails() {
        let secret = b"secret";
        let id = Uuid::new_v4();
        let mac = compute_hmac(secret, &id, "original");
        assert!(!verify_hmac(secret, &id, "tampered", &mac));
    }

    #[test]
    fn sign_request_serialization() {
        let req = SignRequest {
            request_id: Uuid::nil(),
            chain: Chain::Ethereum,
            operation: SignOperation::GetPublicKey { derivation_index: 0 },
            hmac: "abc123".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("get_public_key"));
        assert!(json.contains("ethereum"));
    }

    #[test]
    fn sign_response_success() {
        let resp = SignResponse {
            request_id: Uuid::nil(),
            result: SignResult::Success {
                signed_data: "0xdeadbeef".into(),
                public_key: Some("0x02aabb".into()),
            },
            hmac: "sig".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("0xdeadbeef"));
    }

    #[test]
    fn sign_response_error() {
        let resp = SignResponse {
            request_id: Uuid::nil(),
            result: SignResult::Error {
                code: 5001,
                message: "key derivation failed".into(),
            },
            hmac: "sig".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("5001"));
    }

    #[test]
    fn all_sign_operations_serialize() {
        let ops = vec![
            SignOperation::EthTransaction { derivation_index: 0, unsigned_tx: "0x".into() },
            SignOperation::BtcTransaction { derivation_index: 0, unsigned_tx: "0x".into() },
            SignOperation::TrxTransaction { derivation_index: 0, unsigned_tx: "0x".into() },
            SignOperation::SolTransaction { derivation_index: 0, unsigned_tx: "0x".into() },
            SignOperation::RawSign { derivation_index: 0, message_hash: "0x".into() },
            SignOperation::GetPublicKey { derivation_index: 0 },
        ];
        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            assert!(!json.is_empty());
        }
    }
}
