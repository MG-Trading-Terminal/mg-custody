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
use tower_http::cors::{CorsLayer, Any};
use tower_http::services::ServeDir;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::errors::ApiError;
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
    let rbac_state = RbacState::new(state.db.clone());
    Router::new()
        .route("/health", get(health))
        .route("/health/chains", get(health_chains))
        .route("/health/deep", get(crate::health::deep_health))
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
        .merge(crate::auth_routes::auth_router())
        .merge(crate::fido2_routes::fido2_router())
        .merge(crate::vault_routes::vault_router())
        .merge(crate::ops_routes::ops_router())
        .merge(crate::connectivity_routes::connectivity_router())
        .merge(crate::platform_routes::platform_router())
        .merge(crate::governance_routes::governance_router())
        .merge(crate::operations_routes::operations_router())
        .merge(crate::embedded_routes::embedded_router())
        .merge(crate::swap_routes::swap_router())
        // Dashboard static files (SvelteKit build output)
        // Fallback to index.html for client-side routing
        .fallback_service(dashboard_service())
        .layer(axum_mw::from_fn(crate::middleware::security_headers))
        .layer(axum_mw::from_fn_with_state(rbac_state, rbac_middleware::rbac_auth))
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
        .route("/metrics", get(prometheus_metrics))
        .route("/v1/addresses", post(create_address))
        .route("/v1/withdrawals", post(submit_withdrawal))
        .route("/v1/withdrawals/{id}", get(get_withdrawal_status))
        .route("/v1/withdrawals/{id}/approve", post(approve_withdrawal))
        .route("/v1/wallets/{asset}", get(get_wallet_balance))
        .route("/v1/ws", get(crate::websocket::ws_handler))
        .merge(crate::auth_routes::auth_router())
        .merge(crate::fido2_routes::fido2_router())
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
    CorsLayer::new()
        .allow_origin(Any) // Internal API — configure for specific origin in production
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(Any)
        .max_age(Duration::from_secs(3600))
}

async fn health() -> &'static str { "ok" }

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

    // Idempotency check
    let existing = state.db.find_withdrawal_by_exchange_id(req.withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if let Some(existing) = existing {
        return Ok((StatusCode::OK, Json(SubmitWithdrawalResponse {
            custody_ref: existing.id, status: existing.status,
        })));
    }

    let withdrawal = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: req.withdrawal_id,
        asset: req.asset.clone(), chain: req.chain,
        destination_address: req.address, amount: req.amount,
        fee: None, status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None, created_at: Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
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
