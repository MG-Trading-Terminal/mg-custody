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
    routing::{delete, get, post},
};
use custody_common::safe_executor::{
    AddSafeSignatureRequest, ProposeSafeTxRequest, RegisterSafeRequest,
    SafeManagedTx, SafeManagedTxResponse, SafeWallet, SafeWalletResponse,
    SafeTxStatus, compute_safe_tx_hash,
};
use serde::{Deserialize, Serialize};
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
        .route("/v1/safe-wallets/{address}/members", post(add_member).get(list_members))
        .route(
            "/v1/safe-wallets/{address}/members/{owner_address}",
            delete(remove_member),
        )
}

// ---------------------------------------------------------------------------
// POST /v1/safe-wallets
// ---------------------------------------------------------------------------

/// Register a Gnosis Safe wallet with its address, chain, threshold and owners.
async fn register_safe(
    auth: RequireAdmin,
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

    let wallet = SafeWallet::new_owned(
        req.address.clone(),
        req.chain,
        req.threshold,
        req.owners,
        req.nonce,
        auth.0.user_id,
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

/// Proposing a Safe transaction is gated at Admin level AND scoped to the
/// admin who registered the Safe (migration 029). Owner role is always
/// allowed — the owner oversees all Safes in the workspace.
async fn propose_transaction(
    auth: RequireAdmin,
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
    assert_safe_access(&state, &auth.0, &wallet).await?;

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
    auth: RequireSigner,
    State(state): State<AppState>,
    Path((address, id)): Path<(String, Uuid)>,
    Json(req): Json<AddSafeSignatureRequest>,
) -> Result<Json<SafeManagedTxResponse>, ApiError> {
    let wallet = load_wallet(&state, &address).await?;
    let linked_owners = assert_safe_access(&state, &auth.0, &wallet).await?;
    let mut tx = load_tx(&state, id).await?;

    verify_tx_belongs_to_safe(&tx, &address)?;

    // Signer-identity check. Workspace Owner can still proxy any
    // signer (e.g., submitting a pre-collected co-signature from a
    // HSM), but a plain member MUST be signing from one of their own
    // linked owner addresses — no lateral signing on behalf of others.
    let is_workspace_owner = auth
        .0
        .role
        .has_at_least(&custody_db::sessions::Role::Owner);
    if !is_workspace_owner {
        let signer_lower = req.signer.to_lowercase();
        if !linked_owners.iter().any(|a| a == &signer_lower) {
            return Err(ApiError::forbidden(
                "Signer address is not one of your linked Safe owner addresses",
            ));
        }
    }

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
    auth: RequireSigner,
    State(state): State<AppState>,
    Path((address, id)): Path<(String, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Load wallet to confirm it exists and the address is valid.
    let wallet = load_wallet(&state, &address).await?;
    assert_safe_access(&state, &auth.0, &wallet).await?;
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

/// Ownership check for Safe operations.
///
/// Access precedence:
///   1. Workspace Owner — always.
///   2. User linked via `safe_wallet_members` to at least one of the
///      Safe's on-chain owner addresses. This is the membership path.
///   3. Admin who registered the Safe (`registered_by` match) — kept
///      so a fresh registration still works before members are linked.
///   4. Legacy wallets with `registered_by = NULL` — admin-open to
///      keep pre-migration data reachable. Operators should re-register.
///
/// Returns the list of owner addresses (lowercased) that the caller
/// controls on this Safe. `add_signature` uses it to verify that
/// `req.signer` is actually one of the caller's linked addresses,
/// preventing a member from signing on behalf of a co-owner.
async fn assert_safe_access(
    state: &AppState,
    auth: &crate::rbac_middleware::AuthContext,
    wallet: &SafeWallet,
) -> Result<Vec<String>, ApiError> {
    if auth.role.has_at_least(&custody_db::sessions::Role::Owner) {
        // Owner can operate on any registered Safe. We still return
        // the user's linked owner addresses (if any) so signing
        // endpoints can enforce signer-identity without special-casing.
        let linked = state
            .db
            .user_safe_owner_addresses(&wallet.address, wallet.chain.as_str(), auth.user_id)
            .await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        return Ok(linked);
    }

    let linked = state
        .db
        .user_safe_owner_addresses(&wallet.address, wallet.chain.as_str(), auth.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !linked.is_empty() {
        return Ok(linked);
    }

    match wallet.registered_by {
        None => Ok(Vec::new()), // legacy row — admin can still operate
        Some(id) if id == auth.user_id => Ok(Vec::new()),
        Some(_) => Err(ApiError::forbidden(
            "Not a member of this Safe — ask an Admin to link your on-chain owner address",
        )),
    }
}

fn decode_hex_data(hex_str: &str) -> Result<Vec<u8>, String> {
    let stripped = hex_str.trim_start_matches("0x");
    if stripped.is_empty() {
        return Ok(vec![]);
    }
    hex::decode(stripped).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Safe members (on-chain owner address → workspace user ID mapping)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AddMemberRequest {
    /// On-chain Safe owner address (EIP-55 or lowercase; normalised on
    /// insert so lookups are exact).
    owner_address: String,
    user_id: Uuid,
}

#[derive(Serialize)]
struct MemberResponse {
    owner_address: String,
    user_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// POST /v1/safe-wallets/{address}/members
///
/// Link an on-chain Safe owner address to a workspace user. Admin-only.
/// The supplied `owner_address` MUST currently appear in `wallet.owners`
/// — linking an address the Safe doesn't recognise would be an
/// evidence-free permission grant.
async fn add_member(
    auth: RequireAdmin,
    State(state): State<AppState>,
    Path(address): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<MemberResponse>), ApiError> {
    if req.owner_address.len() != 42 || !req.owner_address.starts_with("0x") {
        return Err(ApiError::bad_request(
            "owner_address must be a 42-character 0x-prefixed EVM address",
        ));
    }

    let wallet = load_wallet(&state, &address).await?;
    // The Admin doing the linking needs membership-or-admin rights on
    // the Safe itself, same as proposing a transaction.
    assert_safe_access(&state, &auth.0, &wallet).await?;

    // Owner address must belong to the on-chain Safe.
    let target_lc = req.owner_address.to_lowercase();
    let owner_known = wallet.owners.iter().any(|o| o.to_lowercase() == target_lc);
    if !owner_known {
        return Err(ApiError::bad_request(
            "owner_address is not listed among this Safe's on-chain owners",
        ));
    }

    state
        .db
        .add_safe_member(&wallet.address, wallet.chain.as_str(), &target_lc, req.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(
        safe = %wallet.address,
        owner = %target_lc,
        user_id = %req.user_id,
        linked_by = %auth.0.email,
        "Safe member linked"
    );

    Ok((
        StatusCode::CREATED,
        Json(MemberResponse {
            owner_address: target_lc,
            user_id: req.user_id,
            created_at: chrono::Utc::now(),
        }),
    ))
}

/// DELETE /v1/safe-wallets/{address}/members/{owner_address}
async fn remove_member(
    auth: RequireAdmin,
    State(state): State<AppState>,
    Path((address, owner_address)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let wallet = load_wallet(&state, &address).await?;
    assert_safe_access(&state, &auth.0, &wallet).await?;

    let removed = state
        .db
        .remove_safe_member(&wallet.address, wallet.chain.as_str(), &owner_address)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !removed {
        return Err(ApiError::not_found("Member not found on this Safe"));
    }
    tracing::info!(
        safe = %wallet.address,
        owner = %owner_address.to_lowercase(),
        unlinked_by = %auth.0.email,
        "Safe member unlinked"
    );
    Ok(Json(serde_json::json!({"status": "unlinked"})))
}

/// GET /v1/safe-wallets/{address}/members
async fn list_members(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<Vec<MemberResponse>>, ApiError> {
    let wallet = load_wallet(&state, &address).await?;
    // Viewer can read the member roster only for Safes they have
    // access to (membership or admin-registered). Otherwise the roster
    // itself becomes a reconnaissance tool.
    assert_safe_access(&state, &auth.0, &wallet).await?;

    let rows = state
        .db
        .list_safe_members(&wallet.address, wallet.chain.as_str())
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(
        rows.into_iter()
            .map(|(owner_address, user_id, created_at)| MemberResponse {
                owner_address,
                user_id,
                created_at,
            })
            .collect(),
    ))
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
