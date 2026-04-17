//! Mobile push notification device registration endpoints.
//!
//! Allows dashboard/mobile users to register, list, and unregister device tokens
//! for Firebase (Android), APNs (iOS), and Web Push (browser) notifications.
//!
//! All endpoints require at least Viewer role (authenticated session).
//!
//! Routes:
//!   POST   /v1/user/devices         — register a device token
//!   GET    /v1/user/devices         — list registered devices for current user
//!   DELETE /v1/user/devices/{id}    — unregister a device

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, post},
};
use chrono::Utc;
use custody_common::push_notifications::{DeviceToken, Platform};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{require_feature, ApiError};
use crate::rbac_middleware::RequireViewer;
use crate::state::AppState;

/// Build the push notification routes router.
pub fn push_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/user/devices", post(register_device).get(list_devices))
        .route("/v1/user/devices/{id}", delete(unregister_device))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RegisterDeviceRequest {
    /// Platform: "ios", "android", or "web".
    platform: String,
    /// Platform-specific push token or Web Push subscription JSON.
    token: String,
}

#[derive(Serialize)]
struct DeviceTokenResponse {
    id: String,
    user_id: String,
    platform: String,
    token_prefix: String, // first 8 chars only — never log full tokens
    created_at: String,
    last_used_at: Option<String>,
}

impl DeviceTokenResponse {
    fn from_token(t: &DeviceToken) -> Self {
        let prefix_len = t.token.len().min(8);
        Self {
            id: t.id.to_string(),
            user_id: t.user_id.to_string(),
            platform: t.platform.as_str().to_string(),
            token_prefix: format!("{}...", &t.token[..prefix_len]),
            created_at: t.created_at.to_rfc3339(),
            last_used_at: t.last_used_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Register a push notification device token for the authenticated user.
///
/// Idempotent: if the token is already registered, returns 200 with the
/// existing record rather than a duplicate error.
async fn register_device(
    auth: RequireViewer,
    State(state): State<AppState>,
    Json(req): Json<RegisterDeviceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let platform = match req.platform.as_str() {
        "ios" => Platform::Ios,
        "android" => Platform::Android,
        "web" => Platform::Web,
        other => {
            return Err(ApiError::bad_request(&format!(
                "Invalid platform '{other}'. Expected one of: ios, android, web"
            )));
        }
    };

    // Gate: refuse to accept a device token when the matching push
    // provider isn't wired. Storing a token on an operator that can't
    // deliver notifications is the exact silent-failure mode we've
    // been excising — the user sees success, assumes alerts will
    // arrive, and gets nothing.
    let feature = match platform {
        Platform::Ios => "push.apns",
        Platform::Android => "push.fcm",
        Platform::Web => "push.web",
    };
    require_feature(&state.readiness, feature)?;

    if req.token.is_empty() {
        return Err(ApiError::bad_request("device token must not be empty"));
    }
    if req.token.len() > 4096 {
        return Err(ApiError::bad_request(
            "device token exceeds maximum length (4096 chars)",
        ));
    }

    let device_id = Uuid::new_v4();
    let token = DeviceToken {
        id: device_id,
        user_id: auth.0.user_id,
        platform,
        token: req.token.clone(),
        created_at: Utc::now(),
        last_used_at: None,
    };

    let stored = state
        .db
        .upsert_device_token(&token)
        .await
        .map_err(|e| ApiError::internal(&format!("device token upsert: {e}")))?;

    tracing::info!(
        user_id = %stored.user_id,
        platform = %stored.platform,
        device_id = %stored.id,
        "Device push token registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": stored.id,
            "user_id": stored.user_id,
            "platform": stored.platform.as_str(),
            "created_at": stored.created_at.to_rfc3339(),
        })),
    ))
}

/// List all registered devices for the authenticated user.
async fn list_devices(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tokens = state
        .db
        .list_device_tokens(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("device token list: {e}")))?;

    let devices: Vec<DeviceTokenResponse> =
        tokens.iter().map(DeviceTokenResponse::from_token).collect();
    let count = devices.len();
    Ok(Json(serde_json::json!({
        "devices": devices,
        "count": count,
    })))
}

/// Unregister a push notification device.
///
/// DELETE is scoped by (id, user_id) at the DB layer — one user can't
/// unregister another's device even if they guess the UUID.
async fn unregister_device(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let removed = state
        .db
        .delete_device_token(auth.0.user_id, device_id)
        .await
        .map_err(|e| ApiError::internal(&format!("device token delete: {e}")))?;

    if !removed {
        return Err(ApiError::not_found("Device not found"));
    }

    tracing::info!(
        user_id = %auth.0.user_id,
        device_id = %device_id,
        "Device push token unregistered"
    );

    Ok(StatusCode::NO_CONTENT)
}
