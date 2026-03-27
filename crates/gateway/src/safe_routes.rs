//! Gnosis Safe multi-sig wallet API routes.
//!
//! Endpoints:
//!   POST   /v1/safe-wallets                                — register a Safe (Admin)
//!   GET    /v1/safe-wallets                                — list Safe wallets (Viewer)
//!   POST   /v1/safe-wallets/{address}/transactions         — propose tx (Editor)
//!   POST   /v1/safe-wallets/{address}/transactions/{id}/sign    — add signature (Signer)
//!   GET    /v1/safe-wallets/{address}/transactions         — list transactions (Viewer)
//!   POST   /v1/safe-wallets/{address}/transactions/{id}/execute — execute ready tx (Signer)

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use custody_common::safe_executor::{
    AddSafeSignatureRequest, ProposeSafeTxRequest, RegisterSafeRequest,
    SafeManagedTx, SafeManagedTxResponse, SafeWallet, SafeWalletResponse,
    SafeTxStatus, compute_safe_tx_hash,
};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireSigner, RequireViewer};
use crate::state::AppState;

pub fn safe_router() -> Router<AppState> {
    Router::new()
        .route("/v1/safe-wallets", post(register_safe).get(list_safes))
        .route("/v1/safe-wallets/{address}/transactions", post(propose_transaction).get(list_transactions))
        .route("/v1/safe-wallets/{address}/transactions/{id}/sign", post(add_signature))
        .route("/v1/safe-wallets/{address}/transactions/{id}/execute", post(execute_transaction))
}

// ---------------------------------------------------------------------------
// POST /v1/safe-wallets
// ---------------------------------------------------------------------------

/// Register a Gnosis Safe wallet with its address, chain, threshold and owners.
async fn register_safe(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<RegisterSafeRequest>,
) -> Result<(StatusCode, Json<SafeWalletResponse>), ApiError> {
    if req.threshold == 0 {
        return Err(ApiError::bad_request("threshold must be >= 1"));
    }
    if req.owners.is_empty() {
        return Err(ApiError::bad_request("owners list must not be empty"));
    }
    if req.threshold as usize > req.owners.len() {
        return Err(ApiError::bad_request(
            "threshold cannot exceed number of owners",
        ));
    }
    if req.address.len() != 42 || !req.address.starts_with("0x") {
        return Err(ApiError::bad_request(
            "address must be a 42-character 0x-prefixed EIP-55 address",
        ));
    }

    let wallet = SafeWallet::new(
        req.address.clone(),
        req.chain,
        req.threshold,
        req.owners,
        req.nonce,
    );

    state.db.upsert_safe_wallet(&wallet).await.map_err(|e| {
        tracing::error!(address = %req.address, error = %e, "Failed to store Safe wallet");
        ApiError::internal(&e.to_string())
    })?;

    tracing::info!(
        address = %wallet.address,
        chain = %wallet.chain,
        threshold = wallet.threshold,
        owners = wallet.owners.len(),
        "Safe wallet registered"
    );

    Ok((StatusCode::CREATED, Json(wallet.into())))
}

// ---------------------------------------------------------------------------
// GET /v1/safe-wallets
// ---------------------------------------------------------------------------

