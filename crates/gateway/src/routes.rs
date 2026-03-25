//! API route handlers with structured JSON errors and request tracing.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::*;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
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
        .with_state(state)
}

pub fn router_with_middleware(state: AppState, hmac_secret: &[u8]) -> Router {
    use axum::middleware as axum_mw;
    use crate::middleware::{self, MiddlewareState};
    let mw_state = MiddlewareState::new(hmac_secret);
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
        .layer(axum_mw::from_fn_with_state(mw_state.clone(), middleware::rate_limit))
        .layer(axum_mw::from_fn_with_state(mw_state, middleware::hmac_auth))
        .with_state(state)
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

    Ok((StatusCode::ACCEPTED, Json(SubmitWithdrawalResponse {
        custody_ref: withdrawal.id, status: WithdrawalStatus::Received,
    })))
}

async fn get_withdrawal_status(
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

async fn approve_withdrawal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let withdrawal = state.db.find_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("Withdrawal {id} not found")))?;

    if withdrawal.status != WithdrawalStatus::PendingApproval {
        return Err(ApiError::conflict(&format!(
            "Withdrawal is {:?}, not PendingApproval", withdrawal.status
        )));
    }

    let approved = body.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
    let approver = body.get("approver").and_then(|v| v.as_str()).unwrap_or("unknown");

    if approved {
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
