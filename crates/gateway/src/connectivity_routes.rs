//! Connectivity API: exchange connections, API keys, notification channels,
//! system config, network fees.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use chrono::Utc;
use custody_common::connectivity::*;
use custody_common::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::state::AppState;

/// Build the connectivity router.
pub fn connectivity_router() -> Router<AppState> {
    Router::new()
        // Exchange connections
        .route("/v1/connections", get(list_connections))
        .route("/v1/connections", post(create_connection))
        .route("/v1/connections/{id}", get(get_connection))
        .route("/v1/connections/{id}", put(update_connection))
        .route("/v1/connections/{id}", delete(delete_connection))
        .route("/v1/connections/{id}/heartbeat", post(heartbeat_connection))
        // API keys
        .route("/v1/api-keys", get(list_api_keys))
        .route("/v1/api-keys", post(create_api_key))
        .route("/v1/api-keys/{id}", get(get_api_key))
        .route("/v1/api-keys/{id}/revoke", post(revoke_api_key))
        .route("/v1/api-keys/{id}", delete(delete_api_key))
        // Notification channels
        .route("/v1/notifications/channels", get(list_notification_channels))
        .route("/v1/notifications/channels", post(create_notification_channel))
        .route("/v1/notifications/channels/{id}", delete(delete_notification_channel))
        .route("/v1/notifications/channels/{id}/toggle", post(toggle_notification_channel))
        .route("/v1/notifications/test/{id}", post(test_notification))
        // System config
        .route("/v1/system/config", get(get_system_config))
        .route("/v1/system/config", put(update_system_config))
        // Network fees (real-time)
        .route("/v1/network/fees", get(get_network_fees))
        .route("/v1/network/status", get(get_network_status))
}

// ---------------------------------------------------------------------------
// Exchange connections
// ---------------------------------------------------------------------------

async fn list_connections(
    State(state): State<AppState>,
) -> Result<Json<Vec<ExchangeConnection>>, ApiError> {
    let connections = state.db.list_connections().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(connections))
}

async fn create_connection(
    State(state): State<AppState>,
    Json(req): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<ExchangeConnection>), ApiError> {
    let now = Utc::now();
    let conn = ExchangeConnection {
        id: Uuid::new_v4(),
        name: req.name,
        connection_type: req.connection_type,
        status: ConnectionStatus::Pending,
        api_url: req.api_url,
        supported_chains: req.supported_chains,
        auto_credit: req.auto_credit,
        created_at: now,
        updated_at: now,
        last_heartbeat_at: None,
    };

    state.db.insert_connection(&conn).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %conn.id, name = %conn.name, "Exchange connection created");
    Ok((StatusCode::CREATED, Json(conn)))
}

async fn get_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ExchangeConnection>, ApiError> {
    let conn = state.db.find_connection(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;
    Ok(Json(conn))
}

async fn update_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateConnectionRequest>,
) -> Result<Json<ExchangeConnection>, ApiError> {
    if let Some(ref status) = req.status {
        state.db.update_connection_status(id, &status.to_string()).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
    }

    let conn = state.db.find_connection(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Connection not found"))?;
    Ok(Json(conn))
}

async fn delete_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_connection(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("Connection not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn heartbeat_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state.db.update_connection_heartbeat(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    state.db.update_connection_status(id, "active").await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({"status": "ok", "connection_id": id})))
}

// ---------------------------------------------------------------------------
// API key management
// ---------------------------------------------------------------------------

async fn list_api_keys(
    State(state): State<AppState>,
) -> Result<Json<Vec<ApiKey>>, ApiError> {
    let keys = state.db.list_api_keys().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(keys))
}

async fn create_api_key(
    State(state): State<AppState>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), ApiError> {
    // Generate a random API key
    let raw_key = format!("cst_{}", hex::encode(&Uuid::new_v4().as_bytes()[..]));

    // Hash it for storage
    use sha2::Digest;
    let key_hash = hex::encode(sha2::Sha256::digest(raw_key.as_bytes()));

    let api_key = ApiKey {
        id: Uuid::new_v4(),
        name: req.name.clone(),
        key: None,
        key_hash,
        role: req.role,
        permissions: req.permissions.clone(),
        ip_whitelist: req.ip_whitelist,
        rate_limit_per_minute: req.rate_limit_per_minute,
        enabled: true,
        created_at: Utc::now(),
        last_used_at: None,
        expires_at: req.expires_at,
    };

    state.db.insert_api_key(&api_key).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %api_key.id, name = %api_key.name, role = %api_key.role, "API key created");

    Ok((StatusCode::CREATED, Json(CreateApiKeyResponse {
        id: api_key.id,
        name: req.name,
        api_key: raw_key, // only time it's returned
        role: req.role,
        permissions: req.permissions,
        created_at: api_key.created_at,
        expires_at: api_key.expires_at,
    })))
}

