//! API route handlers with structured JSON errors and request tracing.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::*;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::errors::{require_chain_ready, require_feature_not_broken, ApiError};
use crate::rbac_middleware::{RequireViewer, RequireEditor, RequireSigner};
use crate::state::AppState;

/// USD threshold above which FIDO2 second-factor is required for withdrawal approval.
///
/// Corresponds to Tier 2 in the approval table ($10K–$100K requires 1 approver with FIDO2).
const FIDO2_REQUIRED_THRESHOLD_USD: rust_decimal::Decimal =
    rust_decimal::Decimal::from_parts(10_000, 0, 0, false, 0);

pub fn router(state: AppState) -> Router {
    use axum::middleware as axum_mw;
    use crate::rbac_middleware::{self, RbacState};
    use crate::middleware::MiddlewareState;
    use crate::rate_limit::RateLimiter;
    let rbac_state = RbacState::new(state.db.clone());
    // Build a MiddlewareState with no HMAC secret (dev/dashboard path — auth is Bearer only).
    let mw_state = MiddlewareState {
        hmac_secret: vec![],
        rate_limiter: std::sync::Arc::new(RateLimiter::default_api()),
        user_rate_limiter: state.user_rate_limiter.clone(),
        public_paths: vec![
            "/health".into(),
            "/health/chains".into(),
            "/health/deep".into(),
            "/v1/health/features".into(),
            "/metrics".into(),
            "/v1/auth/sessions".into(),
            "/v1/auth/sessions/verify-2fa".into(),
            "/v1/auth/password-reset/request".into(),
            "/v1/auth/password-reset/confirm".into(),
        ],
    };
    Router::new()
        .route("/health", get(health))
        .route("/health/chains", get(health_chains))
        .route("/health/deep", get(crate::health::deep_health))
        .route("/v1/health/features", get(health_features))
        .route("/v1/health/features/refresh", post(health_features_refresh))
        .route("/v1/health/features/stream", get(health_features_stream))
        .route("/v1/health/features/history", get(health_features_history))
        .route(
            "/v1/health/features/uptime",
            get(health_features_uptime),
        )
        .route(
            "/v1/health/features/silences",
            get(list_health_silences).post(create_health_silence),
        )
        .route(
            "/v1/health/features/silences/{feature}",
            axum::routing::delete(end_health_silence),
        )
        .route(
            "/v1/health/features/silences/history",
            get(list_silence_history),
        )
        .route(
            "/v1/admin/chaos/overrides",
            get(list_chaos_overrides).post(set_chaos_override),
        )
        .route(
            "/v1/admin/chaos/overrides/{feature}",
            axum::routing::delete(clear_chaos_override),
        )
        .route(
            "/v1/health/features/{feature}/ack",
            post(ack_incident).delete(clear_incident_ack),
        )
        .route("/v1/addresses", post(create_address))
        .route("/v1/withdrawals", post(submit_withdrawal))
        .route("/v1/withdrawals/{id}", get(get_withdrawal_status))
        .route("/v1/withdrawals/{id}/approve", post(approve_withdrawal))
        .route("/v1/wallets/{asset}", get(get_wallet_balance))
        .route("/v1/ws", get(crate::websocket::ws_handler))
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/openapi.json", get(openapi_spec))
        .route("/v1/sdk/methods", get(sdk_methods))
        .route("/v1/sdk/endpoints", get(endpoint_catalog))
        .route("/v1/sdk/python", get(sdk_python))
        .route("/v1/sdk/typescript", get(sdk_typescript))
        .merge(crate::auth_routes::auth_router())
        .merge(crate::totp_routes::totp_router())
        .merge(crate::fido2_routes::fido2_router())
        .merge(crate::vault_routes::vault_router())
        .merge(crate::ops_routes::ops_router())
        .merge(crate::connectivity_routes::connectivity_router())
        .merge(crate::platform_routes::platform_router())
        .merge(crate::governance_routes::governance_router())
        .merge(crate::operations_routes::operations_router())
        .merge(crate::embedded_routes::embedded_router())
        .merge(crate::swap_routes::swap_router())
        .merge(crate::user_routes::user_router())
        .merge(crate::safe_routes::safe_router())
        .merge(crate::password_reset_routes::password_reset_router())
        .merge(crate::email_verification_routes::email_verification_router())
        .merge(crate::nft_routes::nft_router())
        .merge(crate::webhook_mgmt_routes::webhook_mgmt_router())
        // Dashboard static files (SvelteKit build output)
        // Fallback to index.html for client-side routing
        .fallback_service(dashboard_service())
        .layer(axum_mw::from_fn(crate::middleware::request_logger))
        .layer(axum_mw::from_fn(crate::middleware::api_version_headers))
        .layer(axum_mw::from_fn(crate::middleware::security_headers))
        .layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware::rbac_auth))
        .layer(axum_mw::from_fn_with_state(mw_state, crate::middleware::rate_limit))
        .layer(axum_mw::from_fn(crate::request_id::request_id_middleware))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serve dashboard static files from `dashboard/build/`.
/// Falls back to `index.html` for SPA client-side routing.
fn dashboard_service() -> tower_http::services::ServeDir<tower_http::services::ServeFile> {
    let dashboard_dir = std::env::var("CUSTODY_DASHBOARD_DIR")
        .unwrap_or_else(|_| "dashboard/build".into());
    ServeDir::new(&dashboard_dir)
        .append_index_html_on_directories(true)
        .fallback(tower_http::services::ServeFile::new(
            format!("{}/index.html", dashboard_dir),
        ))
}

