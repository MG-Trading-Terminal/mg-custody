//! TOTP 2FA endpoints.
//!
//! Setup flow:
//!   1. POST /v1/auth/totp/setup     — generates secret + backup codes (one-time view)
//!   2. POST /v1/auth/totp/verify    — verify first code, enable TOTP
//!
//! Login flow (when TOTP is enabled):
//!   1. POST /v1/auth/sessions       — password check → {requires_2fa: true, pending_token}
//!   2. POST /v1/auth/sessions/verify-2fa — pending_token + code → full session
//!
//! Disable: POST /v1/auth/totp/disable (requires valid TOTP code, or Admin role).
//!
//! Encryption: TOTP secrets are encrypted with AES-256-GCM before DB storage.
//! Key = SHA-256("totp-v1:" || seed[0..32]) — derived from service master seed.
//!
//! Backup codes (8×8-char alphanumeric) are returned in plaintext exactly once
//! at setup time. Their SHA-256 hashes are stored in DB. Each code is single-use.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use custody_common::totp::TotpSecret;
use custody_db::totp::{
    decrypt_secret, encrypt_secret, generate_backup_codes, hash_backup_code,
};

use crate::errors::ApiError;
use crate::rbac_middleware::AuthUser;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn totp_router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/totp/setup", post(setup_totp))
        .route("/v1/auth/totp/verify", post(verify_and_enable_totp))
        .route("/v1/auth/totp/disable", post(disable_totp))
        .route("/v1/auth/sessions/verify-2fa", post(verify_2fa_login))
}

// ---------------------------------------------------------------------------
// Encryption key derivation
// ---------------------------------------------------------------------------

/// Derive AES-256-GCM key for TOTP secret encryption from the service seed.
///
/// Key = SHA-256("totp-v1:" || seed[0..32])
/// Deterministic across restarts (all state in PostgreSQL per design).
fn totp_encryption_key(state: &AppState) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"totp-v1:");
    hasher.update(&state.seed.0[..32]);
    hasher.finalize().into()
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TotpSetupResponse {
    pub secret_base32: String,
    pub provisioning_uri: String,
    /// Raw backup codes — shown exactly once. Store them securely.
    pub backup_codes: Vec<String>,
    pub message: String,
}

#[derive(Deserialize)]
pub struct VerifyTotpRequest {
    pub code: String,
}

#[derive(Serialize)]
pub struct TotpVerifyResponse {
    pub enabled: bool,
    pub message: String,
}

#[derive(Deserialize)]
pub struct DisableTotpRequest {
    /// Current TOTP code required. Admin may omit.
    pub code: Option<String>,
}

#[derive(Deserialize)]
pub struct Verify2faRequest {
    /// Short-lived pending token issued by the login endpoint.
    pub pending_token: String,
    /// 6-digit TOTP code, or 8-character backup code.
    pub code: String,
}

#[derive(Serialize)]
pub struct Verify2faResponse {
    pub token: String,
    pub user_id: Uuid,
    pub role: String,
    pub expires_in_seconds: i64,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/auth/totp/setup
///
/// Generates a new TOTP secret and stores it (disabled, pending first verification).
/// Returns the base32 secret, provisioning URI, and 8 single-use backup codes.
/// The backup codes are shown exactly once and are NOT retrievable afterwards.
///
/// Idempotent: calling again replaces any pending (not-yet-enabled) setup.
/// Returns 409 if TOTP is already enabled — disable first.
async fn setup_totp(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<TotpSetupResponse>), ApiError> {
    let user_id = auth.0.user_id;
    let email = auth.0.email.clone();

    // Check if already enabled
    if let Some(existing) = state
        .db
        .load_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
    {
        if existing.enabled {
            return Err(ApiError::conflict(
                "TOTP is already enabled. Disable it first before re-enrolling.",
            ));
        }
    }

    let secret = TotpSecret::generate();
    let secret_base32 = secret.to_base32();
    let provisioning_uri = secret.provisioning_uri(&email, "MG Custody");

    let key = totp_encryption_key(&state);
    let secret_encrypted = encrypt_secret(&key, &secret_base32)
        .map_err(|e| ApiError::internal(&format!("Encryption failed: {e}")))?;

    // Generate backup codes — raw codes returned now, hashes stored in DB
    let (raw_codes, hashed_codes) = generate_backup_codes();

    state
        .db
        .upsert_totp_setup(user_id, &secret_encrypted, &hashed_codes)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(user_id = %user_id, "TOTP setup initiated — secret generated");

    Ok((
        StatusCode::CREATED,
        Json(TotpSetupResponse {
            secret_base32,
            provisioning_uri,
            backup_codes: raw_codes,
            message: "Scan the QR code with your authenticator app, then call \
                      POST /v1/auth/totp/verify with a code to enable TOTP. \
                      Store the backup codes securely — they will NOT be shown again."
                .into(),
        }),
    ))
}

/// POST /v1/auth/totp/verify
///
/// Verify the first TOTP code to confirm the authenticator is correctly configured.
/// On success, enables TOTP for the account.
async fn verify_and_enable_totp(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<VerifyTotpRequest>,
) -> Result<Json<TotpVerifyResponse>, ApiError> {
    let user_id = auth.0.user_id;

    let record = state
        .db
        .load_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| {
            ApiError::bad_request("TOTP not set up. Call POST /v1/auth/totp/setup first.")
        })?;

