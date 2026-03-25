//! API route handlers.
//!
//! POST /v1/addresses         — create deposit address
//! POST /v1/withdrawals       — submit withdrawal
//! GET  /v1/withdrawals/:id   — check withdrawal status
//! GET  /v1/wallets/:asset    — wallet balance
//! GET  /health               — health check

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::*;
use uuid::Uuid;

use crate::state::AppState;

/// Build the API router with shared state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/chains", get(health_chains))
        .route("/v1/addresses", post(create_address))
        .route("/v1/withdrawals", post(submit_withdrawal))
        .route("/v1/withdrawals/{id}", get(get_withdrawal_status))
        .route("/v1/withdrawals/{id}/approve", post(approve_withdrawal))
        .route("/v1/wallets/{asset}", get(get_wallet_balance))
        .route("/metrics", get(prometheus_metrics))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn create_address(
    State(state): State<AppState>,
    Json(req): Json<CreateAddressRequest>,
) -> Result<Json<CreateAddressResponse>, StatusCode> {
    tracing::info!(user_id = %req.user_id, asset = %req.asset, chain = %req.chain, "create_address");

    let address = state
        .address_allocator
        .get_or_create(&state.seed, req.user_id, &req.asset, req.chain)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Address creation failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(CreateAddressResponse {
        address: address.address,
        chain: req.chain,
        memo: None,
    }))
}

async fn submit_withdrawal(
    State(state): State<AppState>,
    Json(req): Json<SubmitWithdrawalRequest>,
) -> Result<(StatusCode, Json<SubmitWithdrawalResponse>), StatusCode> {
    tracing::info!(id = %req.withdrawal_id, asset = %req.asset, "submit_withdrawal");

    // Idempotency: check if this withdrawal already exists
    let existing = state
        .db
        .find_withdrawal_by_exchange_id(req.withdrawal_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "DB lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some(existing) = existing {
        return Ok((
            StatusCode::OK,
            Json(SubmitWithdrawalResponse {
                custody_ref: existing.id,
                status: existing.status,
            }),
        ));
    }

    // Create new withdrawal in Received status
    let withdrawal = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: req.withdrawal_id,
        asset: req.asset.clone(),
        chain: req.chain,
        destination_address: req.address,
        amount: req.amount,
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None,
        block_height: None,
        confirmations: 0,
        error_message: None,
        created_at: Utc::now(),
        signed_at: None,
        broadcast_at: None,
        confirmed_at: None,
        delay_release_at: None,
    };

    state.db.insert_withdrawal(&withdrawal).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to insert withdrawal");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(SubmitWithdrawalResponse {
            custody_ref: withdrawal.id,
            status: WithdrawalStatus::Received,
        }),
    ))
}

async fn get_withdrawal_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WithdrawalStatusResponse>, StatusCode> {
    tracing::info!(id = %id, "get_withdrawal_status");

    let withdrawal = state
        .db
        .find_withdrawal(id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "DB lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

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

async fn get_wallet_balance(
    State(_state): State<AppState>,
    Path(asset): Path<String>,
) -> Result<Json<WalletBalanceResponse>, StatusCode> {
    tracing::info!(asset = %asset, "get_wallet_balance");

    // TODO: Wire to wallet manager / DB balance query
    Ok(Json(WalletBalanceResponse {
        asset: Asset::new(&asset),
        hot_balance: rust_decimal::Decimal::ZERO,
        warm_balance: rust_decimal::Decimal::ZERO,
        total_managed: rust_decimal::Decimal::ZERO,
    }))
}

/// Approve a withdrawal (for PendingApproval status).
async fn approve_withdrawal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    tracing::info!(id = %id, "approve_withdrawal");

    let withdrawal = state
        .db
        .find_withdrawal(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if withdrawal.status != WithdrawalStatus::PendingApproval {
        return Err(StatusCode::CONFLICT);
    }

    let approved = body.get("approved").and_then(|v| v.as_bool()).unwrap_or(false);
    let approver = body.get("approver").and_then(|v| v.as_str()).unwrap_or("unknown");

    if approved {
        // Move to Validated so pipeline picks it up for signing
        state
            .db
            .update_withdrawal_status(id, WithdrawalStatus::Validated)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        tracing::info!(id = %id, approver, "Withdrawal approved");
        Ok(Json(serde_json::json!({ "status": "approved", "withdrawal_id": id })))
    } else {
        state
            .db
            .update_withdrawal_status(id, WithdrawalStatus::Cancelled)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        tracing::info!(id = %id, approver, "Withdrawal rejected");
        Ok(Json(serde_json::json!({ "status": "rejected", "withdrawal_id": id })))
    }
}

/// Prometheus metrics endpoint.
async fn prometheus_metrics() -> String {
    custody_common::prometheus_metrics::render_metrics()
}

/// Per-chain health check.
async fn health_chains() -> Json<serde_json::Value> {
    // TODO: Actually query each chain's RPC for block height and latency
    Json(serde_json::json!({
        "chains": {
            "ethereum": { "status": "unknown", "block_height": 0 },
            "bitcoin": { "status": "unknown", "block_height": 0 },
            "tron": { "status": "unknown", "block_height": 0 },
            "solana": { "status": "unknown", "block_height": 0 },
        }
    }))
}
