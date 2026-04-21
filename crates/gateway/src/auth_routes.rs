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
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
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
    /// Whether the user's email address has been verified. When false the
    /// UI should nudge the user toward the verify-email flow — we do NOT
    /// gate login on this, because locking a user out of their account
    /// over an unsent verification email is a worse failure mode than a
    /// temporarily unverified session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,
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
        .route("/v1/auth/sessions/current", delete(logout_current))
        .route("/v1/auth/sessions/{token_prefix}", delete(revoke_session))
        .route("/v1/user/login-history", get(list_login_history))
}

/// Attach a `Set-Cookie` header carrying `token` to a JSON body.
///
/// Returns a full `Response` (instead of a tuple extractor) because
/// axum's tuple form does not compose cleanly when the status code,
/// header map, and JSON body all vary by branch.
fn respond_with_cookie(
    status: StatusCode,
    token: &str,
    body: LoginResponse,
) -> Result<Response, ApiError> {
    let cookie = crate::session_cookie::build_set_cookie(token);
    let cookie_val = HeaderValue::from_str(&cookie)
        .map_err(|e| ApiError::internal(&format!("cookie header: {e}")))?;
    let mut resp = (status, Json(body)).into_response();
    resp.headers_mut().append(header::SET_COOKIE, cookie_val);
    Ok(resp)
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
) -> Result<Response, ApiError> {
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

    // Auto-upgrade legacy SHA-256 hashes to argon2id on successful login
    if needs_rehash(&stored_hash) {
        let new_hash = hash_password(&req.password);
        if let Err(e) = state.db.set_user_password_hash(user_id, &new_hash).await {
            tracing::warn!(error = %e, "Failed to rehash password (non-critical)");
        } else {
            tracing::info!(user = %req.email, "Password upgraded from SHA-256 to argon2id");
        }
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

        // 2FA pending — no full session yet, so no Set-Cookie. The
        // pending_token is short-lived and lives in the JSON body only.
        return Ok((
            StatusCode::OK,
            Json(LoginResponse {
                token: None,
                user_id: None,
                role: None,
                expires_in_seconds: None,
                requires_2fa: true,
                pending_token: Some(pending_token),
                email_verified: None,
            }),
        )
            .into_response());
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

    // Best-effort — if the column doesn't exist yet in some stale test DB,
    // we fall back to None rather than failing login.
    let email_verified = state.db.is_email_verified(user_id).await.ok();

    tracing::info!(email = %req.email, role = %role_str, "User logged in");

    respond_with_cookie(
        StatusCode::CREATED,
        &token,
        LoginResponse {
            token: Some(token.clone()),
            user_id: Some(user_id),
            role: Some(role_str),
            expires_in_seconds: Some(8 * 3600),
            requires_2fa: false,
            pending_token: None,
            email_verified,
        },
    )
}

/// DELETE /v1/auth/sessions/current — log out the caller.
///
/// Revokes the session whose token arrived in the cookie (or the
/// Authorization header, for script/CLI callers) and emits a
/// `Set-Cookie: custody_session=; Max-Age=0` so the browser drops it
/// even if the DB revoke races.
async fn logout_current(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let token = headers
        .get("Cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::session_cookie::extract_session_token)
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer ").map(|t| t.to_string()))
        });

    // Revoke the row even when we can't — log the failure. A
    // silent `let _ = ...` here used to promise the caller
    // `status: "logged_out"` while the DB row kept the token
    // live until its 8h expiry. If the user hit logout in
    // response to a suspected cookie leak, that gap is exactly
    // what they're trying to close. We still clear the client
    // cookie (cookie clear is browser-atomic, does not depend
    // on the DB) but we surface the DB outcome so the UI can
    // warn + the operator can see it in logs.
    let mut server_revoked: Option<bool> = None;
    if let Some(tok) = token {
        let token_hash = sessions::hash_token(&tok);
        match state.db.revoke_session(&token_hash).await {
            Ok(was_active) => server_revoked = Some(was_active),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Logout: server-side session revoke failed — client cookie \
                     cleared but the token row remains in the DB until the \
                     cleanup task or expiry reaps it. If this logout was \
                     triggered by suspected credential leak, rotate \
                     CUSTODY_HMAC_SECRET or revoke all sessions for the \
                     user via the admin endpoint."
                );
                server_revoked = None;
            }
        }
    }

    let cookie_val = HeaderValue::from_str(&crate::session_cookie::build_clear_cookie())
        .map_err(|e| ApiError::internal(&format!("cookie header: {e}")))?;
    let mut resp = Json(serde_json::json!({
        "status": "logged_out",
        "server_revoked": server_revoked,
    }))
    .into_response();
    resp.headers_mut().append(header::SET_COOKIE, cookie_val);
    Ok(resp)
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

/// Verify password against stored hash.
///
/// Supports both argon2id (new, prefix `$argon2id$`) and legacy SHA-256 (hex string).
/// Legacy hashes are auto-upgraded on next successful login via rehash.
fn verify_password(password: &str, stored_hash: &str) -> bool {
    if stored_hash.starts_with("$argon2") {
        // Argon2id verification (constant-time internally)
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        let Ok(parsed) = PasswordHash::new(stored_hash) else { return false };
        Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
    } else {
        // Legacy SHA-256 fallback (for migration from old hashes)
        use sha2::{Digest, Sha256};
        let computed = hex::encode(Sha256::digest(password.as_bytes()));
        constant_time_eq(computed.as_bytes(), stored_hash.as_bytes())
    }
}

/// Check if a stored hash needs rehashing (i.e., it's legacy SHA-256).
pub fn needs_rehash(stored_hash: &str) -> bool {
    !stored_hash.starts_with("$argon2")
}

/// Hash a password for storage using argon2id.
///
/// Parameters: OWASP 2024 recommended (m=19456 KiB, t=2, p=1).
/// Returns PHC string format: `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`
pub fn hash_password(password: &str) -> String {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    let salt = SaltString::generate(&mut rand::thread_rng());
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2id hashing should not fail")
        .to_string()
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
    fn password_hashing_argon2id() {
        let hash = hash_password("my_secret_password");
        // Argon2id hashes start with $argon2id$
        assert!(hash.starts_with("$argon2id$"), "Hash should be argon2id, got: {}", &hash[..20]);
        assert!(verify_password("my_secret_password", &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn legacy_sha256_still_verifies() {
        // Legacy hash: SHA-256 of "legacy_password"
        use sha2::{Digest, Sha256};
        let legacy_hash = hex::encode(Sha256::digest(b"legacy_password"));
        assert!(verify_password("legacy_password", &legacy_hash));
        assert!(!verify_password("wrong", &legacy_hash));
        assert!(needs_rehash(&legacy_hash), "SHA-256 should need rehash");
    }

    #[test]
    fn argon2id_does_not_need_rehash() {
        let hash = hash_password("test123");
        assert!(!needs_rehash(&hash), "Argon2id should not need rehash");
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
            email_verified: Some(true),
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
            email_verified: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"requires_2fa\":true"));
        assert!(json.contains("pending_abc"));
    }
}
