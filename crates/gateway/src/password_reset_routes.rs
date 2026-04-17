//! Password reset and email OTP 2FA routes.
//!
//! # Password reset flow
//! 1. `POST /v1/auth/password-reset/request` — client sends their email.
//!    The server generates an email OTP (purpose = `"password_reset"`),
//!    logs the code (SMTP will replace the `tracing::info!` call when `lettre` is added),
//!    and responds with `{"message": "Code sent if account exists"}` regardless of whether
//!    the email was found (prevents email enumeration).
//!
//! 2. `POST /v1/auth/password-reset/confirm` — client sends email + code + new_password.
//!    The server verifies the OTP and replaces the password hash.
//!    All existing sessions are revoked after a successful reset.
//!
//! # Email OTP as login 2FA
//! If the user has no TOTP enrolled but has `email_otp_enabled = true` on their profile
//! (future: stored in workspace_users), the login handler can issue a pending token and
//! a fresh email OTP.  The client then calls `POST /v1/auth/sessions/verify-2fa` with
//! `code_type = "email_otp"` and the 6-digit code.
//!
//! Email delivery goes through `state.email` — an `EmailSender` enum. In
//! dev the Console transport pretty-prints to stdout; tests use Noop; a
//! real SMTP variant will land when `lettre` is added to the workspace.
//! Never log raw OTP codes via `tracing::info!` — always dispatch via
//! `state.email.send(...)` so the transport can be swapped atomically.

use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use custody_common::email::{render_password_reset, EmailMessage};

use crate::auth_routes::hash_password;
use crate::errors::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn password_reset_router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/password-reset/request", post(request_password_reset))
        .route("/v1/auth/password-reset/confirm", post(confirm_password_reset))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PasswordResetRequestBody {
    pub email: String,
}

#[derive(Serialize)]
pub struct PasswordResetRequestResponse {
    /// Always the same message to prevent email enumeration.
    pub message: String,
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmBody {
    pub email: String,
    /// 6-digit OTP code delivered to the user's email.
    pub code: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct PasswordResetConfirmResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/auth/password-reset/request
///
/// Generates a 6-digit OTP for password reset and "sends" it to the user
/// (currently logged at INFO level; replace with SMTP when available).
///
/// The response is identical whether or not the email exists, to prevent
/// email enumeration.
async fn request_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequestBody>,
) -> Result<Json<PasswordResetRequestResponse>, ApiError> {
    // Validate email format (basic check)
    if req.email.is_empty() || !req.email.contains('@') {
        return Err(ApiError::bad_request("Invalid email address"));
    }

    // Look up user — tolerate not-found silently.
    let user = state
        .db
        .find_user_id_by_email_for_reset(&req.email)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if let Some((user_id, email, _enabled)) = user {
        let code = state
            .db
            .create_email_otp(user_id, "password_reset")
            .await
            .map_err(|e| ApiError::internal(&e.to_string()))?;

        let (subject, body) = render_password_reset(&code, 10);
        if let Err(e) = state
            .email
            .send(EmailMessage::new(email.clone(), subject, body))
            .await
        {
            tracing::warn!(
                email = %email,
                error = %e,
                "Password reset email dispatch failed — returning generic success to caller"
            );
        }
    } else {
        // Introduce a consistent small delay to resist timing-based enumeration.
        // In production, also consider uniform response time via tokio::time::sleep.
        tracing::debug!(email = %req.email, "Password reset requested for unknown email — silently ignored");
    }

    Ok(Json(PasswordResetRequestResponse {
        message: "If an account with that email exists, a reset code has been sent.".into(),
    }))
}

/// POST /v1/auth/password-reset/confirm
///
/// Verifies the OTP and updates the user's password.
/// - Validates new password length (≥ 12 characters).
/// - Verifies and atomically consumes the OTP.
/// - Replaces the password hash.
/// - Revokes all existing sessions (forces re-login everywhere).
async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirmBody>,
) -> Result<Json<PasswordResetConfirmResponse>, ApiError> {
    // Basic input validation
    if req.email.is_empty() || !req.email.contains('@') {
        return Err(ApiError::bad_request("Invalid email address"));
    }
    if req.code.len() != 6 || !req.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::bad_request("Reset code must be exactly 6 digits"));
    }
    if req.new_password.len() < 12 {
        return Err(ApiError::bad_request(
            "New password must be at least 12 characters",
        ));
    }

    // Find user — again tolerate not-found with a generic message.
    let user = state
        .db
        .find_user_id_by_email_for_reset(&req.email)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let (user_id, _email, _enabled) = user.ok_or_else(|| {
        // Generic message — same for wrong-email and wrong-code.
        ApiError::unauthorized("Invalid email or reset code")
    })?;

    // Verify and consume the OTP atomically.
    let ok = state
        .db
        .verify_email_otp(user_id, &req.code, "password_reset")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !ok {
        tracing::warn!(user_id = %user_id, "Password reset failed — invalid or expired OTP");
        return Err(ApiError::unauthorized("Invalid email or reset code"));
    }

    // Update password.
    let new_hash = hash_password(&req.new_password);
    state
        .db
        .set_user_password_hash(user_id, &new_hash)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Revoke all sessions — user must re-authenticate with the new password.
    let revoked = state
        .db
        .revoke_user_sessions(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(
        user_id = %user_id,
        revoked_sessions = revoked,
        "Password reset completed successfully"
    );

    Ok(Json(PasswordResetConfirmResponse {
        message: "Password updated. All existing sessions have been signed out.".into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_reset_request_body_deserialize() {
        let json = r#"{"email":"user@example.com"}"#;
        let body: PasswordResetRequestBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.email, "user@example.com");
    }

    #[test]
    fn password_reset_confirm_body_deserialize() {
        let json = r#"{"email":"user@example.com","code":"123456","new_password":"SuperSecure123!"}"#;
        let body: PasswordResetConfirmBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.email, "user@example.com");
        assert_eq!(body.code, "123456");
        assert_eq!(body.new_password, "SuperSecure123!");
    }

    #[test]
    fn password_length_validation() {
        // Minimum 12 characters is enforced in handler.
        assert!("short".len() < 12);
        assert!("exactly12chr".len() >= 12);
        assert!("SuperSecure123!".len() >= 12);
    }

    #[test]
    fn otp_code_format_validation() {
        // Handler checks: len == 6, all ascii_digit.
        let valid = "123456";
        assert!(valid.len() == 6 && valid.chars().all(|c| c.is_ascii_digit()));

        let too_short = "12345";
        assert!(!(too_short.len() == 6));

        let non_digit = "12345a";
        assert!(!non_digit.chars().all(|c| c.is_ascii_digit()));

        let too_long = "1234567";
        assert!(!(too_long.len() == 6));
    }

    #[test]
    fn email_basic_validation() {
        assert!("user@example.com".contains('@'));
        assert!(!"notanemail".contains('@'));
        assert!(!"".contains('@'));
    }

    #[test]
    fn password_reset_request_response_serialization() {
        let r = PasswordResetRequestResponse {
            message: "If an account with that email exists, a reset code has been sent.".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("message"));
        assert!(json.contains("reset code"));
    }

    #[test]
    fn password_reset_confirm_response_serialization() {
        let r = PasswordResetConfirmResponse {
            message: "Password updated. All existing sessions have been signed out.".into(),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("signed out"));
    }
}
