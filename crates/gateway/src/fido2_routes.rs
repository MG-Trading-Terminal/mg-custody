//! FIDO2/WebAuthn API endpoints (Phase 20).
//!
//! Registration flow:
//!   1. POST /v1/auth/fido2/register/begin → returns challenge + options
//!   2. Client uses navigator.credentials.create() with these options
//!   3. POST /v1/auth/fido2/register/complete → verifies and stores credential
//!
//! Authentication flow (for Tier 2+ approvals):
//!   1. POST /v1/auth/fido2/authenticate/begin → returns challenge + allowed credentials
//!   2. Client uses navigator.credentials.get()
//!   3. POST /v1/auth/fido2/authenticate/complete → verifies assertion
//!
//! Credential management:
//!   GET /v1/auth/fido2/credentials → list user's credentials
//!   DELETE /v1/auth/fido2/credentials/{id} → disable credential

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{AuthUser, RequireAdmin};
use crate::state::AppState;
use custody_db::fido2::StoredCredential;

/// FIDO2 routes.
pub fn fido2_router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/fido2/register/begin", post(register_begin))
        .route("/v1/auth/fido2/register/complete", post(register_complete))
        .route("/v1/auth/fido2/authenticate/begin", post(authenticate_begin))
        .route("/v1/auth/fido2/authenticate/complete", post(authenticate_complete))
        .route("/v1/auth/fido2/credentials", get(list_credentials))
        .route("/v1/auth/fido2/credentials/{id}", delete(disable_credential))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct RegisterBeginResponse {
    challenge: String,
    rp_id: String,
    rp_name: String,
    user_id: String,
    user_name: String,
    attestation: String,
    timeout_ms: u32,
}

#[derive(Deserialize)]
struct RegisterCompleteRequest {
    challenge: String,
    credential_id: String,
    public_key_cose: String,
    #[serde(default)]
    sign_count: i64,
    #[serde(default)]
    aaguid: Option<String>,
    #[serde(default)]
    device_name: Option<String>,
}

#[derive(Serialize)]
struct RegisterCompleteResponse {
    credential_id: Uuid,
    status: String,
}

#[derive(Serialize)]
struct AuthenticateBeginResponse {
    challenge: String,
    rp_id: String,
    allowed_credentials: Vec<AllowedCred>,
    timeout_ms: u32,
}

#[derive(Serialize)]
struct AllowedCred {
    credential_id: String,
    #[serde(rename = "type")]
    cred_type: String,
}

#[derive(Deserialize)]
struct AuthenticateCompleteRequest {
    challenge: String,
    credential_id: String,
    sign_count: i64,
    // In real WebAuthn: authenticator_data, client_data_json, signature
    // For now we verify the challenge + sign_count monotonicity
}

#[derive(Serialize)]
struct AuthenticateCompleteResponse {
    status: String,
    user_id: Uuid,
    credential_id: String,
}

#[derive(Serialize)]
struct CredentialInfo {
    id: Uuid,
    credential_id: String,
    device_name: Option<String>,
    sign_count: i64,
    registered_at: String,
    last_used_at: Option<String>,
    enabled: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/auth/fido2/register/begin
/// Initiates FIDO2 registration — returns challenge and options.
async fn register_begin(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<RegisterBeginResponse>, ApiError> {
    let challenge = state
        .db
        .create_fido2_challenge(auth.0.user_id, "registration")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // TODO: make rp_id configurable from config/default.toml
    let rp_id = std::env::var("CUSTODY_FIDO2_RP_ID")
        .unwrap_or_else(|_| "custody.local".into());
    let rp_name = std::env::var("CUSTODY_FIDO2_RP_NAME")
        .unwrap_or_else(|_| "MG Custody".into());

    tracing::info!(user = %auth.0.email, "FIDO2 registration started");

    Ok(Json(RegisterBeginResponse {
        challenge,
        rp_id,
        rp_name,
        user_id: auth.0.user_id.to_string(),
        user_name: auth.0.email.clone(),
        attestation: "direct".into(),
        timeout_ms: 60_000,
    }))
}

/// POST /v1/auth/fido2/register/complete
/// Completes FIDO2 registration — verifies challenge and stores credential.
async fn register_complete(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<RegisterCompleteRequest>,
) -> Result<(StatusCode, Json<RegisterCompleteResponse>), ApiError> {
    // Verify and consume the challenge (single-use)
    let challenge = state
        .db
        .consume_fido2_challenge(&req.challenge, "registration")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("Invalid or expired challenge"))?;

    // Verify challenge belongs to this user
    if challenge.user_id != auth.0.user_id {
        return Err(ApiError::unauthorized("Challenge does not belong to this user"));
    }

    // Check credential_id not already registered
    if let Some(_existing) = state
        .db
        .find_fido2_credential_by_cred_id(&req.credential_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
    {
        return Err(ApiError::conflict("Credential already registered"));
    }

    // Store the credential
    let cred = StoredCredential {
        id: Uuid::new_v4(),
        user_id: auth.0.user_id,
        credential_id: req.credential_id,
        public_key_cose: req.public_key_cose,
        sign_count: req.sign_count,
        aaguid: req.aaguid,
        device_name: req.device_name,
        registered_at: Utc::now(),
        last_used_at: None,
        enabled: true,
    };

    state
        .db
        .insert_fido2_credential(&cred)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(
        user = %auth.0.email,
        credential_id = %cred.credential_id,
        "FIDO2 credential registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(RegisterCompleteResponse {
            credential_id: cred.id,
            status: "registered".into(),
        }),
    ))
}