pub fn router_with_middleware(state: AppState, hmac_secret: &[u8]) -> Router {
    use axum::middleware as axum_mw;
    use crate::middleware::{self, MiddlewareState};
    use crate::rbac_middleware::{self, RbacState};
    let mw_state = MiddlewareState::new(hmac_secret);
    let rbac_state = RbacState::new(state.db.clone());
    Router::new()
        .route("/health", get(health))
        .route("/health/chains", get(health_chains))
        .route("/health/deep", get(crate::health::deep_health))
        .route("/v1/health/features", get(health_features))
        .route("/v1/health/features/refresh", post(health_features_refresh))
        .route("/v1/health/features/stream", get(health_features_stream))
        .route("/v1/health/features/history", get(health_features_history))
        .route(
            "/v1/health/features/uptime",
            get(health_features_uptime),
        )
        .route(
            "/v1/health/features/silences",
            get(list_health_silences).post(create_health_silence),
        )
        .route(
            "/v1/health/features/silences/{feature}",
            axum::routing::delete(end_health_silence),
        )
        .route(
            "/v1/health/features/silences/history",
            get(list_silence_history),
        )
        .route(
            "/v1/admin/chaos/overrides",
            get(list_chaos_overrides).post(set_chaos_override),
        )
        .route(
            "/v1/admin/chaos/overrides/{feature}",
            axum::routing::delete(clear_chaos_override),
        )
        .route(
            "/v1/health/features/{feature}/ack",
            post(ack_incident).delete(clear_incident_ack),
        )
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/addresses", post(create_address))
        .route("/v1/withdrawals", post(submit_withdrawal))
        .route("/v1/withdrawals/{id}", get(get_withdrawal_status))
        .route("/v1/withdrawals/{id}/approve", post(approve_withdrawal))
        .route("/v1/wallets/{asset}", get(get_wallet_balance))
        .route("/v1/ws", get(crate::websocket::ws_handler))
        .merge(crate::auth_routes::auth_router())
        .merge(crate::totp_routes::totp_router())
        .merge(crate::fido2_routes::fido2_router())
        .merge(crate::user_routes::user_router())
        .merge(crate::safe_routes::safe_router())
        .merge(crate::password_reset_routes::password_reset_router())
        .merge(crate::email_verification_routes::email_verification_router())
        .layer(axum_mw::from_fn(crate::middleware::request_logger))
        .layer(axum_mw::from_fn(crate::middleware::api_version_headers))
        .layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware::rbac_auth))
        .layer(axum_mw::from_fn(crate::request_id::request_id_middleware))
        .layer(axum_mw::from_fn_with_state(mw_state.clone(), middleware::rate_limit))
        .layer(axum_mw::from_fn_with_state(mw_state, middleware::hmac_auth))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// CORS layer — internal API, restrict to configured origins.
fn cors_layer() -> CorsLayer {
    // CORS origin: same-origin by default (dashboard served by same axum server).
    // Override with CUSTODY_CORS_ORIGIN env var for split deployments.
    let allowed_origin = std::env::var("CUSTODY_CORS_ORIGIN").ok();

    let cors = CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-custody-timestamp"),
            axum::http::HeaderName::from_static("x-custody-signature"),
        ])
        .max_age(Duration::from_secs(3600));

    if let Some(origin) = allowed_origin {
        // Split deployment — the SPA lives on a different host than
        // the API. We must emit `Access-Control-Allow-Credentials: true`
        // and an explicit origin (not `*`) for the browser to attach
        // the session cookie to cross-origin requests.
        cors.allow_origin(origin.parse::<axum::http::HeaderValue>().expect("Invalid CUSTODY_CORS_ORIGIN"))
            .allow_credentials(true)
    } else {
        // Default: same-origin only (no Access-Control-Allow-Origin header sent,
        // browser enforces same-origin policy automatically).
        // For development, set CUSTODY_CORS_ORIGIN=http://localhost:5173
        cors
    }
}

async fn health() -> &'static str { "ok" }

/// `GET /v1/health/features` — honest feature readiness report.
///
/// Lists every feature the server was booted with plus its current
/// state (`ready` / `disabled` / `misconfigured` / `degraded`) so
/// operators can see at a glance what will work vs what silently
/// won't. This is the counterpart to the startup log table — the
/// JSON version that dashboards and monitoring can scrape.
/// `POST /v1/health/features/refresh` — tell the background refresh
/// task to re-run every probe now. Returns immediately with 202; the
/// updated report shows up on the next `GET /v1/health/features`
/// (typically within a second or two as probes complete).
///
/// Admin-only — probing external vendors can generate measurable
/// traffic (SMTP handshakes, JSON-RPC calls, HEAD requests), so we
/// don't let unauthenticated callers trigger it.
async fn health_features_refresh(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    state.readiness.request_refresh();
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "requested": true,
            "note": "Probes are re-running asynchronously. \
                     Poll GET /v1/health/features for the updated report.",
        })),
    )
}

/// `GET /v1/health/features/history?since=<rfc3339>&limit=<int>` —
/// past readiness transitions persisted to `readiness_transitions`.
/// Default: latest 200 rows across all features. Admin-only so the
/// incident history isn't world-readable.
async fn health_features_history(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let since = q
        .since
        .as_deref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&chrono::Utc)))
        .transpose()
        .map_err(|e| ApiError::bad_request(&format!("since must be RFC 3339: {e}")))?;
    let rows = state
        .db
        .list_readiness_transitions(since, limit)
        .await
        .map_err(|e| ApiError::internal(&format!("history query: {e}")))?;
    let count = rows.len();
    Ok(Json(serde_json::json!({
        "transitions": rows,
        "count": count,
    })))
}

#[derive(serde::Deserialize)]
struct HistoryQuery {
    since: Option<String>,
    limit: Option<i64>,
}

#[derive(serde::Deserialize)]
struct CreateSilenceRequest {
    feature: String,
    /// Silence duration in seconds from `starts_at` (or from now if
    /// `starts_at` is omitted). Required — no "forever" silences;
    /// an operator would forget one exists.
    duration_secs: i64,
    reason: String,
    /// RFC 3339 start time. Omit for "silence immediately"; provide
    /// a future timestamp to pre-schedule a maintenance window.
    /// Must not be more than 30 days in the future.
    #[serde(default)]
    starts_at: Option<String>,
}

/// `GET /v1/health/features/silences` — every silence still live.
/// Dashboard uses this to render the badge on silenced rows.
async fn list_health_silences(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let rows = state
        .db
        .list_active_silences()
        .await
        .map_err(|e| ApiError::internal(&format!("silence list: {e}")))?;
    let count = rows.len();
    Ok(Json(serde_json::json!({
        "silences": rows,
        "count": count,
    })))
}