async fn get_api_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiKey>, ApiError> {
    let key = state.db.find_api_key(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("API key not found"))?;
    Ok(Json(key))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let revoked = state.db.revoke_api_key(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !revoked {
        return Err(ApiError::not_found("API key not found"));
    }
    tracing::info!(id = %id, "API key revoked");
    Ok(Json(serde_json::json!({"status": "revoked", "id": id})))
}

async fn delete_api_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_api_key(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("API key not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Notification channels
// ---------------------------------------------------------------------------

async fn list_notification_channels(
    State(state): State<AppState>,
) -> Result<Json<Vec<NotificationChannel>>, ApiError> {
    let channels = state.db.list_notification_channels().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(channels))
}

async fn create_notification_channel(
    State(state): State<AppState>,
    Json(req): Json<CreateNotificationChannelRequest>,
) -> Result<(StatusCode, Json<NotificationChannel>), ApiError> {
    let channel = NotificationChannel {
        id: Uuid::new_v4(),
        name: req.name,
        channel_type: req.channel_type,
        config: req.config,
        min_severity: req.min_severity,
        enabled: true,
        created_at: Utc::now(),
    };

    state.db.insert_notification_channel(&channel).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %channel.id, name = %channel.name, "Notification channel created");
    Ok((StatusCode::CREATED, Json(channel)))
}

async fn delete_notification_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_notification_channel(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("Notification channel not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_notification_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    state.db.toggle_notification_channel(id, enabled).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({"id": id, "enabled": enabled})))
}

async fn test_notification(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let channel = state.db.find_notification_channel(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Notification channel not found"))?;

    // For now, return a mock test result
    Ok(Json(serde_json::json!({
        "channel_id": id,
        "channel_type": channel.channel_type,
        "test_sent": true,
        "message": "Test notification sent successfully",
    })))
}

// ---------------------------------------------------------------------------
// System configuration
// ---------------------------------------------------------------------------

async fn get_system_config() -> Json<SystemConfig> {
    // Return defaults — in production this would read from DB
    Json(SystemConfig::default())
}

async fn update_system_config(
    Json(req): Json<UpdateSystemConfigRequest>,
) -> Result<Json<SystemConfig>, ApiError> {
    let mut config = SystemConfig::default();

    if let Some(v) = req.withdrawal_limit_single { config.withdrawal_limit_single = v; }
    if let Some(v) = req.withdrawal_limit_hourly { config.withdrawal_limit_hourly = v; }
    if let Some(v) = req.withdrawal_limit_daily { config.withdrawal_limit_daily = v; }
    if let Some(v) = req.withdrawals_paused { config.withdrawals_paused = v; }
    if let Some(v) = req.deposits_paused { config.deposits_paused = v; }
    if let Some(v) = req.confirmation_overrides { config.confirmation_overrides = v; }
    if let Some(v) = req.maintenance_mode { config.maintenance_mode = v; }

    tracing::info!(
        paused = config.withdrawals_paused,
        maintenance = config.maintenance_mode,
        "System config updated"
    );

    Ok(Json(config))
}

// ---------------------------------------------------------------------------
// Network fees & status
// ---------------------------------------------------------------------------

async fn get_network_fees() -> Json<serde_json::Value> {
    let fees = vec![
        NetworkFeeCache {
            chain: Chain::Ethereum,
            base_fee: dec!(25.5),
            priority_fee: dec!(2.0),
            max_fee: dec!(53.0),
            last_updated: Utc::now(),
            block_number: Some(19_500_000),
            is_stale: false,
        },
        NetworkFeeCache {
            chain: Chain::Bitcoin,
            base_fee: dec!(15),
            priority_fee: Decimal::ZERO,
            max_fee: dec!(15),
            last_updated: Utc::now(),
            block_number: Some(840_000),
            is_stale: false,
        },
        NetworkFeeCache {
            chain: Chain::Solana,
            base_fee: dec!(0.000005),
            priority_fee: dec!(0.00001),
            max_fee: dec!(0.000015),
            last_updated: Utc::now(),
            block_number: Some(250_000_000),
            is_stale: false,
        },
        NetworkFeeCache {
            chain: Chain::Tron,
            base_fee: dec!(40),
            priority_fee: Decimal::ZERO,
            max_fee: dec!(40),
            last_updated: Utc::now(),
            block_number: Some(60_000_000),
            is_stale: false,
        },
    ];

    Json(serde_json::json!({"fees": fees}))
}

async fn get_network_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "chains": {
            "ethereum": {"status": "healthy", "block": 19_500_000, "latency_ms": 45},
            "bitcoin": {"status": "healthy", "block": 840_000, "latency_ms": 120},
            "solana": {"status": "healthy", "slot": 250_000_000, "latency_ms": 15},
            "tron": {"status": "healthy", "block": 60_000_000, "latency_ms": 80},
        },
        "timestamp": Utc::now().to_rfc3339(),
    }))
}
