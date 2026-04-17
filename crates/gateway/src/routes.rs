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

use crate::errors::{require_chain_ready, ApiError};
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
async fn health_features(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    let (ready, disabled, misc, degraded) = state.readiness.summary_counts();
    Json(serde_json::json!({
        "summary": {
            "ready": ready,
            "disabled": disabled,
            "misconfigured": misc,
            "degraded": degraded,
            "total": state.readiness.features.len(),
        },
        "features": state.readiness.features,
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

    let address = state.address_allocator
        .get_or_create(&state.seed, req.user_id, &req.asset, req.chain)
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

    let withdrawal = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: req.withdrawal_id,
        user_id: req.user_id,
        vault_id: req.vault_id,
        asset: req.asset.clone(), chain: req.chain,
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
#[derive(serde::Deserialize)]
struct ApproveWithdrawalBody {
    approved: bool,
    #[serde(default)]
    approver: Option<String>,
    // Required when withdrawal.amount >= FIDO2_REQUIRED_THRESHOLD_USD
    fido2_credential_id: Option<String>,
    fido2_challenge: Option<String>,
    fido2_sign_count: Option<i64>,
}

async fn approve_withdrawal(
    _auth: RequireSigner,
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

    let approver = body.approver.as_deref().unwrap_or("unknown");

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

        // 3. Enforce monotonically increasing sign count (cloned key detection).
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

    if body.approved {
        state.db.update_withdrawal_status(id, WithdrawalStatus::Validated).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        tracing::info!(id = %id, approver, "Withdrawal approved");
        Ok(Json(serde_json::json!({"status": "approved", "withdrawal_id": id})))
    } else {
        state.db.update_withdrawal_status(id, WithdrawalStatus::Cancelled).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        tracing::info!(id = %id, approver, "Withdrawal rejected");
        Ok(Json(serde_json::json!({"status": "rejected", "withdrawal_id": id})))
    }
}

async fn get_wallet_balance(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(asset): Path<String>,
) -> Result<Json<WalletBalanceResponse>, ApiError> {
    let hot = state.db.account_balance(&format!("hot:{asset}"), &asset).await.unwrap_or(rust_decimal::Decimal::ZERO);
    let warm = state.db.account_balance(&format!("warm:{asset}"), &asset).await.unwrap_or(rust_decimal::Decimal::ZERO);
    let cold = state.db.account_balance(&format!("cold:{asset}"), &asset).await.unwrap_or(rust_decimal::Decimal::ZERO);

    Ok(Json(WalletBalanceResponse {
        asset: Asset::new(&asset),
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