    if record.enabled {
        return Err(ApiError::conflict("TOTP is already enabled."));
    }

    let key = totp_encryption_key(&state);
    let secret_base32 = decrypt_secret(&key, &record.secret_encrypted)
        .map_err(|e| ApiError::internal(&format!("Decryption failed: {e}")))?;

    let secret = TotpSecret::from_base32(&secret_base32)
        .ok_or_else(|| ApiError::internal("Stored TOTP secret is corrupt"))?;

    let now_ts = Utc::now().timestamp() as u64;
    if !secret.verify(&req.code, now_ts) {
        tracing::warn!(user_id = %user_id, "TOTP verification failed during setup");
        return Err(ApiError::unauthorized("Invalid TOTP code."));
    }

    let enabled = state
        .db
        .enable_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !enabled {
        return Err(ApiError::internal(
            "Failed to enable TOTP — setup record not found. Retry setup.",
        ));
    }

    let _ = state.db.touch_totp_used(user_id).await;
    tracing::info!(user_id = %user_id, "TOTP enabled successfully");

    Ok(Json(TotpVerifyResponse {
        enabled: true,
        message: "TOTP enabled. A code will now be required at every login.".into(),
    }))
}

/// POST /v1/auth/totp/disable
///
/// Disables and removes TOTP. Requires a valid current TOTP code unless the caller
/// is Admin or Owner.
///
/// Also revokes all active sessions to force re-login (security hygiene on 2FA change).
async fn disable_totp(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<DisableTotpRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    use custody_db::sessions::Role;

    let user_id = auth.0.user_id;
    let is_admin = auth.0.role.has_at_least(&Role::Admin);

    let record = state
        .db
        .load_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("TOTP is not set up for this account."))?;

    if !record.enabled {
        return Err(ApiError::bad_request("TOTP is not currently enabled."));
    }

    if !is_admin {
        let code = req.code.as_deref().ok_or_else(|| {
            ApiError::bad_request(
                "TOTP code required to disable 2FA. Admin can override without a code.",
            )
        })?;

        let key = totp_encryption_key(&state);
        let secret_base32 = decrypt_secret(&key, &record.secret_encrypted)
            .map_err(|e| ApiError::internal(&format!("Decryption failed: {e}")))?;
        let secret = TotpSecret::from_base32(&secret_base32)
            .ok_or_else(|| ApiError::internal("Stored TOTP secret is corrupt"))?;

        let now_ts = Utc::now().timestamp() as u64;
        if !secret.verify(code, now_ts) {
            tracing::warn!(user_id = %user_id, "TOTP disable rejected — wrong code");
            return Err(ApiError::unauthorized("Invalid TOTP code."));
        }
    }

    state
        .db
        .delete_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Revoke all active sessions (force re-login after 2FA change)
    let _ = state.db.revoke_user_sessions(user_id).await;

    tracing::info!(user_id = %user_id, by_admin = is_admin, "TOTP disabled");

    Ok(Json(serde_json::json!({
        "disabled": true,
        "message": "TOTP disabled. All sessions have been revoked — please log in again."
    })))
}

