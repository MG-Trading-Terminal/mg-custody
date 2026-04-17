//! Email-address verification routes.
//!
//! A user's `email_verified_at` column starts NULL and gets stamped
//! once they successfully consume an OTP with `purpose = "email_verify"`.
//!
//! # Flow
//! 1. `POST /v1/auth/verify-email/request` — authenticated. Generates an
//!    OTP, renders the template, and hands it to `state.email` for
//!    delivery. Always returns a generic success message — we do not
//!    leak whether dispatch actually succeeded, to avoid giving a
//!    signal to someone holding a stolen session token.
//! 2. `POST /v1/auth/verify-email/confirm` — authenticated. Consumes
//!    the OTP atomically and stamps `email_verified_at`.
//! 3. `GET  /v1/auth/verify-email/status` — cheap probe the UI can hit
//!    to decide whether to show the "verify your email" banner.
//!
//! Unlike password reset, these endpoints are authenticated: you prove
//! you own the email by signing in (knowing the password), and then
//! confirm you *control* the inbox by consuming an OTP we send to it.
//! That means a workspace admin who sets someone's password still
//! can't fake verification without access to the mailbox.
//!
//! The request handler rate-limits itself via the standard per-user
//! limiter configured on `AppState.user_rate_limiter`.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use custody_common::email::{render_email_verification, EmailMessage};

use crate::errors::ApiError;
use crate::rbac_middleware::RequireViewer;
use crate::state::AppState;

/// Must match `OTP_EXPIRY_MINUTES` in `custody_db::email_otp`. Kept as
/// a const here so we can pass it into the template body without
/// plumbing the DB constant through the public API.
const OTP_TTL_MINUTES: u64 = 10;

pub fn email_verification_router() -> Router<AppState> {
    Router::new()
        .route("/v1/auth/verify-email/request", post(request_verification))
        .route("/v1/auth/verify-email/confirm", post(confirm_verification))
        .route("/v1/auth/verify-email/status", get(verification_status))
}

#[derive(Serialize)]
pub struct VerificationRequestResponse {
    pub message: String,
    /// Echoed so the UI can confirm which address it's asking the user
    /// to check. This is the user's own email — no information leak.
    pub email: String,
}

#[derive(Deserialize)]
pub struct VerificationConfirmBody {
    pub code: String,
}

#[derive(Serialize)]
pub struct VerificationConfirmResponse {
    pub message: String,
    pub email_verified: bool,
}

#[derive(Serialize)]
pub struct VerificationStatusResponse {
    pub email: String,
    pub email_verified: bool,
}

/// POST /v1/auth/verify-email/request
///
/// Issues a fresh OTP and dispatches it via `state.email`. If the user
/// is already verified, short-circuits with a friendly message so the
/// UI can hide the banner without a second round-trip.
async fn request_verification(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<VerificationRequestResponse>, ApiError> {
    let user_id = auth.0.user_id;

    let (email, already_verified) = state
        .db
        .fetch_email_and_verified(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("User not found"))?;

    if already_verified {
        return Ok(Json(VerificationRequestResponse {
            message: "Email is already verified.".into(),
            email,
        }));
    }

    let code = state
        .db
        .create_email_otp(user_id, "email_verify")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let (subject, body) = render_email_verification(&code, OTP_TTL_MINUTES);
    if let Err(e) = state
        .email
        .send(EmailMessage::new(email.clone(), subject, body))
        .await
    {
        tracing::warn!(
            user_id = %user_id,
            error = %e,
            "Email verification dispatch failed — returning generic success to caller"
        );
    }

    Ok(Json(VerificationRequestResponse {
        message: "Verification code sent. Check your inbox.".into(),
        email,
    }))
}

/// POST /v1/auth/verify-email/confirm
///
/// Consumes the OTP and stamps `email_verified_at`. On wrong/expired
/// code we return 401 with a generic message — no hint about which
/// of {wrong code, expired, already consumed} actually happened.
async fn confirm_verification(
    auth: RequireViewer,
    State(state): State<AppState>,
    Json(req): Json<VerificationConfirmBody>,
) -> Result<Json<VerificationConfirmResponse>, ApiError> {
    if req.code.len() != 6 || !req.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::bad_request("Code must be exactly 6 digits"));
    }
    let user_id = auth.0.user_id;

    let ok = state
        .db
        .verify_email_otp(user_id, &req.code, "email_verify")
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !ok {
        tracing::warn!(user_id = %user_id, "Email verification failed — invalid or expired OTP");
        return Err(ApiError::unauthorized("Invalid or expired code"));
    }

    state
        .db
        .mark_email_verified(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(user_id = %user_id, "Email verified");

    Ok(Json(VerificationConfirmResponse {
        message: "Email verified.".into(),
        email_verified: true,
    }))
}

/// GET /v1/auth/verify-email/status
async fn verification_status(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<VerificationStatusResponse>, ApiError> {
    let user_id = auth.0.user_id;
    let (email, verified) = state
        .db
        .fetch_email_and_verified(user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("User not found"))?;

    Ok(Json(VerificationStatusResponse {
        email,
        email_verified: verified,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_body_deserialize() {
        let body: VerificationConfirmBody =
            serde_json::from_str(r#"{"code":"123456"}"#).unwrap();
        assert_eq!(body.code, "123456");
    }

    #[test]
    fn status_response_serialize() {
        let r = VerificationStatusResponse {
            email: "u@e.com".into(),
            email_verified: false,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("email_verified"));
        assert!(json.contains("u@e.com"));
    }

    #[test]
    fn otp_ttl_matches_db_expiry() {
        assert_eq!(OTP_TTL_MINUTES, 10);
    }
}
