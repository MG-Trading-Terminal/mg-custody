//! Embedded wallets + contract interactions + payment flows API endpoints.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::embedded_wallets::*;
use custody_common::contract_interactions::*;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::state::AppState;

pub fn embedded_router() -> Router<AppState> {
    Router::new()
        // Embedded wallets
        .route("/v1/embedded-wallets", post(create_embedded_wallet))
        .route("/v1/embedded-wallets/{id}", get(get_embedded_wallet))
        .route("/v1/embedded-wallets/user/{user_id}", get(list_user_wallets))
        // Deployed contracts
        .route("/v1/contracts", get(list_contracts))
        .route("/v1/contracts", post(register_contract))
        .route("/v1/contracts/{address}/{chain}", get(get_contract))
        // Contract calls
        .route("/v1/contracts/call", post(contract_call))
}

// --- Embedded wallets ---

async fn create_embedded_wallet(
    State(state): State<AppState>,
    Json(req): Json<CreateEmbeddedWalletRequest>,
) -> Result<(StatusCode, Json<EmbeddedWallet>), ApiError> {
    let wallet = EmbeddedWallet {
        id: Uuid::new_v4(),
        user_id: req.user_id,
        wallet_type: req.wallet_type,
        status: WalletStatus::Active,
        accounts: vec![], // accounts created on demand
        created_at: Utc::now(),
        last_active_at: None,
    };

    state.db.insert_embedded_wallet(&wallet).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %wallet.id, "Embedded wallet created");
    Ok((StatusCode::CREATED, Json(wallet)))
}

async fn get_embedded_wallet(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EmbeddedWallet>, ApiError> {
    let wallet = state.db.find_embedded_wallet(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Embedded wallet not found"))?;
    Ok(Json(wallet))
}

async fn list_user_wallets(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<EmbeddedWallet>>, ApiError> {
    let wallets = state.db.list_embedded_wallets_by_user(&user_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(wallets))
}

// --- Deployed contracts ---

async fn list_contracts(
    State(state): State<AppState>,
) -> Result<Json<Vec<DeployedContract>>, ApiError> {
    let contracts = state.db.list_deployed_contracts(None).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(contracts))
}

async fn register_contract(
    State(state): State<AppState>,
    Json(contract): Json<DeployedContract>,
) -> Result<(StatusCode, Json<DeployedContract>), ApiError> {
    state.db.insert_deployed_contract(&contract).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(address = %contract.address, chain = %contract.chain, "Contract registered");
    Ok((StatusCode::CREATED, Json(contract)))
}

async fn get_contract(
    State(state): State<AppState>,
    Path((address, chain)): Path<(String, String)>,
) -> Result<Json<DeployedContract>, ApiError> {
    let contract = state.db.find_deployed_contract(&address, &chain).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Contract not found"))?;
    Ok(Json(contract))
}

// --- Contract calls ---

async fn contract_call(
    Json(req): Json<ContractCallRequest>,
) -> Result<Json<ContractCallResult>, ApiError> {
    if req.is_read_only {
        // For read-only: would call eth_call via RPC
        Ok(Json(ContractCallResult {
            success: true,
            return_data: Some(serde_json::json!("0x")),
            tx_hash: None,
            gas_used: None,
            error: None,
        }))
    } else {
        // For write: would build transaction, sign, broadcast
        Ok(Json(ContractCallResult {
            success: true,
            return_data: None,
            tx_hash: Some("pending".into()),
            gas_used: Some(65000),
            error: None,
        }))
    }
}