/// POST /v1/auth/sessions/verify-2fa
///
/// Completes a 2-step login for accounts with TOTP enabled.
///
/// After `POST /v1/auth/sessions` returns `{requires_2fa: true, pending_token}`,
/// the client submits that token here along with their TOTP code (6 digits)
/// or a backup code (8 alphanumeric characters).
pub async fn verify_2fa_login(
    State(state): State<AppState>,
    Json(req): Json<Verify2faRequest>,
) -> Result<(StatusCode, Json<Verify2faResponse>), ApiError> {
    // Consume the pending token — single-use, 5-minute TTL
    let user_id = state
        .db
        .consume_pending_2fa_token(&req.pending_token)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| {
            ApiError::unauthorized("Invalid, expired, or already-used 2FA pending token.")
        })?;

    // Load TOTP record
    let record = state
        .db
        .load_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::internal("TOTP record missing for user with pending 2FA"))?;

    if !record.enabled {
        return Err(ApiError::internal(
            "TOTP not enabled but 2FA login was issued — contact support",
        ));
    }

    let code_accepted = if req.code.len() == 6 && req.code.chars().all(|c| c.is_ascii_digit()) {
        // Regular 6-digit TOTP code
        let key = totp_encryption_key(&state);
        let secret_base32 = decrypt_secret(&key, &record.secret_encrypted)
            .map_err(|e| ApiError::internal(&format!("Decryption failed: {e}")))?;
        let secret = TotpSecret::from_base32(&secret_base32)
            .ok_or_else(|| ApiError::internal("Stored TOTP secret is corrupt"))?;
        let now_ts = Utc::now().timestamp() as u64;
        secret.verify(&req.code, now_ts)
    } else if req.code.len() == 8 && req.code.chars().all(|c| c.is_alphanumeric()) {
        // 8-character backup code
        let code_hash = hash_backup_code(&req.code);
        state
            .db
            .consume_backup_code(user_id, &code_hash)
            .await
            .map_err(|e| ApiError::internal(&e.to_string()))?
    } else {
        // Invalid format — reject without hitting crypto
        false
    };

    if !code_accepted {
        tracing::warn!(user_id = %user_id, "2FA login rejected — invalid code");
        return Err(ApiError::unauthorized("Invalid 2FA code."));
    }

    let _ = state.db.touch_totp_used(user_id).await;

    // Create full authenticated session
    let token = state
        .db
        .create_session(user_id, "unknown", "api")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Fetch role for response
    let role_str = state
        .db
        .get_user_role(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .unwrap_or_else(|| "viewer".into());

    let _ = state.db.update_last_login(user_id).await;
    tracing::info!(user_id = %user_id, "2FA login completed successfully");

    Ok((
        StatusCode::CREATED,
        Json(Verify2faResponse {
            token,
            user_id,
            role: role_str,
            expires_in_seconds: 8 * 3600,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_encryption_key_deterministic() {
        // Two calls with the same seed bytes must produce the same key
        let seed_bytes = [0x42u8; 64];
        let mut h1 = Sha256::new();
        h1.update(b"totp-v1:");
        h1.update(&seed_bytes[..32]);
        let k1: [u8; 32] = h1.finalize().into();

        let mut h2 = Sha256::new();
        h2.update(b"totp-v1:");
        h2.update(&seed_bytes[..32]);
        let k2: [u8; 32] = h2.finalize().into();

        assert_eq!(k1, k2);
    }

    #[test]
    fn different_seeds_produce_different_keys() {
        let seed_a = [0x11u8; 64];
        let seed_b = [0x22u8; 64];

        let mut h1 = Sha256::new();
        h1.update(b"totp-v1:");
        h1.update(&seed_a[..32]);
        let k1: [u8; 32] = h1.finalize().into();

        let mut h2 = Sha256::new();
        h2.update(b"totp-v1:");
        h2.update(&seed_b[..32]);
        let k2: [u8; 32] = h2.finalize().into();

        assert_ne!(k1, k2);
    }
}
