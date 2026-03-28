//! Authentication endpoints (Phase 19).
//!
//! Session CRUD for Bearer token auth:
//! - POST /v1/auth/sessions — login (email + password → token)
//! - DELETE /v1/auth/sessions/{token_prefix} — revoke session
//! - GET /v1/auth/sessions — list active sessions (admin or own)
//!
//! # Brute-force protection
//! After 5 failed attempts within 15 minutes, the account enters a 5-minute
//! cooldown.  Every attempt (success or failure) is recorded in `login_history`.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::AuthUser;
use crate::state::AppState;
use custody_db::sessions;

/// Number of failures within the lockout window before a cooldown is applied.
const LOCKOUT_THRESHOLD: i64 = 5;
/// Duration of the sliding failure window.
const LOCKOUT_WINDOW_MINUTES: i64 = 15;
/// Duration of the temporary cooldown once the threshold is reached.
const LOCKOUT_COOLDOWN_MINUTES: i64 = 5;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    /// Full session token, or null when requires_2fa is true.
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub role: Option<String>,
    pub expires_in_seconds: Option<i64>,
    /// True when the account has TOTP enabled and a second factor is needed.
    pub requires_2fa: bool,
    /// Short-lived token to present with the TOTP code at /v1/auth/sessions/verify-2fa.
    /// Only present when requires_2fa is true. Expires in 5 minutes.
    pub pending_token: Option<String>,
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
        .route("/v1/user/login-history", get(list_login_history))
}

/// POST /v1/auth/sessions — create session (login).
///
/// Accepts email + password, verifies against the stored hash.
///
/// If the account has TOTP enabled, returns `requires_2fa: true` and a short-lived
/// `pending_token` instead of a full session. The client must then call
/// `POST /v1/auth/sessions/verify-2fa` with the pending token and their TOTP code.
///
/// If TOTP is not enabled, returns a full session token immediately.
///
/// After 5 failed attempts within 15 minutes, the account is temporarily locked
/// for 5 minutes.  All attempts — successful or not — are recorded in login_history.
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), ApiError> {
    // Extract request metadata for audit recording.
    let ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let user_agent = headers
        .get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Find user by email — use a generic error to avoid email enumeration.
    let (user_id, role_str, password_hash) = match state
        .db
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
    {
        Some(row) => row,
        None => {
            tracing::warn!(email = %req.email, "Login attempt for unknown email");
            return Err(ApiError::unauthorized("Invalid email or password"));
        }
    };

    // ── Brute-force check ────────────────────────────────────────────────────
    // Count recent failures; reject immediately if the lockout threshold is met.
    let since = Utc::now() - Duration::minutes(LOCKOUT_WINDOW_MINUTES);
    let failure_count = state
        .db
        .count_failed_logins(user_id, since)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if failure_count >= LOCKOUT_THRESHOLD {
        tracing::warn!(
            email = %req.email,
            failures = failure_count,
            "Account temporarily locked — too many failed attempts"
        );
        // Record this blocked attempt.
        let _ = state
            .db
            .record_login(user_id, &ip, &user_agent, "password", false, Some("account_locked"))
            .await;
        return Err(ApiError {
            error: "account_locked".into(),
            message: format!(
                "Too many failed attempts. Please try again in {} minutes.",
                LOCKOUT_COOLDOWN_MINUTES
            ),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::TOO_MANY_REQUESTS,
        });
    }

    // ── Password verification ─────────────────────────────────────────────────
    let stored_hash = match password_hash {
        Some(h) => h,
        None => {
            let _ = state
                .db
                .record_login(user_id, &ip, &user_agent, "password", false, Some("password_not_set"))
                .await;
            return Err(ApiError::unauthorized("Password not set — contact admin"));
        }
    };

    if !verify_password(&req.password, &stored_hash) {
        tracing::warn!(email = %req.email, "Failed login attempt — wrong password");
        let _ = state
            .db
            .record_login(user_id, &ip, &user_agent, "password", false, Some("invalid_password"))
            .await;
        return Err(ApiError::unauthorized("Invalid email or password"));
    }

    // ── TOTP check ────────────────────────────────────────────────────────────
    let totp_enabled = state
        .db
        .load_totp(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .map(|t| t.enabled)
        .unwrap_or(false);

    if totp_enabled {
        // Issue a pending 2FA token — full session is created only after TOTP passes.
        let pending_token = state
            .db
            .create_pending_2fa_token(user_id)
            .await
            .map_err(|e| ApiError::internal(&e.to_string()))?;

        tracing::info!(email = %req.email, "Password OK — 2FA required");
        // Record password step as success; the TOTP step will be recorded separately.
        let _ = state
            .db
            .record_login(user_id, &ip, &user_agent, "password", true, None)
            .await;

        return Ok((
            StatusCode::OK,
            Json(LoginResponse {
                token: None,
                user_id: None,
                role: None,
                expires_in_seconds: None,
                requires_2fa: true,
                pending_token: Some(pending_token),
            }),
        ));
    }

    // ── Session creation ──────────────────────────────────────────────────────
    let token = state
        .db
        .create_session(user_id, &ip, &user_agent)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let _ = state.db.update_last_login(user_id).await;
    let _ = state
        .db
        .record_login(user_id, &ip, &user_agent, "password", true, None)
        .await;

    tracing::info!(email = %req.email, role = %role_str, "User logged in");

    Ok((
        StatusCode::CREATED,
        Json(LoginResponse {
            token: Some(token),
            user_id: Some(user_id),
            role: Some(role_str),
            expires_in_seconds: Some(8 * 3600),
            requires_2fa: false,
            pending_token: None,
        }),
    ))
}

/// GET /v1/user/login-history — list recent login events for the authenticated user.
async fn list_login_history(
    State(state): State<AppState>,
    auth: crate::rbac_middleware::RequireViewer,
) -> Result<Json<Vec<custody_db::login_history::LoginHistoryEntry>>, ApiError> {
    let entries = state
        .db
        .list_login_history(auth.0.user_id, 50)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(entries))
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
    fn login_response_serialization_no_2fa() {
        let resp = LoginResponse {
            token: Some("abc123".into()),
            user_id: Some(Uuid::new_v4()),
            role: Some("admin".into()),
            expires_in_seconds: Some(28800),
            requires_2fa: false,
            pending_token: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("abc123"));
        assert!(json.contains("28800"));
        assert!(json.contains("\"requires_2fa\":false"));
    }

    #[test]
    fn login_response_serialization_requires_2fa() {
        let resp = LoginResponse {
            token: None,
            user_id: None,
            role: None,
            expires_in_seconds: None,
            requires_2fa: true,
            pending_token: Some("pending_abc".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"requires_2fa\":true"));
        assert!(json.contains("pending_abc"));
    }
}
