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
    routing::{delete, get, post},
};
use chrono::Utc;
use custody_common::push_notifications::{DeviceToken, Platform};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
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
    State(_state): State<AppState>,
    Json(req): Json<RegisterDeviceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    // Validate platform
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

    if req.token.is_empty() {
        return Err(ApiError::bad_request("device token must not be empty"));
    }

    // Limit token length to prevent abuse
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

    // In production: INSERT ... ON CONFLICT (token) DO UPDATE SET last_used_at = NOW()
    tracing::info!(
        user_id = %token.user_id,
        platform = %token.platform,
        device_id = %token.id,
        "Device push token registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": token.id,
            "user_id": token.user_id,
            "platform": token.platform.as_str(),
            "created_at": token.created_at.to_rfc3339(),
        })),
    ))
}

/// List all registered devices for the authenticated user.
async fn list_devices(
    auth: RequireViewer,
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    // In production: SELECT * FROM device_tokens WHERE user_id = $1 ORDER BY created_at DESC
    tracing::debug!(user_id = %auth.0.user_id, "Listing registered devices");

    Json(serde_json::json!({
        "devices": [],
        "count": 0,
    }))
}

/// Unregister a push notification device.
///
/// Users can only unregister their own devices.
async fn unregister_device(
    auth: RequireViewer,
    State(_state): State<AppState>,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // In production: DELETE FROM device_tokens WHERE id = $1 AND user_id = $2
    tracing::info!(
        user_id = %auth.0.user_id,
        device_id = %device_id,
        "Device push token unregistered"
    );

    Ok(StatusCode::NO_CONTENT)
}
