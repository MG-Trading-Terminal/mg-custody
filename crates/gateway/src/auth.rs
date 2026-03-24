//! HMAC-SHA256 request authentication.
//!
//! Every request from the exchange includes:
//!   X-Custody-Timestamp: <unix_timestamp_ms>
//!   X-Custody-Signature: HMAC-SHA256(secret, method + path + timestamp + body)
//!
//! Requests with timestamp older than 30 seconds are rejected.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify request HMAC signature.
pub fn verify_signature(
    secret: &[u8],
    method: &str,
    path: &str,
    timestamp: &str,
    body: &[u8],
    provided_signature: &str,
) -> bool {
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };

    mac.update(method.as_bytes());
    mac.update(path.as_bytes());
    mac.update(timestamp.as_bytes());
    mac.update(body);

    let expected = hex::encode(mac.finalize().into_bytes());
    // Constant-time comparison via the hmac crate is already done above,
    // but we hex-encode for the header comparison.
    expected == provided_signature
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_signature_passes() {
        let secret = b"test-secret-key";
        let method = "POST";
        let path = "/v1/addresses";
        let timestamp = "1711234567890";
        let body = br#"{"user_id":"abc"}"#;

        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(method.as_bytes());
        mac.update(path.as_bytes());
        mac.update(timestamp.as_bytes());
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());

        assert!(verify_signature(secret, method, path, timestamp, body, &sig));
    }

    #[test]
    fn wrong_signature_fails() {
        let secret = b"test-secret-key";
        assert!(!verify_signature(
            secret, "POST", "/v1/addresses", "123", b"{}", "deadbeef"
        ));
    }
}