async fn list_safes(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<SafeWalletResponse>>, ApiError> {
    let wallets = state.db.list_safe_wallets().await.map_err(|e| {
        ApiError::internal(&format!("safe wallet query failed: {e}"))
    })?;

    Ok(Json(wallets.into_iter().map(SafeWalletResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// POST /v1/safe-wallets/{address}/transactions
// ---------------------------------------------------------------------------

async fn propose_transaction(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(address): Path<String>,
    Json(req): Json<ProposeSafeTxRequest>,
) -> Result<(StatusCode, Json<SafeManagedTxResponse>), ApiError> {
    if req.value_wei <= rust_decimal::Decimal::ZERO && req.data_hex.trim_start_matches("0x").is_empty() {
        return Err(ApiError::bad_request(
            "transaction must have non-zero value or non-empty calldata",
        ));
    }

    let wallet = load_wallet(&state, &address).await?;

    let data = decode_hex_data(&req.data_hex)
        .map_err(|e| ApiError::bad_request(&format!("bad data_hex: {e}")))?;

    let tx = wallet.propose_transaction(
        req.to.clone(),
        req.value_wei,
        data,
        req.operation,
        req.chain_id,
    );

    state.db.insert_safe_tx(&tx).await.map_err(|e| {
        tracing::error!(
            safe = %address,
            to = %req.to,
            error = %e,
            "Failed to store proposed Safe tx"
        );
        ApiError::internal(&e.to_string())
    })?;

    tracing::info!(
        safe = %address,
        tx_id = %tx.id,
        to = %req.to,
        safe_tx_hash = %tx.safe_tx_hash,
        "Safe transaction proposed"
    );

    Ok((StatusCode::CREATED, Json(tx.into())))
}

// ---------------------------------------------------------------------------
// POST /v1/safe-wallets/{address}/transactions/{id}/sign
// ---------------------------------------------------------------------------

async fn add_signature(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path((address, id)): Path<(String, Uuid)>,
    Json(req): Json<AddSafeSignatureRequest>,
) -> Result<Json<SafeManagedTxResponse>, ApiError> {
    let wallet = load_wallet(&state, &address).await?;
    let mut tx = load_tx(&state, id).await?;

    verify_tx_belongs_to_safe(&tx, &address)?;

    let sig_bytes = decode_hex_data(&req.signature_hex)
        .map_err(|e| ApiError::bad_request(&format!("bad signature_hex: {e}")))?;

    tx.add_signature(req.signer.clone(), sig_bytes, &wallet)
        .map_err(|e| ApiError::bad_request(&e.to_string()))?;

    state.db.update_safe_tx(&tx).await.map_err(|e| {
        tracing::error!(tx_id = %id, error = %e, "Failed to update Safe tx after signature");
        ApiError::internal(&e.to_string())
    })?;

    tracing::info!(
        safe = %address,
        tx_id = %id,
        signer = %req.signer,
        signatures = tx.signatures.len(),
        status = ?tx.status,
        "Signature added to Safe tx"
    );

    Ok(Json(tx.into()))
}

// ---------------------------------------------------------------------------
// GET /v1/safe-wallets/{address}/transactions
// ---------------------------------------------------------------------------

async fn list_transactions(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Vec<SafeManagedTxResponse>>, ApiError> {
    let txs = state.db.list_safe_txs(&address).await.map_err(|e| {
        ApiError::internal(&format!("safe tx query failed: {e}"))
    })?;

    Ok(Json(txs.into_iter().map(SafeManagedTxResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// POST /v1/safe-wallets/{address}/transactions/{id}/execute
// ---------------------------------------------------------------------------

async fn execute_transaction(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path((address, id)): Path<(String, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Load wallet to confirm it exists and the address is valid.
    let _wallet = load_wallet(&state, &address).await?;
    let mut tx = load_tx(&state, id).await?;

    verify_tx_belongs_to_safe(&tx, &address)?;

    if tx.status != SafeTxStatus::Ready {
        return Err(ApiError::conflict(&format!(
            "Transaction {} is {:?}; must be Ready to execute",
            id, tx.status
        )));
    }

    let calldata = tx.encode_exec().map_err(|e| {
        ApiError::internal(&format!("exec encoding failed: {e}"))
    })?;

    // Mark as executed in our records. The actual on-chain broadcast would go
    // through the withdrawal pipeline in a full implementation.
    tx.status = SafeTxStatus::Executed;
    state.db.update_safe_tx(&tx).await.map_err(|e| {
        tracing::error!(tx_id = %id, error = %e, "Failed to mark Safe tx executed");
        ApiError::internal(&e.to_string())
    })?;

    tracing::info!(
        safe = %address,
        tx_id = %id,
        safe_tx_hash = %tx.safe_tx_hash,
        calldata_len = calldata.len(),
        "Safe transaction ready for broadcast"
    );

    Ok(Json(serde_json::json!({
        "status": "executed",
        "tx_id": id,
        "safe_tx_hash": tx.safe_tx_hash,
        "calldata_hex": format!("0x{}", hex::encode(&calldata)),
    })))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

async fn load_wallet(state: &AppState, address: &str) -> Result<SafeWallet, ApiError> {
    state.db.find_safe_wallet(address).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("Safe wallet {address} not found")))
}

async fn load_tx(state: &AppState, id: Uuid) -> Result<SafeManagedTx, ApiError> {
    state.db.find_safe_tx(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found(&format!("Safe transaction {id} not found")))
}

fn verify_tx_belongs_to_safe(tx: &SafeManagedTx, address: &str) -> Result<(), ApiError> {
    if !tx.safe_address.eq_ignore_ascii_case(address) {
        return Err(ApiError::not_found(&format!(
            "Transaction {} does not belong to Safe {address}",
            tx.id
        )));
    }
    Ok(())
}

fn decode_hex_data(hex_str: &str) -> Result<Vec<u8>, String> {
    let stripped = hex_str.trim_start_matches("0x");
    if stripped.is_empty() {
        return Ok(vec![]);
    }
    hex::decode(stripped).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_data_empty_0x() {
        assert_eq!(decode_hex_data("0x").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_hex_data("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decode_hex_data_with_prefix() {
        let data = decode_hex_data("0xdeadbeef").unwrap();
        assert_eq!(data, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn decode_hex_data_without_prefix() {
        let data = decode_hex_data("deadbeef").unwrap();
        assert_eq!(data, vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn decode_hex_data_invalid_errors() {
        assert!(decode_hex_data("0xGGGG").is_err());
        assert!(decode_hex_data("odd_length1").is_err());
    }
}