/// `POST /v1/health/features/silences` — create one. Maximum 7 days
/// per silence; longer maintenance windows need a conscious re-create.
async fn create_health_silence(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<CreateSilenceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    // Same 10s cooldown as chaos — silences can be just as noisy
    // if a script starts creating dozens in a loop, and each one
    // writes an audit_log row.
    if let Err(retry) = state.admin_cooldown.check(
        &format!("silence-create:{}", auth.0.user_id),
        std::time::Duration::from_secs(10),
    ) {
        return Err(ApiError {
            error: "rate_limited".into(),
            message: format!(
                "Silence create cooldown active — retry in {retry}s."
            ),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::TOO_MANY_REQUESTS,
        });
    }
    const MAX_SECS: i64 = 7 * 24 * 60 * 60;
    if req.feature.is_empty() {
        return Err(ApiError::bad_request("feature must not be empty"));
    }
    if req.duration_secs <= 0 {
        return Err(ApiError::bad_request("duration_secs must be positive"));
    }
    if req.duration_secs > MAX_SECS {
        return Err(ApiError::bad_request(
            "duration_secs may not exceed 7 days (604800)",
        ));
    }
    if req.reason.trim().is_empty() {
        return Err(ApiError::bad_request("reason must not be empty"));
    }
    let now = chrono::Utc::now();
    let starts_at = req
        .starts_at
        .as_deref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&chrono::Utc)))
        .transpose()
        .map_err(|e| ApiError::bad_request(&format!("starts_at must be RFC 3339: {e}")))?;
    // Cap scheduled starts at 30 days out — anything further is
    // likely a typo (year 2025 → 2026 off-by-one in the calendar)
    // and we don't want a silence sitting dormant for months.
    if let Some(start) = starts_at {
        let thirty_days = chrono::Duration::days(30);
        if start > now + thirty_days {
            return Err(ApiError::bad_request(
                "starts_at may not be more than 30 days in the future",
            ));
        }
        if start < now - chrono::Duration::hours(1) {
            return Err(ApiError::bad_request(
                "starts_at may not be more than 1 hour in the past",
            ));
        }
    }
    let expires_at =
        starts_at.unwrap_or(now) + chrono::Duration::seconds(req.duration_secs);
    let created_by = auth.0.user_id.to_string();
    let silence = state
        .db
        .insert_readiness_silence(&req.feature, starts_at, expires_at, &req.reason, &created_by)
        .await
        .map_err(|e| ApiError::internal(&format!("silence insert: {e}")))?;
    let audit = custody_db::audit::AuditEntry::readiness_silence_created(
        &created_by,
        &req.feature,
        req.duration_secs,
        &req.reason,
    );
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.insert_audit_entry(&audit),
        )
        .await;
    });
    tracing::warn!(
        target: "custody::readiness::alert",
        feature = %req.feature,
        expires_at = %expires_at,
        reason = %req.reason,
        created_by = %created_by,
        "Readiness alert silence created"
    );
    Ok((StatusCode::CREATED, Json(serde_json::json!(silence))))
}

/// `GET /v1/health/features/silences/history?limit=N` — every
/// silence ever created, newest first. For postmortem tooling:
/// "what was muted during last Thursday's outage?".
async fn list_silence_history(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let rows = state
        .db
        .list_silence_history(limit)
        .await
        .map_err(|e| ApiError::internal(&format!("silence history: {e}")))?;
    let count = rows.len();
    Ok(Json(serde_json::json!({
        "silences": rows,
        "count": count,
    })))
}

/// `DELETE /v1/health/features/silences/:feature` — end an active
/// silence early (planned maintenance finished ahead of schedule).
async fn end_health_silence(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(feature): Path<String>,
) -> Result<StatusCode, ApiError> {
    let ended = state
        .db
        .end_readiness_silence(&feature)
        .await
        .map_err(|e| ApiError::internal(&format!("silence end: {e}")))?;
    if !ended {
        return Err(ApiError::not_found("No active silence for that feature"));
    }
    let audit = custody_db::audit::AuditEntry::readiness_silence_ended(
        &auth.0.user_id.to_string(),
        &feature,
    );
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.insert_audit_entry(&audit),
        )
        .await;
    });
    tracing::info!(
        target: "custody::readiness::alert",
        feature = %feature,
        ended_by = %auth.0.user_id,
        "Readiness alert silence ended early"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct ChaosOverrideRequest {
    /// Feature name, e.g. "scanner.ethereum". Must match a feature
    /// name that appears in the readiness report — overrides on
    /// non-existent features are silently dropped.
    feature: String,
    /// Target state. Only "degraded" and "misconfigured" make sense
    /// here — a chaos drill that flips a feature to Ready doesn't
    /// test any alerting path.
    state: String,
    /// Free-form note. Written verbatim into the degraded reason
    /// (prefixed with "[chaos]") so incident responders looking at
    /// the webhook payload see immediately that this wasn't real.
    reason: String,
}

/// `GET /v1/admin/chaos/overrides` — list active overrides.
async fn list_chaos_overrides(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    let overrides = state.chaos_overrides.snapshot();
    let count = overrides.len();
    Json(serde_json::json!({
        "overrides": overrides,
        "count": count,
    }))
}

/// `POST /v1/admin/chaos/overrides` — inject an override. The next
/// refresh tick (≤60s, or call `POST /v1/health/features/refresh`
/// to expedite) will apply it and fire the alerting pipeline as if
/// the real vendor went down.
async fn set_chaos_override(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<ChaosOverrideRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    use custody_common::readiness::FeatureState;
    // Per-user 10s cooldown — a shell loop accidentally running
    // this handler in the background would otherwise spam the
    // audit log + refresh task + alert channel every tick.
    if let Err(retry) = state.admin_cooldown.check(
        &format!("chaos-inject:{}", auth.0.user_id),
        std::time::Duration::from_secs(10),
    ) {
        return Err(ApiError {
            error: "rate_limited".into(),
            message: format!(
                "Chaos inject cooldown active — retry in {retry}s."
            ),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::TOO_MANY_REQUESTS,
        });
    }
    if req.feature.is_empty() {
        return Err(ApiError::bad_request("feature must not be empty"));
    }
    if req.reason.trim().is_empty() {
        return Err(ApiError::bad_request("reason must not be empty"));
    }
    let forced = match req.state.as_str() {
        "degraded" => FeatureState::Degraded {
            reason: req.reason.clone(),
        },
        "misconfigured" => FeatureState::Misconfigured {
            missing: vec![format!("[chaos] {}", req.reason)],
        },
        other => {
            return Err(ApiError::bad_request(&format!(
                "state must be \"degraded\" or \"misconfigured\", got \"{other}\""
            )));
        }
    };
    state.chaos_overrides.set(
        &req.feature,
        forced,
        &req.reason,
        auth.0.user_id.to_string(),
    );
    // Audit write is best-effort and bounded. A hung DB must not
    // block the chaos endpoint — it's an operator drill tool; the
    // in-memory override is the source of truth for the refresh
    // task, audit is just the paper trail.
    let audit = custody_db::audit::AuditEntry::readiness_chaos_injected(
        &auth.0.user_id.to_string(),
        &req.feature,
        &req.state,
        &req.reason,
    );
    let db = state.db.clone();
    tokio::spawn(async move {
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.insert_audit_entry(&audit),
        )
        .await;
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(
                target: "custody::readiness::chaos",
                error = %e,
                "Chaos audit write failed"
            ),
            Err(_) => tracing::warn!(
                target: "custody::readiness::chaos",
                "Chaos audit write timed out"
            ),
        }
    });
    // Wake the refresh task so the operator doesn't wait up to 60s
    // to see the alert fire.
    state.readiness.request_refresh();
    tracing::warn!(
        target: "custody::readiness::chaos",
        feature = %req.feature,
        new_state = %req.state,
        reason = %req.reason,
        created_by = %auth.0.user_id,
        "Chaos override injected — next refresh will fire alerts as if the feature had actually degraded"
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "applied": true,
            "note": "Override set. Next refresh (≤60s) will apply it and fire the alert pipeline. \
                     DELETE /v1/admin/chaos/overrides/{feature} to clear."
        })),
    ))
}

