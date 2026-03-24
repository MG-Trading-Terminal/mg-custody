//! API route handlers.
//!
//! POST /v1/addresses         — create deposit address
//! POST /v1/withdrawals       — submit withdrawal
//! GET  /v1/withdrawals/:id   — check withdrawal status
//! GET  /v1/wallets/:asset    — wallet balance
//! GET  /health               — health check

use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use custody_common::*;

/// Build the API router.
pub fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/addresses", post(create_address))
        .route("/v1/withdrawals", post(submit_withdrawal))
        .route("/v1/withdrawals/{id}", get(get_withdrawal_status))
        .route("/v1/wallets/{asset}", get(get_wallet_balance))
}

async fn health() -> &'static str {
    "ok"
}

async fn create_address(
    Json(req): Json<CreateAddressRequest>,
) -> Result<Json<CreateAddressResponse>, StatusCode> {
    // TODO: wire to address factory
    tracing::info!(user_id = %req.user_id, asset = %req.asset, chain = %req.chain, "create_address");
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn submit_withdrawal(
    Json(req): Json<SubmitWithdrawalRequest>,
) -> Result<(StatusCode, Json<SubmitWithdrawalResponse>), StatusCode> {
    // TODO: wire to withdrawal pipeline
    tracing::info!(id = %req.withdrawal_id, asset = %req.asset, "submit_withdrawal");
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn get_withdrawal_status(
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<WithdrawalStatusResponse>, StatusCode> {
    // TODO: wire to DB lookup
    tracing::info!(id = %id, "get_withdrawal_status");
    Err(StatusCode::NOT_IMPLEMENTED)
}

async fn get_wallet_balance(
    Path(asset): Path<String>,
) -> Result<Json<WalletBalanceResponse>, StatusCode> {
    // TODO: wire to wallet manager
    tracing::info!(asset = %asset, "get_wallet_balance");
    Err(StatusCode::NOT_IMPLEMENTED)
}
