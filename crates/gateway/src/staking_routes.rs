//! Staking API endpoints.
//!
//! Manage staking positions: stake, unstake, list positions, rewards.

use axum::{
    Json,
    extract::State,
    routing::{get, post},
    Router,
};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::errors::ApiError;
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
    amount: String, // string-encoded decimal
    asset: String,
}

async fn stake(
    State(_state): State<AppState>,
    Json(req): Json<StakeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let amount: Decimal = req.amount.parse()
        .map_err(|_| ApiError::bad_request("Invalid amount — must be string-encoded decimal"))?;

    // Validate based on chain
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

    let position_id = uuid::Uuid::new_v4();

    tracing::info!(
        chain = %req.chain,
        validator = %req.validator_address,
        amount = %amount,
        position_id = %position_id,
        "Staking request submitted"
    );

    Ok(Json(serde_json::json!({
        "position_id": position_id,
        "chain": req.chain,
        "validator": req.validator_address,
        "amount": amount.to_string(),
        "asset": req.asset,
        "status": "pending",
    })))
}

#[derive(Deserialize)]
struct UnstakeRequest {
    position_id: uuid::Uuid,
}

async fn unstake(
    State(_state): State<AppState>,
    Json(req): Json<UnstakeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!(position_id = %req.position_id, "Unstake request");

    Ok(Json(serde_json::json!({
        "position_id": req.position_id,
        "status": "unstaking",
        "message": "Unstaking initiated. Cooldown period applies.",
    })))
}

async fn list_positions(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    // In production: query staking_positions table
    Json(serde_json::json!({
        "positions": [],
        "count": 0,
    }))
}

async fn list_rewards(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "rewards": [],
        "total_earned": "0",
    }))
}

async fn list_validators(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    // Pre-configured safe validators
    Json(serde_json::json!({
        "validators": [
            {
                "chain": "ethereum",
                "name": "Lido",
                "address": "lido",
                "apy_pct": "3.6",
                "commission_pct": "10",
                "safe": true,
            },
            {
                "chain": "solana",
                "name": "Marinade",
                "address": "marinade",
                "apy_pct": "7.0",
                "commission_pct": "6",
                "safe": true,
            },
        ],
    }))
}