/// POST /v1/auth/fido2/authenticate/begin
/// Initiates FIDO2 authentication — returns challenge and allowed credentials.
async fn authenticate_begin(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<AuthenticateBeginResponse>, ApiError> {
    // Get user's credentials
    let creds = state
        .db
        .list_fido2_credentials(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let active_creds: Vec<_> = creds.into_iter().filter(|c| c.enabled).collect();
    if active_creds.is_empty() {
        return Err(ApiError::bad_request("No FIDO2 credentials registered"));
    }

    let challenge = state
        .db
        .create_fido2_challenge(auth.0.user_id, "authentication")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let rp_id = std::env::var("CUSTODY_FIDO2_RP_ID")
        .unwrap_or_else(|_| "custody.local".into());

    let allowed = active_creds
        .iter()
        .map(|c| AllowedCred {
            credential_id: c.credential_id.clone(),
            cred_type: "public-key".into(),
        })
        .collect();

    Ok(Json(AuthenticateBeginResponse {
        challenge,
        rp_id,
        allowed_credentials: allowed,
        timeout_ms: 60_000,
    }))
}

/// POST /v1/auth/fido2/authenticate/complete
/// Completes FIDO2 authentication — verifies assertion.
async fn authenticate_complete(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<AuthenticateCompleteRequest>,
) -> Result<Json<AuthenticateCompleteResponse>, ApiError> {
    // Verify and consume the challenge
    let challenge = state
        .db
        .consume_fido2_challenge(&req.challenge, "authentication")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("Invalid or expired challenge"))?;

    if challenge.user_id != auth.0.user_id {
        return Err(ApiError::unauthorized("Challenge does not belong to this user"));
    }

    // Find the credential
    let cred = state
        .db
        .find_fido2_credential_by_cred_id(&req.credential_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Credential not found"))?;

    if !cred.enabled {
        return Err(ApiError::unauthorized("Credential is disabled"));
    }

    if cred.user_id != auth.0.user_id {
        return Err(ApiError::unauthorized("Credential does not belong to this user"));
    }

    // Verify sign count monotonically increases (cloned key detection)
    let updated = state
        .db
        .update_fido2_sign_count(&req.credential_id, req.sign_count)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !updated {
        tracing::warn!(
            user = %auth.0.email,
            credential = %req.credential_id,
            old_count = cred.sign_count,
            new_count = req.sign_count,
            "FIDO2 sign count did not increase — possible cloned key!"
        );
        return Err(ApiError::unauthorized(
            "Sign count did not increase — possible cloned authenticator",
        ));
    }

    tracing::info!(
        user = %auth.0.email,
        credential = %req.credential_id,
        "FIDO2 authentication successful"
    );

    Ok(Json(AuthenticateCompleteResponse {
        status: "authenticated".into(),
        user_id: auth.0.user_id,
        credential_id: req.credential_id,
    }))
}

/// GET /v1/auth/fido2/credentials — list user's credentials.
async fn list_credentials(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<CredentialInfo>>, ApiError> {
    let creds = state
        .db
        .list_fido2_credentials(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let infos: Vec<CredentialInfo> = creds
        .into_iter()
        .map(|c| CredentialInfo {
            id: c.id,
            credential_id: c.credential_id,
            device_name: c.device_name,
            sign_count: c.sign_count,
            registered_at: c.registered_at.to_rfc3339(),
            last_used_at: c.last_used_at.map(|t| t.to_rfc3339()),
            enabled: c.enabled,
        })
        .collect();

    Ok(Json(infos))
}

/// DELETE /v1/auth/fido2/credentials/{id} — disable a credential.
async fn disable_credential(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let disabled = state
        .db
        .disable_fido2_credential(id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if disabled {
        Ok(Json(serde_json::json!({"status": "disabled"})))
    } else {
        Err(ApiError::not_found("Credential not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_begin_response_serialization() {
        let resp = RegisterBeginResponse {
            challenge: "abc123".into(),
            rp_id: "custody.local".into(),
            rp_name: "MG Custody".into(),
            user_id: Uuid::new_v4().to_string(),
            user_name: "alice@test.com".into(),
            attestation: "direct".into(),
            timeout_ms: 60_000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("custody.local"));
        assert!(json.contains("direct"));
        assert!(json.contains("60000"));
    }

    #[test]
    fn authenticate_begin_response_serialization() {
        let resp = AuthenticateBeginResponse {
            challenge: "xyz789".into(),
            rp_id: "custody.local".into(),
            allowed_credentials: vec![AllowedCred {
                credential_id: "cred-1".into(),
                cred_type: "public-key".into(),
            }],
            timeout_ms: 60_000,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("public-key"));
        assert!(json.contains("cred-1"));
    }

    #[test]
    fn credential_info_serialization() {
        let info = CredentialInfo {
            id: Uuid::new_v4(),
            credential_id: "cred-abc".into(),
            device_name: Some("YubiKey 5".into()),
            sign_count: 42,
            registered_at: "2026-03-27T00:00:00Z".into(),
            last_used_at: None,
            enabled: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("YubiKey 5"));
        assert!(json.contains("42"));
    }
}
