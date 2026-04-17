//! Staking API endpoints — **feature gated off** in this build.
//!
//! The previous implementation accepted stake/unstake requests and
//! replied `{"status": "pending"}` while doing nothing: no signing
//! backend, no broadcast, no persistence. A user wiring this up to
//! real capital would watch their funds appear to enter a validator
//! and never come out, because no on-chain transaction was ever
//! constructed. That is a silent fund-loss path.
//!
//! Until a real staking pipeline lands (validator registry in DB,
//! signing-backend call, withdrawal-pipeline integration for the
//! beacon/delegate transaction, reward accounting), every endpoint
//! here returns **501 Not Implemented** with a clear message. The
//! input-validation helpers in `custody_signing::staking` still run
//! on the write paths so malformed requests are rejected with 400
//! before the 501 — we haven't regressed input validation, only
//! stopped pretending we had an execution layer behind it.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireSigner, RequireViewer};
use crate::state::AppState;

/// Build staking API router.
pub fn staking_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/staking/positions", get(list_positions))
        .route("/v1/staking/stake", post(stake))
        .route("/v1/staking/unstake", post(unstake))
        .route("/v1/staking/rewards", get(list_rewards))
        .route("/v1/staking/validators", get(list_validators))
        .with_state(state)
}

#[derive(Deserialize)]
struct StakeRequest {
    chain: String,
    validator_address: String,
    amount: String,
    asset: String,
}

/// Helper — build a uniform 501 response with a shared hint text.
fn staking_unavailable(detail: &'static str) -> ApiError {
    ApiError {
        error: "not_implemented".into(),
        message: format!(
            "Staking is not wired to a signing/broadcast backend in this build. {detail}"
        ),
        request_id: Uuid::new_v4().to_string(),
        status: StatusCode::NOT_IMPLEMENTED,
    }
}

async fn stake(
    _auth: RequireSigner,
    State(_state): State<AppState>,
    Json(req): Json<StakeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let amount: Decimal = req.amount.parse()
        .map_err(|_| ApiError::bad_request("Invalid amount — must be string-encoded decimal"))?;

    // Run the input validators so a malformed amount still fails with
    // 400 rather than leaking "accepted, will be processed" — useful
    // for integrators testing the shape before the feature lands.
    match req.chain.as_str() {
        "ethereum" | "eth" => {
            custody_signing::staking::validate_eth_stake_amount(amount)
                .map_err(|e| ApiError::bad_request(&e.to_string()))?;
        }
        "solana" | "sol" => {
            let lamports = (amount * Decimal::new(1_000_000_000, 0))
                .to_string().parse::<u64>()
                .map_err(|_| ApiError::bad_request("Invalid SOL amount"))?;
            custody_signing::staking::validate_sol_delegation(lamports)
                .map_err(|e| ApiError::bad_request(&e.to_string()))?;
        }
        chain => return Err(ApiError::bad_request(&format!("Staking not supported for chain: {chain}"))),
    }

    tracing::warn!(
        chain = %req.chain,
        validator = %req.validator_address,
        amount = %amount,
        asset = %req.asset,
        "Staking request rejected — no backend wired (returning 501)"
    );

    Err(staking_unavailable(
        "Stake requests cannot be broadcast. Subscribe to ROADMAP.md staking phase before retrying.",
    ))
}

#[derive(Deserialize)]
struct UnstakeRequest {
    position_id: Uuid,
}

async fn unstake(
    _auth: RequireSigner,
    State(_state): State<AppState>,
    Json(req): Json<UnstakeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::warn!(position_id = %req.position_id, "Unstake rejected — no backend wired (501)");
    Err(staking_unavailable(
        "Unstake requests cannot be broadcast; no positions are tracked to begin with.",
    ))
}

async fn list_positions(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Returning `{"positions": [], "count": 0}` here would suggest the
    // caller simply has no positions; that's a lie when the feature is
    // off. 501 is the correct signal to the dashboard — it can render
    // "staking unavailable in this build" instead of an empty table.
    Err(staking_unavailable(
        "Position listing needs DB-backed staking_positions; not wired.",
    ))
}

async fn list_rewards(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(staking_unavailable(
        "Reward accounting needs on-chain balance deltas per validator; not wired.",
    ))
}

async fn list_validators(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // The previous response returned hardcoded Lido / Marinade rows
    // with APYs that were promotional, not measured — any UI rendering
    // those numbers was lying to the user. Until we pull APY from a
    // live feed and honour a per-workspace whitelist, this endpoint
    // is off.
    Err(staking_unavailable(
        "Validator list was previously hardcoded with promotional APYs; no live feed wired.",
    ))
}
