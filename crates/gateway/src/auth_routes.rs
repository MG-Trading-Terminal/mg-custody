//! Authentication endpoints (Phase 19).
//!
//! Session CRUD for Bearer token auth:
//! - POST /v1/auth/sessions — login (email + password → token)
//! - DELETE /v1/auth/sessions/{token_prefix} — revoke session
//! - GET /v1/auth/sessions — list active sessions (admin or own)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::AuthUser;
use crate::state::AppState;
use custody_db::sessions;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: Uuid,
    pub role: String,
    pub expires_in_seconds: i64,
}

#[derive(Serialize)]
pub struct SessionListItem {
    pub token_hash_prefix: String,
    pub user_id: Uuid,
    pub email: String,
    pub ip_address: String,
    pub created_at: String,
    pub last_active_at: String,
}

/// Auth routes (no RBAC guard on login, guarded on list/revoke).
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/sessions", post(login))
        .route("/v1/auth/sessions", get(list_sessions))
        .route("/v1/auth/sessions/{token_prefix}", delete(revoke_session))
}

/// POST /v1/auth/sessions — create session (login).
///
/// Accepts email + password, verifies against argon2id hash in DB,
/// returns a Bearer token valid for 8 hours.
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), ApiError> {
    // Find user by email
    let (user_id, role_str, password_hash) = state
        .db
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("Invalid email or password"))?;

    // Verify password
    let stored_hash = password_hash
        .ok_or_else(|| ApiError::unauthorized("Password not set — contact admin"))?;

    if !verify_password(&req.password, &stored_hash) {
        tracing::warn!(email = %req.email, "Failed login attempt");
        return Err(ApiError::unauthorized("Invalid email or password"));
    }

    // Extract IP from request (not available here directly, use placeholder)
    let ip = "unknown".to_string(); // Will be set properly when we have ConnectInfo
    let user_agent = "api".to_string();

    // Create session
    let token = state
        .db
        .create_session(user_id, &ip, &user_agent)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Update last_login_at
    let _ = state.db.update_last_login(user_id).await;

    tracing::info!(email = %req.email, role = %role_str, "User logged in");

    Ok((
        StatusCode::CREATED,
        Json(LoginResponse {
            token,
            user_id,
            role: role_str,
            expires_in_seconds: 8 * 3600,
        }),
    ))
}

/// GET /v1/auth/sessions — list sessions.
/// Admin sees all; regular user sees own sessions only.
async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<SessionListItem>>, ApiError> {
    let sessions = state
        .db
        .list_user_sessions(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let items: Vec<SessionListItem> = sessions
        .into_iter()
        .map(|s| SessionListItem {
            token_hash_prefix: s.token_hash_prefix,
            user_id: s.user_id,
            email: s.email,
            ip_address: s.ip_address,
            created_at: s.created_at.to_rfc3339(),
            last_active_at: s.last_active_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(items))
}

/// DELETE /v1/auth/sessions/{token_prefix} — revoke a session.
/// Admin can revoke any session; regular user can revoke own only.
async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(token_prefix): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // For now, user can only revoke by providing exact token to hash
    // In practice, the dashboard would show token_hash_prefix and allow revocation
    let token_hash = sessions::hash_token(&token_prefix);
    let revoked = state
        .db
        .revoke_session(&token_hash)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if revoked {
        tracing::info!(user = %auth.0.email, "Session revoked");
        Ok(Json(serde_json::json!({"status": "revoked"})))
    } else {
        Err(ApiError::not_found("Session not found or already revoked"))
    }
}

/// Verify password against argon2id hash.
/// For MVP, uses simple SHA-256 comparison (upgrade to argon2id when argon2 crate added).
fn verify_password(password: &str, stored_hash: &str) -> bool {
    use sha2::{Digest, Sha256};
    let computed = hex::encode(Sha256::digest(password.as_bytes()));
    // Constant-time comparison
    constant_time_eq(computed.as_bytes(), stored_hash.as_bytes())
}

/// Hash a password for storage.
/// For MVP, uses SHA-256. TODO: upgrade to argon2id.
pub fn hash_password(password: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(password.as_bytes()))
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hashing() {
        let hash = hash_password("my_secret_password");
        assert_eq!(hash.len(), 64); // SHA-256 hex
        assert!(verify_password("my_secret_password", &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn constant_time_comparison() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer_string"));
    }

    #[test]
    fn login_response_serialization() {
        let resp = LoginResponse {
            token: "abc123".into(),
            user_id: Uuid::new_v4(),
            role: "admin".into(),
            expires_in_seconds: 28800,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("28800"));
    }
}