/// `DELETE /v1/admin/chaos/overrides/:feature` — remove an override.
/// The next refresh will re-probe reality; the recovery path fires
/// its "feature recovered" alert on that transition.
async fn clear_chaos_override(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(feature): Path<String>,
) -> Result<StatusCode, ApiError> {
    let cleared = state.chaos_overrides.clear(&feature);
    if !cleared {
        return Err(ApiError::not_found("No chaos override for that feature"));
    }
    let audit = custody_db::audit::AuditEntry::readiness_chaos_cleared(
        &auth.0.user_id.to_string(),
        &feature,
    );
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            db.insert_audit_entry(&audit),
        )
        .await;
    });
    state.readiness.request_refresh();
    tracing::info!(
        target: "custody::readiness::chaos",
        feature = %feature,
        cleared_by = %auth.0.user_id,
        "Chaos override cleared — real probe state will be restored on next refresh"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct UptimeQuery {
    /// Window size: "1h", "24h", "7d", "30d". Default 24h.
    #[serde(default)]
    window: Option<String>,
}

/// `GET /v1/health/features/uptime?window=24h` — per-feature uptime
/// percentage over the given window, computed from
/// readiness_transitions.
///
/// Algorithm: for each feature, sort transitions ascending. At t=0
/// (window start) we assume the feature was Ready unless the very
/// first event in-window is a Recovery (which implies it was down
/// before). Walk events, tracking `down_since`; whenever we see a
/// Recovery, close the down interval and add to `down_total`. Open
/// intervals at window end close against `now`. Uptime % = 1 −
/// down_total / window.
///
/// Features that had no transitions in the window are reported as
/// 100% (happy path: never degraded).
async fn health_features_uptime(
    _auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(q): axum::extract::Query<UptimeQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let window_str = q.window.as_deref().unwrap_or("24h");
    let window_duration = parse_window(window_str)
        .ok_or_else(|| ApiError::bad_request("window must be one of: 1h, 24h, 7d, 30d"))?;
    let now = chrono::Utc::now();
    let since = now - window_duration;
    let transitions = state
        .db
        .transitions_in_window(since, now)
        .await
        .map_err(|e| ApiError::internal(&format!("transition query: {e}")))?;

    // Group transitions per feature.
    let mut per_feature: std::collections::HashMap<String, Vec<_>> =
        std::collections::HashMap::new();
    for t in &transitions {
        per_feature.entry(t.feature.clone()).or_default().push(t);
    }

    // Every feature currently in the readiness report, even those
    // with no transitions, should appear in the output — the
    // dashboard wants a row per known feature, not only ones that
    // bounced.
    let snap = state.readiness.snapshot();
    let mut uptimes: Vec<serde_json::Value> = Vec::new();
    let window_secs = window_duration.num_seconds() as f64;
    for f in &snap.features {
        let events = per_feature.remove(&f.name).unwrap_or_default();
        let down_secs = compute_down_seconds(&events, since, now);
        let uptime_pct = if window_secs > 0.0 {
            ((window_secs - down_secs) / window_secs * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };
        let slo = custody_common::readiness::slo_target(&f.name);
        let breach = uptime_pct < slo;
        // Error budget: how many seconds of downtime the SLO permits
        // over this window. 99.9% over 30d = 43.2m. Consumed is the
        // down time we've already spent; remaining is what's left
        // before breach. remaining can go negative during a breach —
        // that's the useful signal.
        let budget_secs = window_secs * (1.0 - slo / 100.0);
        let remaining_secs = budget_secs - down_secs;
        uptimes.push(serde_json::json!({
            "feature": f.name,
            "uptime_pct": format!("{uptime_pct:.4}"),
            "down_seconds": down_secs as i64,
            "transitions": events.len(),
            "slo_target_pct": format!("{slo:.2}"),
            "slo_breach": breach,
            "budget_seconds": budget_secs as i64,
            "budget_consumed_seconds": down_secs as i64,
            "budget_remaining_seconds": remaining_secs as i64,
        }));
    }
    Ok(Json(serde_json::json!({
        "window": window_str,
        "since": since.to_rfc3339(),
        "until": now.to_rfc3339(),
        "features": uptimes,
    })))
}

fn parse_window(s: &str) -> Option<chrono::Duration> {
    match s {
        "1h" => Some(chrono::Duration::hours(1)),
        "24h" => Some(chrono::Duration::hours(24)),
        "7d" => Some(chrono::Duration::days(7)),
        "30d" => Some(chrono::Duration::days(30)),
        _ => None,
    }
}

/// Walk ordered transitions for one feature and return total down
/// seconds within `[window_start, window_end]`. See docstring on
/// `health_features_uptime` for the algorithm.
fn compute_down_seconds(
    events: &[&custody_db::readiness_transitions::ReadinessTransition],
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
) -> f64 {
    let mut down_since: Option<chrono::DateTime<chrono::Utc>> = None;
    // If the first event in-window is a Recovery, the feature was
    // already down before the window opened. Count from window_start.
    if let Some(first) = events.first()
        && first.kind == "recovery"
    {
        down_since = Some(window_start);
    }
    let mut down_total = 0.0;
    for e in events {
        match e.kind.as_str() {
            "degradation" | "misconfiguration" => {
                if down_since.is_none() {
                    down_since = Some(e.occurred_at);
                }
            }
            "recovery" => {
                if let Some(start) = down_since.take() {
                    down_total += (e.occurred_at - start).num_milliseconds() as f64 / 1000.0;
                }
            }
            _ => {}
        }
    }
    // If still down at window_end, count the tail.
    if let Some(start) = down_since {
        down_total += (window_end - start).num_milliseconds() as f64 / 1000.0;
    }
    down_total.max(0.0)
}

#[derive(serde::Deserialize)]
struct AckRequest {
    /// Short message — "looking into it", "rolling back deploy",
    /// "vendor ticket opened". Surfaces on the dashboard next to
    /// the ACK badge so the rest of the team knows what's happening.
    message: String,
}

/// `POST /v1/health/features/:feature/ack` — operator signals
/// "I'm on this incident". Any admin can ack; the ack carries the
/// actor's user id and message. Idempotent: re-acking the same
/// feature replaces the prior ack with the new one (useful when
/// shift changes hand off an incident). Returns 200 on success
/// regardless of the feature's current state — the ack sticks
/// either way and gets pruned when the refresh task sees recovery.
async fn ack_incident(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(feature): Path<String>,
    Json(req): Json<AckRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.message.trim().is_empty() {
        return Err(ApiError::bad_request("message must not be empty"));
    }
    let who = auth.0.user_id.to_string();
    state.incident_acks.ack(&feature, &who, req.message.trim());
    tracing::info!(
        target: "custody::readiness::ack",
        feature = %feature,
        acknowledged_by = %who,
        "Incident acknowledged"
    );
    Ok(Json(serde_json::json!({ "acknowledged": true })))
}

/// `DELETE /v1/health/features/:feature/ack` — clear manually.
/// Usually not needed because the refresh task prunes acks when a
/// feature returns to Ready, but useful when an operator wants to
/// explicitly un-ack (handed off incorrectly, recorded wrong note).
async fn clear_incident_ack(
    auth: crate::rbac_middleware::RequireAdmin,
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(feature): Path<String>,
) -> Result<StatusCode, ApiError> {
    let cleared = state.incident_acks.clear(&feature);
    if !cleared {
        return Err(ApiError::not_found("No active ack for that feature"));
    }
    tracing::info!(
        target: "custody::readiness::ack",
        feature = %feature,
        cleared_by = %auth.0.user_id,
        "Incident ack cleared"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/health/features/stream` — WebSocket stream of readiness
/// updates. First frame carries the current snapshot; every
/// subsequent frame is the post-replace snapshot from the refresh
/// task. Dashboard subscribes to this and falls back to polling if
/// the WS connection drops (mobile network, load-balancer idle
/// timeout, etc).
///
/// No auth on the stream itself — the payload is already served
/// by the polled endpoint which is also unauthenticated under
/// `router()` mode. When deployed with middleware(), the
/// surrounding layer enforces RBAC.
async fn health_features_stream(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| async move {
        use axum::extract::ws::Message;
        use futures::{SinkExt, StreamExt};

        let (mut sender, mut receiver) = socket.split();
        let mut rx = state.readiness.subscribe();

        // First frame: current snapshot so the client has a baseline
        // before any updates land.
        let initial = state.readiness.snapshot();
        let body = serde_json::json!({
            "summary": summary_of(&initial),
            "features": render_features(&initial, &state.incident_acks.snapshot()),
        });
        if let Ok(txt) = serde_json::to_string(&body)
            && sender.send(Message::Text(txt.into())).await.is_err()
        {
            return;
        }

        loop {
            tokio::select! {
                // Client closed or sent something — we don't accept
                // any frames from the client, just watch for close.
                client = receiver.next() => {
                    match client {
                        Some(Ok(Message::Close(_))) | None => return,
                        _ => continue,
                    }
                }
                update = rx.recv() => {
                    let snap = match update {
                        Ok(s) => s,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Client fell behind — close so it
                            // reconnects and resyncs from the next
                            // initial-snapshot frame.
                            let _ = sender.send(Message::Close(None)).await;
                            return;
                        }
                        Err(_) => return,
                    };
                    let body = serde_json::json!({
                        "summary": summary_of(&snap),
                        "features": render_features(&snap, &state.incident_acks.snapshot()),
                    });
                    let Ok(txt) = serde_json::to_string(&body) else { continue };
                    if sender.send(Message::Text(txt.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    })
}

fn summary_of(report: &custody_common::readiness::ReadinessReport) -> serde_json::Value {
    let (ready, disabled, misc, degraded) = report.summary_counts();
    serde_json::json!({
        "ready": ready,
        "disabled": disabled,
        "misconfigured": misc,
        "degraded": degraded,
        "total": report.features.len(),
    })
}

fn render_features(
    report: &custody_common::readiness::ReadinessReport,
    acks: &std::collections::HashMap<String, custody_common::readiness_ack::IncidentAck>,
) -> Vec<serde_json::Value> {
    report
        .features
        .iter()
        .map(|f| {
            let mut v = serde_json::to_value(f).unwrap_or(serde_json::json!({}));
            if let Some(ack) = acks.get(&f.name) {
                v["ack"] = serde_json::to_value(ack).unwrap_or_default();
            }
            v
        })
        .collect()
}

async fn health_features(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    let snap = state.readiness.snapshot();
    let acks = state.incident_acks.snapshot();
    Json(serde_json::json!({
        "summary": summary_of(&snap),
        "features": render_features(&snap, &acks),
    }))
}

async fn prometheus_metrics() -> String {
    custody_common::prometheus_metrics::render_metrics()
}

async fn health_chains() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "chains": {
            "ethereum": {"status": "unknown"},
            "bitcoin": {"status": "unknown"},
            "tron": {"status": "unknown"},
            "solana": {"status": "unknown"},
        }
    }))
}

async fn create_address(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<CreateAddressRequest>,
) -> Result<Json<CreateAddressResponse>, ApiError> {
    let rid = Uuid::new_v4();
    tracing::info!(rid = %rid, user_id = %req.user_id, asset = %req.asset, chain = %req.chain, "create_address");

    // Refuse to hand out an address we can't monitor. Otherwise a
    // user deposits, the scanner is dark, the deposit is lost.
    require_chain_ready(&state.readiness, req.chain)?;

    // Canonicalize asset code — same rationale as
    // submit_withdrawal. `get_or_create` is idempotent on
    // (user_id, asset, chain); without normalization a caller
    // who alternates case between sessions would get TWO
    // addresses for the same effective asset, both live, both
    // receiving deposits. Dedup at the case boundary.
    let asset_normalized = Asset::new(req.asset.as_str().to_ascii_uppercase());

    let address = state.address_allocator
        .get_or_create(&state.seed, req.user_id, &asset_normalized, req.chain)
        .await
        .map_err(|e| {
            tracing::error!(rid = %rid, error = %e, "Address creation failed");
            ApiError::internal(&e.to_string())
        })?;

    Ok(Json(CreateAddressResponse { address: address.address, chain: req.chain, memo: None }))
}

async fn submit_withdrawal(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<SubmitWithdrawalRequest>,
) -> Result<(StatusCode, Json<SubmitWithdrawalResponse>), ApiError> {
    let rid = Uuid::new_v4();
    tracing::info!(rid = %rid, id = %req.withdrawal_id, asset = %req.asset, "submit_withdrawal");

    // Idempotency check — allowed even when the scanner is down, so a
    // retry sees the same custody_ref.
    let existing = state.db.find_withdrawal_by_exchange_id(req.withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if let Some(existing) = existing {
        return Ok((StatusCode::OK, Json(SubmitWithdrawalResponse {
            custody_ref: existing.id, status: existing.status,
        })));
    }

    // For a fresh submission we need the chain scanner + broadcaster
    // reachable. Accepting a withdrawal we can't broadcast means the
    // row sits in Received forever and the user thinks we're
    // processing.
    require_chain_ready(&state.readiness, req.chain)?;

    // Fail-closed on AML screening. If an operator configured a
    // vendor (Chainalysis / Elliptic) and it's currently unreachable,
    // we refuse to accept the withdrawal rather than silently bypass
    // screening. Explicitly-unconfigured screening (Disabled) is
    // respected — operator opt-out is not the same as a vendor
    // outage. This distinction is the whole reason
    // `require_feature_not_broken` exists instead of a blanket
    // `require_feature`.
    require_feature_not_broken(&state.readiness, "screening.aml")?;

    // Validate and normalise the destination address using the chain-specific checksum.
    let validated_addr = custody_common::address_checksum::validate_address(&req.address, req.chain)
        .map_err(|e| ApiError::bad_request(&format!("Invalid destination address: {e}")))?;

    // Chain-specific memo / destination-tag rules. Centralized in
    // `custody_common::validation::validate_withdrawal_memo` so the
    // batch path and the single-submit path stay in lockstep.
    custody_common::validation::validate_withdrawal_memo(
        req.chain,
        req.memo.as_deref(),
        req.destination_tag,
    )
    .map_err(|e| ApiError::bad_request(&e))?;

    // Canonicalize asset code to uppercase. The rest of the
    // custody stack (ledger keys `hot:ETH`, reconciliation,
    // sweep scheduler, wallet-balance API) uses the uppercase
    // symbol as the single source of truth. Accepting
    // mixed-case input from callers and storing it as-is used
    // to trap funds in a parallel-universe account that no
    // other component queried — a deposit submitted as "eth"
    // wrote ledger entries to `hot:eth`, which reconciliation
    // (`hot:ETH`), sweep, and balance endpoints all missed.
    // Normalize once here at the API boundary so every downstream
    // writer + reader sees the same key.
    let asset_normalized = Asset::new(req.asset.as_str().to_ascii_uppercase());

    let withdrawal = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: req.withdrawal_id,
        user_id: req.user_id,
        vault_id: req.vault_id,
        asset: asset_normalized, chain: req.chain,
        destination_address: validated_addr.normalized, amount: req.amount,
        fee: None, status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None, created_at: Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
        memo: req.memo.clone(),
        destination_tag: req.destination_tag,
        depends_on_withdrawal_id: None,
    };

    state.db.insert_withdrawal(&withdrawal).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Record metric
    custody_common::prometheus_metrics::WITHDRAWALS_TOTAL
        .with_label_values(&[req.chain.as_str(), "received"]).inc();

    Ok((StatusCode::ACCEPTED, Json(SubmitWithdrawalResponse {
        custody_ref: withdrawal.id, status: WithdrawalStatus::Received,
    })))
}

async fn get_withdrawal_status(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WithdrawalStatusResponse>, ApiError> {
    let withdrawal = state.db.find_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("Withdrawal {id} not found")))?;

    Ok(Json(WithdrawalStatusResponse {
        custody_ref: withdrawal.id,
        exchange_withdrawal_id: withdrawal.exchange_withdrawal_id,
        status: withdrawal.status,
        tx_hash: withdrawal.tx_hash,
        confirmations: withdrawal.confirmations,
        required_confirmations: withdrawal.chain.required_confirmations(),
        error: withdrawal.error_message,
    }))
}

/// Request body for withdrawal approval.
///
/// FIDO2 fields (required when withdrawal.amount >=
/// FIDO2_REQUIRED_THRESHOLD_USD):
/// - `fido2_credential_id`: stored credential id
/// - `fido2_challenge`: the challenge string we handed to the
///   authenticator via the begin endpoint
/// - `fido2_sign_count`: monotonically-increasing counter
/// - `fido2_client_data_json`: base64-URL-encoded raw
///   clientDataJSON produced by the browser/authenticator
/// - `fido2_authenticator_data`: base64-URL-encoded raw
///   authenticatorData
/// - `fido2_signature`: base64-URL-encoded raw ES256 signature
#[derive(serde::Deserialize)]
struct ApproveWithdrawalBody {
    approved: bool,
    #[serde(default)]
    approver: Option<String>,
    fido2_credential_id: Option<String>,
    fido2_challenge: Option<String>,
    fido2_sign_count: Option<i64>,
    #[serde(default)]
    fido2_client_data_json: Option<String>,
    #[serde(default)]
    fido2_authenticator_data: Option<String>,
    #[serde(default)]
    fido2_signature: Option<String>,
}

async fn approve_withdrawal(
    auth: RequireSigner,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ApproveWithdrawalBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let withdrawal = state.db.find_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("Withdrawal {id} not found")))?;

    if withdrawal.status != WithdrawalStatus::PendingApproval {
        return Err(ApiError::conflict(&format!(
            "Withdrawal {} is {:?}, not PendingApproval", id, withdrawal.status
        )));
    }

    // CRITICAL: the approver identity MUST come from the
    // authenticated session, never from the request body. The
    // previous handler signature used `_auth: RequireSigner`
    // (authenticated but ignored) and pulled `body.approver` as
    // the approver name. That let a single authenticated signer
    // meet a 2-of-N / 3-of-N quorum by POSTing multiple times
    // with different claimed names — the idempotency check
    // further down compares decision-set entries by string,
    // and each fresh name slipped past it. Tie the approver
    // identity to the session email so one authenticated user
    // can only contribute exactly ONE decision regardless of
    // what they submit in the body.
    //
    // `body.approver` is kept as a request-audit breadcrumb only:
    // if the caller attaches a display name we log it alongside
    // the authoritative email so operators can correlate UI
    // flows without trusting the name for the count.
    let approver = auth.0.email.as_str();
    let claimed_display_name = body.approver.as_deref();
    if let Some(claimed) = claimed_display_name
        && claimed != approver
    {
        tracing::info!(
            id = %id,
            authenticated_as = approver,
            claimed = claimed,
            "Approval body.approver differs from authenticated email — using authenticated identity"
        );
    }

    // Tier 2+ withdrawals ($10K+) require FIDO2 second factor.
    if withdrawal.amount >= FIDO2_REQUIRED_THRESHOLD_USD {
        let cred_id = body.fido2_credential_id.as_deref().ok_or_else(|| {
            ApiError::bad_request(&format!(
                "Withdrawal {} amount {} requires FIDO2 (fido2_credential_id missing)",
                id, withdrawal.amount
            ))
        })?;
        let challenge = body.fido2_challenge.as_deref().ok_or_else(|| {
            ApiError::bad_request(&format!(
                "Withdrawal {} amount {} requires FIDO2 (fido2_challenge missing)",
                id, withdrawal.amount
            ))
        })?;
        let sign_count = body.fido2_sign_count.ok_or_else(|| {
            ApiError::bad_request(&format!(
                "Withdrawal {} amount {} requires FIDO2 (fido2_sign_count missing)",
                id, withdrawal.amount
            ))
        })?;

        // 1. Consume challenge — single-use, checks expiry atomically.
        let consumed = state.db.consume_fido2_challenge(challenge, "withdrawal_approval").await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        if consumed.is_none() {
            tracing::warn!(id = %id, approver, "FIDO2 challenge invalid or expired");
            return Err(ApiError::unauthorized("FIDO2 challenge invalid, expired, or already consumed"));
        }

        // 2. Verify the credential exists and is enabled.
        let credential = state.db.find_fido2_credential_by_cred_id(cred_id).await
            .map_err(|e| ApiError::internal(&e.to_string()))?
            .ok_or_else(|| ApiError::unauthorized("FIDO2 credential not found"))?;

        if !credential.enabled {
            tracing::warn!(id = %id, approver, "FIDO2 credential disabled");
            return Err(ApiError::unauthorized("FIDO2 credential is disabled"));
        }

        // 3. Real WebAuthn verification — parse clientDataJSON,
        //    validate the origin/type/challenge bindings, then
        //    verify the ES256 signature against the stored
        //    credential's COSE public key. Previously this
        //    handler only checked sign_count monotonicity, which
        //    is insufficient on its own — a forged signature with
        //    a fabricated counter would pass. See
        //    `crate::fido2_verify` for the full recipe.
        let client_data = body.fido2_client_data_json.as_deref().ok_or_else(|| {
            ApiError::bad_request("FIDO2 verification requires fido2_client_data_json")
        })?;
        let auth_data = body.fido2_authenticator_data.as_deref().ok_or_else(|| {
            ApiError::bad_request("FIDO2 verification requires fido2_authenticator_data")
        })?;
        let signature = body.fido2_signature.as_deref().ok_or_else(|| {
            ApiError::bad_request("FIDO2 verification requires fido2_signature")
        })?;
        crate::fido2_verify::verify_assertion(
            &credential.public_key_cose,
            client_data,
            auth_data,
            signature,
            challenge,
            &crate::fido2_verify::fido2_expected_origin_default(),
            &crate::fido2_verify::fido2_expected_rp_id_default(),
        )
        .map_err(|e| {
            tracing::warn!(id = %id, approver, error = %e, "FIDO2 signature verification failed");
            ApiError::unauthorized(&format!("FIDO2 verification: {e}"))
        })?;

        // 4. Enforce monotonically increasing sign count (cloned key detection).
        //    update_fido2_sign_count returns false if new_count <= stored count.
        let updated = state.db.update_fido2_sign_count(cred_id, sign_count).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        if !updated {
            tracing::warn!(
                id = %id, approver,
                "FIDO2 sign count did not advance — possible cloned authenticator"
            );
            return Err(ApiError::unauthorized(
                "FIDO2 sign count invalid — authenticator may be cloned"
            ));
        }

        tracing::info!(id = %id, approver, "FIDO2 verification passed for high-value withdrawal");
    }

    // Load the pending approval_request the pipeline created at
    // the PendingApproval transition. Every approve call records a
    // decision here; only when `check_approval_quorum` reports the
    // required count is met do we flip the withdrawal to Validated.
    //
    // Previous behaviour: flip Validated on a single "approved:true"
    // regardless of policy quorum. That was a straight multi-
    // approver bypass — the /v1/withdrawals/{id}/approve endpoint
    // always granted 1-of-N in practice.
    let pending = state.db.find_pending_approval_for_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    let Some((request_id, required, decisions)) = pending else {
        return Err(ApiError::conflict(
            "Withdrawal has no pending approval_request row — cannot attach a decision. \
             The pipeline creates this row on the PendingApproval transition; \
             if it's missing the row was denied, expired, or never created."
        ));
    };

    // Idempotency + double-approval guard: one approver can't pad
    // the count by hitting this endpoint twice with the same
    // identity. Scan existing decisions for a matching approver.
    let already = decisions.as_array()
        .map(|arr| arr.iter().any(|d| d["approver"].as_str() == Some(approver)))
        .unwrap_or(false);
    if already {
        return Err(ApiError::conflict(&format!(
            "Approver '{approver}' has already recorded a decision on withdrawal {id}"
        )));
    }

    // Record the decision.
    state.db.add_approval_decision(request_id, approver, body.approved).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !body.approved {
        // One rejection finalises the whole request — one "no"
        // votes the withdrawal down, matching how most multi-sig
        // policies are understood in custody operations.
        state.db.update_approval_status(request_id, "rejected").await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        state.db.update_withdrawal_status(id, WithdrawalStatus::Cancelled).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        tracing::info!(id = %id, approver, "Withdrawal rejected — approval_request closed");
        return Ok(Json(serde_json::json!({
            "status": "rejected", "withdrawal_id": id,
        })));
    }

    // Approved this round. Check whether the quorum is met.
    let met = state.db.check_approval_quorum(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .unwrap_or(false);

    if met {
        state.db.update_approval_status(request_id, "approved").await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        state.db.update_withdrawal_status(id, WithdrawalStatus::Validated).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        tracing::info!(id = %id, approver, required, "Approval quorum met — moving to Validated");
        Ok(Json(serde_json::json!({
            "status": "approved",
            "withdrawal_id": id,
            "quorum_met": true,
            "required": required,
        })))
    } else {
        // Count the approvals recorded so far (including this one)
        // for the response so the caller can show progress.
        let current = decisions.as_array()
            .map(|arr| arr.iter().filter(|d| d["approved"] == true).count())
            .unwrap_or(0) + 1; // +1 for the one we just added
        tracing::info!(
            id = %id, approver, current, required,
            "Approval recorded but quorum not yet met"
        );
        Ok(Json(serde_json::json!({
            "status": "recorded",
            "withdrawal_id": id,
            "quorum_met": false,
            "approvals_recorded": current,
            "required": required,
        })))
    }
}

async fn get_wallet_balance(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(asset): Path<String>,
) -> Result<Json<WalletBalanceResponse>, ApiError> {
    // Normalize asset code to uppercase for the ledger lookup.
    // The pipeline writes entries under the canonical uppercase
    // symbol ("hot:ETH"); a caller who hit `/v1/wallets/eth`
    // used to get a silent zero balance because the lowercase
    // lookup missed every row. Normalizing here removes the
    // case trap without widening the public API.
    let asset_upper = asset.to_ascii_uppercase();
    let hot = state.db.account_balance(&format!("hot:{asset_upper}"), &asset_upper).await.unwrap_or(rust_decimal::Decimal::ZERO);
    let warm = state.db.account_balance(&format!("warm:{asset_upper}"), &asset_upper).await.unwrap_or(rust_decimal::Decimal::ZERO);
    let cold = state.db.account_balance(&format!("cold:{asset_upper}"), &asset_upper).await.unwrap_or(rust_decimal::Decimal::ZERO);

    Ok(Json(WalletBalanceResponse {
        asset: Asset::new(&asset_upper),
        hot_balance: hot, warm_balance: warm, total_managed: hot + warm + cold,
    }))
}

async fn openapi_spec() -> Json<serde_json::Value> {
    Json(crate::openapi::generate_openapi_v3())
}

async fn sdk_methods() -> Json<Vec<custody_common::developer::SdkMethod>> {
    Json(custody_common::developer::sdk_methods())
}

async fn endpoint_catalog() -> Json<Vec<custody_common::developer::ApiEndpoint>> {
    Json(custody_common::developer::all_endpoints())
}

/// Return the auto-generated Python SDK as plain text.
async fn sdk_python() -> axum::response::Response {
    use axum::http::header;
    let code = custody_common::sdk_generator::generate_python_sdk();
    axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"custody_client.py\"",
        )
        .body(axum::body::Body::from(code))
        .unwrap()
}

/// Return the auto-generated TypeScript SDK as plain text.
async fn sdk_typescript() -> axum::response::Response {
    use axum::http::header;
    let code = custody_common::sdk_generator::generate_typescript_sdk();
    axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"custody_client.ts\"",
        )
        .body(axum::body::Body::from(code))
        .unwrap()
}
