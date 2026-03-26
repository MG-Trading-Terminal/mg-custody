//! FIDO2/WebAuthn types for hardware key authentication.
//!
//! Used for high-value withdrawal approvals (Tier 2+).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A registered FIDO2 credential (hardware security key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fido2Credential {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: String,
    pub public_key: String,
    pub sign_count: u32,
    pub aaguid: Option<String>,
    pub device_name: Option<String>,
    pub registered_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// WebAuthn registration (attestation) options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationOptions {
    pub challenge: String,
    pub rp_id: String,
    pub rp_name: String,
    pub user_id: String,
    pub user_name: String,
    pub user_display_name: String,
    pub attestation: AttestationType,
    pub authenticator_selection: AuthenticatorSelection,
    pub timeout_ms: u32,
}

/// WebAuthn authentication (assertion) options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationOptions {
    pub challenge: String,
    pub rp_id: String,
    pub allowed_credentials: Vec<AllowedCredential>,
    pub timeout_ms: u32,
    pub user_verification: UserVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowedCredential {
    pub credential_id: String,
    #[serde(rename = "type")]
    pub cred_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationType {
    None,
    Direct,
    Indirect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatorSelection {
    pub authenticator_attachment: Option<String>,
    pub resident_key: Option<String>,
    pub user_verification: UserVerification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserVerification {
    Required,
    Preferred,
    Discouraged,
}

/// Registration response from the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub credential_id: String,
    pub attestation_object: String,
    pub client_data_json: String,
}

/// Authentication response from the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticationResponse {
    pub credential_id: String,
    pub authenticator_data: String,
    pub client_data_json: String,
    pub signature: String,
}

/// Generate a random challenge (32 bytes, hex-encoded).
pub fn generate_challenge() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

/// Default registration options for custody approvals.
pub fn default_registration_options(user_id: &str, user_name: &str) -> RegistrationOptions {
    RegistrationOptions {
        challenge: generate_challenge(),
        rp_id: "custody.local".into(),
        rp_name: "MG Custody".into(),
        user_id: user_id.into(),
        user_name: user_name.into(),
        user_display_name: user_name.into(),
        attestation: AttestationType::Direct,
        authenticator_selection: AuthenticatorSelection {
            authenticator_attachment: Some("cross-platform".into()),
            resident_key: Some("preferred".into()),
            user_verification: UserVerification::Required,
        },
        timeout_ms: 60_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_challenge_length() {
        let challenge = generate_challenge();
        assert_eq!(challenge.len(), 64); // 32 bytes = 64 hex
        assert!(challenge.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn challenge_unique() {
        let c1 = generate_challenge();
        let c2 = generate_challenge();
        assert_ne!(c1, c2);
    }

    #[test]
    fn default_registration() {
        let opts = default_registration_options("user-123", "alice@custody.com");
        assert_eq!(opts.rp_name, "MG Custody");
        assert_eq!(opts.attestation, AttestationType::Direct);
        assert_eq!(opts.authenticator_selection.user_verification, UserVerification::Required);
        assert_eq!(opts.timeout_ms, 60_000);
    }

    #[test]
    fn credential_serialization() {
        let cred = Fido2Credential {
            id: Uuid::nil(), user_id: Uuid::nil(),
            credential_id: "cred-abc123".into(),
            public_key: "pk-xyz".into(),
            sign_count: 5,
            aaguid: Some("aaguid-123".into()),
            device_name: Some("YubiKey 5".into()),
            registered_at: Utc::now(),
            last_used_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&cred).unwrap();
        assert!(json.contains("YubiKey 5"));
        assert!(json.contains("sign_count"));
    }

    #[test]
    fn attestation_type_serialization() {
        assert_eq!(serde_json::to_string(&AttestationType::Direct).unwrap(), r#""direct""#);
        assert_eq!(serde_json::to_string(&AttestationType::None).unwrap(), r#""none""#);
    }

    #[test]
    fn user_verification_serialization() {
        assert_eq!(serde_json::to_string(&UserVerification::Required).unwrap(), r#""required""#);
    }
}
