//! Embedded wallets + contract interactions + payment flows API endpoints.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::analytics::TransactionLabel;
use custody_common::embedded_wallets::*;
use custody_common::contract_interactions::*;
use custody_common::payment_flows::*;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireSigner, RequireViewer};
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
        // Payment flows
        .route("/v1/payment-flows", get(list_payment_flows))
        .route("/v1/payment-flows", post(create_payment_flow))
        .route("/v1/payment-flows/{id}", get(get_payment_flow))
        // Transaction labels
        .route("/v1/transactions/{id}/labels", get(get_tx_labels))
        .route("/v1/transactions/{id}/labels", axum::routing::put(set_tx_labels))
}

// --- Embedded wallets ---

async fn create_embedded_wallet(
    _auth: RequireEditor,
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
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EmbeddedWallet>, ApiError> {
    let wallet = state.db.find_embedded_wallet(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Embedded wallet not found"))?;
    Ok(Json(wallet))
}

async fn list_user_wallets(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<EmbeddedWallet>>, ApiError> {
    let wallets = state.db.list_embedded_wallets_by_user(&user_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(wallets))
}

// --- Deployed contracts ---

async fn list_contracts(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<DeployedContract>>, ApiError> {
    let contracts = state.db.list_deployed_contracts(None).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(contracts))
}

async fn register_contract(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Json(contract): Json<DeployedContract>,
) -> Result<(StatusCode, Json<DeployedContract>), ApiError> {
    state.db.insert_deployed_contract(&contract).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(address = %contract.address, chain = %contract.chain, "Contract registered");
    Ok((StatusCode::CREATED, Json(contract)))
}

async fn get_contract(
    _auth: RequireViewer,
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
    _auth: RequireSigner,
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

// --- Payment flows ---

async fn list_payment_flows(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<PaymentFlow>>, ApiError> {
    let flows = state.db.list_payment_flows().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(flows))
}

async fn create_payment_flow(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(flow): Json<PaymentFlow>,
) -> Result<(StatusCode, Json<PaymentFlow>), ApiError> {
    state.db.insert_payment_flow(&flow).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    tracing::info!(id = %flow.id, name = %flow.name, "Payment flow created");
    Ok((StatusCode::CREATED, Json(flow)))
}

async fn get_payment_flow(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PaymentFlow>, ApiError> {
    let flow = state.db.find_payment_flow(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Payment flow not found"))?;
    Ok(Json(flow))
}

// --- Transaction labels ---

async fn get_tx_labels(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let label = state.db.get_transaction_label(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    match label {
        Some(l) => Ok(Json(serde_json::to_value(&l).unwrap())),
        None => Ok(Json(serde_json::json!({"transaction_id": id, "labels": []}))),
    }
}

async fn set_tx_labels(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(label): Json<TransactionLabel>,
) -> Result<Json<TransactionLabel>, ApiError> {
    let mut label = label;
    label.transaction_id = id;
    label.updated_at = Utc::now();
    state.db.upsert_transaction_label(&label).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(label))
}
