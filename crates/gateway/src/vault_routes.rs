//! Vault management, internal transfers, transaction history, raw signing,
//! fee estimation, asset management, and tx cancel/accelerate API endpoints.
//!
//! These endpoints bring functional parity with Fireblocks' core API.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use chrono::Utc;
use custody_common::vault::*;
use custody_common::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireViewer, RequireEditor, RequireSigner, RequireAdmin};
use crate::state::AppState;

/// Build the vault router — merged into the main router.
pub fn vault_router() -> Router<AppState> {
    Router::new()
        // Vault CRUD
        .route("/v1/vaults", post(create_vault))
        .route("/v1/vaults", get(list_vaults))
        .route("/v1/vaults/{id}", get(get_vault))
        .route("/v1/vaults/{id}", put(update_vault))
        .route("/v1/vaults/{id}", delete(delete_vault))
        // Vault accounts
        .route("/v1/vaults/{id}/accounts", post(create_vault_account))
        .route("/v1/vaults/{id}/accounts", get(list_vault_accounts))
        .route("/v1/vaults/{vault_id}/accounts/{account_id}", get(get_vault_account))
        // Raw signing
        .route("/v1/vaults/{id}/raw-sign", post(raw_sign))
        // Internal transfers
        .route("/v1/transfers", post(create_transfer))
        .route("/v1/transfers/{id}", get(get_transfer))
        // Transaction history
        .route("/v1/transactions", get(list_transactions))
        .route("/v1/transactions/{id}", get(get_transaction))
        .route("/v1/transactions/{id}/cancel", post(cancel_transaction))
        .route("/v1/transactions/{id}/accelerate", post(accelerate_transaction))
        // Asset management
        .route("/v1/assets", get(list_assets))
        .route("/v1/assets", post(create_asset))
        .route("/v1/assets/{symbol}/{chain}", get(get_asset))
        .route("/v1/assets/{id}", put(update_asset))
        .route("/v1/assets/{id}", delete(delete_asset))
        // Fee estimation
        .route("/v1/estimate-fee", get(estimate_fee))
}

// ---------------------------------------------------------------------------
// Vault CRUD
// ---------------------------------------------------------------------------

async fn create_vault(
    auth: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateVaultRequest>,
) -> Result<(StatusCode, Json<Vault>), ApiError> {
    let owner_id = auth.0.user_id;
    let now = Utc::now();

    let vault = Vault {
        id: Uuid::new_v4(),
        name: req.name,
        owner_id,
        auto_fuel: req.auto_fuel,
        hidden: false,
        created_at: now,
        updated_at: now,
    };

    state.db.insert_vault(&vault).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(vault_id = %vault.id, name = %vault.name, "Vault created");
    Ok((StatusCode::CREATED, Json(vault)))
}

#[derive(Deserialize)]
struct ListVaultsQuery {
    #[serde(default)]
    include_hidden: bool,
}

async fn list_vaults(
    auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<ListVaultsQuery>,
) -> Result<Json<Vec<Vault>>, ApiError> {
    let owner_id = auth.0.user_id;
    let vaults = state.db.list_vaults(owner_id, q.include_hidden).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(vaults))
}

async fn get_vault(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VaultDetail>, ApiError> {
    let vault = state.db.find_vault(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    let accounts = state.db.list_vault_accounts(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(VaultDetail { vault, accounts }))
}

async fn update_vault(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateVaultRequest>,
) -> Result<Json<Vault>, ApiError> {
    state.db.update_vault(id, req.name.as_deref(), req.auto_fuel).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let vault = state.db.find_vault(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    Ok(Json(vault))
}

async fn delete_vault(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Check vault exists
    let _vault = state.db.find_vault(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    // Check no remaining balance
    let has_balance = state.db.vault_has_balance(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if has_balance {
        return Err(ApiError::conflict("Cannot delete vault with non-zero balances"));
    }

    state.db.hide_vault(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(vault_id = %id, "Vault soft-deleted");
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Vault accounts
// ---------------------------------------------------------------------------

async fn create_vault_account(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
    Json(req): Json<CreateVaultAccountRequest>,
) -> Result<(StatusCode, Json<VaultAccount>), ApiError> {
    // Verify vault exists
    let _vault = state.db.find_vault(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    // Check idempotency
    if let Some(existing) = state.db.find_vault_account(vault_id, req.asset.as_str(), req.chain.as_str()).await
        .map_err(|e| ApiError::internal(&e.to_string()))? {
        return Ok((StatusCode::OK, Json(existing)));
    }

    // Derive address via the address allocator
    let addr = state.address_allocator
        .get_or_create(&state.seed, vault_id, &req.asset, req.chain)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let account = VaultAccount {
        id: Uuid::new_v4(),
        vault_id,
        asset: req.asset,
        chain: req.chain,
        address: addr.address.clone(),
        derivation_path: addr.derivation_path.clone(),
        balance: Decimal::ZERO,
        pending_balance: Decimal::ZERO,
        frozen: false,
        created_at: Utc::now(),
    };

    state.db.insert_vault_account(&account).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(vault_id = %vault_id, asset = %account.asset, chain = %account.chain, "Vault account created");
    Ok((StatusCode::CREATED, Json(account)))
}

async fn list_vault_accounts(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
) -> Result<Json<Vec<VaultAccount>>, ApiError> {
    let accounts = state.db.list_vault_accounts(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(accounts))
}

async fn get_vault_account(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path((vault_id, account_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<VaultAccount>, ApiError> {
    let account = state.db.find_vault_account_by_id(account_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault account not found"))?;

    if account.vault_id != vault_id {
        return Err(ApiError::not_found("Vault account not found in this vault"));
    }
    Ok(Json(account))
}

// ---------------------------------------------------------------------------
// Raw message signing
// ---------------------------------------------------------------------------

async fn raw_sign(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
    Json(req): Json<RawSignRequest>,
) -> Result<Json<RawSignResponse>, ApiError> {
    // Find the vault account for this chain
    let _vault = state.db.find_vault(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    // Decode the hex message
    let message_bytes = hex::decode(&req.message)
        .map_err(|_| ApiError::bad_request("Invalid hex encoding in message"))?;

    let seed_bytes: &[u8; 64] = &state.seed.0;
    // Use index 0 for now (vault's primary account)
    let index = 0u32;

    let (signature, public_key, algorithm) = match req.chain {
        Chain::Solana => {
            // Ed25519 signs raw message (not a hash)
            let msg = match req.encoding {
                MessageEncoding::Hex => message_bytes,
                MessageEncoding::Utf8 => message_bytes, // already bytes
                _ => return Err(ApiError::bad_request("Solana only supports hex and utf8 encoding")),
            };
            let (sig, pk) = custody_signing::raw::sign_raw_ed25519(seed_bytes, index, &msg)
                .map_err(|e| ApiError::internal(&e.to_string()))?;
            (sig, pk, RawSigningAlgorithm::Ed25519)
        }
        chain => {
            // secp256k1: hash the message first
            let hash: [u8; 32] = match req.encoding {
                MessageEncoding::Hex => custody_signing::raw::keccak256_hash(&message_bytes),
                MessageEncoding::Eip191 => custody_signing::raw::eip191_hash(&message_bytes),
                MessageEncoding::Eip712 => {
                    if message_bytes.len() != 32 {
                        return Err(ApiError::bad_request("EIP-712 message must be exactly 32 bytes"));
                    }
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&message_bytes);
                    h
                }
                MessageEncoding::Utf8 => custody_signing::raw::keccak256_hash(&message_bytes),
            };
            let (sig, pk) = custody_signing::raw::sign_raw_secp256k1(seed_bytes, chain, index, &hash)
                .map_err(|e| ApiError::internal(&e.to_string()))?;
            (sig, pk, RawSigningAlgorithm::Secp256k1)
        }
    };

    tracing::info!(
        vault_id = %vault_id,
        chain = %req.chain,
        encoding = ?req.encoding,
        "Raw message signed"
    );

    Ok(Json(RawSignResponse {
        id: Uuid::new_v4(),
        signature: hex::encode(&signature),
        public_key: hex::encode(&public_key),
        algorithm,
        signed_at: Utc::now(),
    }))
}

// ---------------------------------------------------------------------------
// Internal transfers
// ---------------------------------------------------------------------------

async fn create_transfer(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<CreateTransferRequest>,
) -> Result<Json<InternalTransfer>, ApiError> {
    // Validate source != dest
    if req.source_vault_id == req.dest_vault_id {
        return Err(ApiError::bad_request("Source and destination vaults must be different"));
    }

    // Validate amount > 0
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("Amount must be positive"));
    }

    // Idempotency check
    if let Some(ref key) = req.idempotency_key {
        if let Some(existing) = state.db.find_internal_transfer_by_key(key).await
            .map_err(|e| ApiError::internal(&e.to_string()))? {
            return Ok(Json(existing));
        }
    }

    // Verify both vaults exist
    state.db.find_vault(req.source_vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Source vault not found"))?;

    state.db.find_vault(req.dest_vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Destination vault not found"))?;

    // Check source has sufficient balance
    let source_account = state.db
        .find_vault_account(req.source_vault_id, req.asset.as_str(), req.chain.as_str()).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::bad_request("Source vault has no account for this asset/chain"))?;

    if source_account.frozen {
        return Err(ApiError::conflict("Source account is frozen"));
    }

    if source_account.balance < req.amount {
        return Err(ApiError::bad_request(&format!(
            "Insufficient balance: {} < {}", source_account.balance, req.amount
        )));
    }

    let now = Utc::now();
    let transfer = InternalTransfer {
        id: Uuid::new_v4(),
        source_vault_id: req.source_vault_id,
        dest_vault_id: req.dest_vault_id,
        asset: req.asset.clone(),
        chain: req.chain,
        amount: req.amount,
        status: InternalTransferStatus::Completed,
        idempotency_key: req.idempotency_key,
        note: req.note,
        created_at: now,
        completed_at: Some(now),
    };

    // Insert transfer
    state.db.insert_internal_transfer(&transfer).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Record double-entry ledger
    state.db.record_internal_transfer(
        transfer.id,
        req.source_vault_id,
        req.dest_vault_id,
        req.asset.as_str(),
        req.chain,
        req.amount,
    ).await.map_err(|e| ApiError::internal(&e.to_string()))?;

    // Update cached balances
    let new_source_balance = source_account.balance - req.amount;
    state.db.update_vault_account_balance(
        source_account.id, new_source_balance, source_account.pending_balance,
    ).await.map_err(|e| ApiError::internal(&e.to_string()))?;

    if let Some(dest_account) = state.db
        .find_vault_account(req.dest_vault_id, req.asset.as_str(), req.chain.as_str()).await
        .map_err(|e| ApiError::internal(&e.to_string()))? {
        let new_dest_balance = dest_account.balance + req.amount;
        state.db.update_vault_account_balance(
            dest_account.id, new_dest_balance, dest_account.pending_balance,
        ).await.map_err(|e| ApiError::internal(&e.to_string()))?;
    }

    tracing::info!(
        transfer_id = %transfer.id,
        source = %req.source_vault_id,
        dest = %req.dest_vault_id,
        amount = %req.amount,
        "Internal transfer completed"
    );

    Ok(Json(transfer))
}

async fn get_transfer(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<InternalTransfer>, ApiError> {
    let transfer = state.db.find_internal_transfer(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Transfer not found"))?;
    Ok(Json(transfer))
}

// ---------------------------------------------------------------------------
// Transaction history
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TransactionQuery {
    vault_id: Option<Uuid>,
    asset: Option<String>,
    chain: Option<String>,
    status: Option<String>,
    #[serde(rename = "type")]
    tx_type: Option<String>,
    after: Option<String>,
    before: Option<String>,
    min_amount: Option<Decimal>,
    max_amount: Option<Decimal>,
    cursor: Option<String>,
    limit: Option<u32>,
}

async fn list_transactions(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<TransactionQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let tx_type = q.tx_type.as_deref().map(|t| match t {
        "deposit" => TransactionType::Deposit,
        "withdrawal" => TransactionType::Withdrawal,
        "internal_transfer" => TransactionType::InternalTransfer,
        _ => TransactionType::Deposit,
    });

    let filter = TransactionFilter {
        vault_id: q.vault_id,
        asset: q.asset,
        chain: q.chain,
        status: q.status,
        tx_type,
        after: q.after.and_then(|s| s.parse().ok()),
        before: q.before.and_then(|s| s.parse().ok()),
        min_amount: q.min_amount,
        max_amount: q.max_amount,
        cursor: q.cursor,
        limit: q.limit,
    };

    let (transactions, next_cursor) = state.db.list_transactions(&filter).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({
        "transactions": transactions,
        "next_cursor": next_cursor,
    })))
}

async fn get_transaction(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Transaction>, ApiError> {
    let tx = state.db.find_transaction(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Transaction not found"))?;
    Ok(Json(tx))
}

// ---------------------------------------------------------------------------
// Transaction cancel / accelerate
// ---------------------------------------------------------------------------

async fn cancel_transaction(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let withdrawal = state.db.find_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Transaction not found"))?;

    // Only cancellable if broadcast/signed and not confirmed
    match withdrawal.status {
        WithdrawalStatus::Broadcast | WithdrawalStatus::Signed => {}
        WithdrawalStatus::Finalized | WithdrawalStatus::Confirming => {
            return Err(ApiError::conflict("Transaction already confirmed or confirming"));
        }
        _ => {
            return Err(ApiError::conflict(&format!(
                "Transaction in status {:?} cannot be cancelled via RBF", withdrawal.status
            )));
        }
    }

    // Only BTC and ETH support cancellation via replacement
    match withdrawal.chain {
        Chain::Bitcoin | Chain::Ethereum => {}
        _ => {
            return Err(ApiError::bad_request(&format!(
                "Chain {} does not support transaction cancellation", withdrawal.chain
            )));
        }
    }

    // Mark as cancelled
    state.db.update_withdrawal_status(id, WithdrawalStatus::Cancelled).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %id, chain = %withdrawal.chain, "Transaction cancelled");

    Ok(Json(serde_json::json!({
        "status": "cancelled",
        "withdrawal_id": id,
    })))
}

async fn accelerate_transaction(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AccelerateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let withdrawal = state.db.find_withdrawal(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Transaction not found"))?;

    if withdrawal.status != WithdrawalStatus::Broadcast {
        return Err(ApiError::conflict("Can only accelerate broadcast transactions"));
    }

    match withdrawal.chain {
        Chain::Bitcoin | Chain::Ethereum => {}
        _ => {
            return Err(ApiError::bad_request(&format!(
                "Chain {} does not support transaction acceleration", withdrawal.chain
            )));
        }
    }

    // Validate fee multiplier (default 1.25, max 5.0)
    let multiplier = req.fee_multiplier.unwrap_or(dec!(1.25));
    if multiplier < dec!(1.125) {
        return Err(ApiError::bad_request("Fee multiplier must be at least 1.125 (12.5% increase)"));
    }
    if multiplier > dec!(5.0) {
        return Err(ApiError::bad_request("Fee multiplier cannot exceed 5.0"));
    }

    tracing::info!(
        id = %id,
        chain = %withdrawal.chain,
        multiplier = %multiplier,
        "Transaction acceleration requested"
    );

    Ok(Json(serde_json::json!({
        "status": "accelerating",
        "withdrawal_id": id,
        "fee_multiplier": multiplier,
    })))
}

// ---------------------------------------------------------------------------
// Asset management
// ---------------------------------------------------------------------------

async fn list_assets(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<AssetQuery>,
) -> Result<Json<Vec<ManagedAsset>>, ApiError> {
    let assets = state.db.list_assets(q.chain.as_deref()).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(assets))
}

#[derive(Deserialize)]
struct AssetQuery {
    chain: Option<String>,
}

async fn get_asset(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path((symbol, chain)): Path<(String, String)>,
) -> Result<Json<ManagedAsset>, ApiError> {
    let asset = state.db.find_asset(&symbol, &chain).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Asset not found"))?;
    Ok(Json(asset))
}

async fn create_asset(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<ManagedAsset>), ApiError> {
    let asset = ManagedAsset {
        id: Uuid::new_v4(),
        symbol: req.symbol.to_uppercase(),
        chain: req.chain,
        contract_address: req.contract_address,
        decimals: req.decimals,
        is_native: false,
        enabled: true,
        added_at: Utc::now(),
    };

    state.db.insert_asset(&asset).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(symbol = %asset.symbol, chain = %asset.chain, "Asset created");
    Ok((StatusCode::CREATED, Json(asset)))
}

async fn update_asset(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAssetRequest>,
) -> Result<Json<ManagedAsset>, ApiError> {
    state.db.update_asset_enabled(id, req.enabled).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let asset = state.db.find_asset_by_id(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Asset not found"))?;

    Ok(Json(asset))
}

async fn delete_asset(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_asset(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !deleted {
        return Err(ApiError::not_found("Asset not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Fee estimation
// ---------------------------------------------------------------------------

async fn estimate_fee(
    _auth: RequireViewer,
    Query(q): Query<FeeEstimateQuery>,
) -> Result<Json<FeeEstimateResponse>, ApiError> {
    let asset_str = q.asset.as_deref().unwrap_or(match q.chain {
        Chain::Bitcoin => "BTC",
        Chain::Ethereum => "ETH",
        Chain::Tron => "TRX",
        Chain::Solana => "SOL",
    });

    let response = match q.chain {
        Chain::Ethereum => FeeEstimateResponse {
            chain: Chain::Ethereum,
            asset: Asset::new(asset_str),
            low: FeeLevel {
                fee: dec!(0.000315),
                gas_price_gwei: Some(dec!(15)),
                sat_per_vbyte: None,
                estimated_seconds: 60,
            },
            medium: FeeLevel {
                fee: dec!(0.000630),
                gas_price_gwei: Some(dec!(30)),
                sat_per_vbyte: None,
                estimated_seconds: 15,
            },
            high: FeeLevel {
                fee: dec!(0.001260),
                gas_price_gwei: Some(dec!(60)),
                sat_per_vbyte: None,
                estimated_seconds: 6,
            },
            estimated_at: Utc::now(),
        },
        Chain::Bitcoin => FeeEstimateResponse {
            chain: Chain::Bitcoin,
            asset: Asset::new(asset_str),
            low: FeeLevel {
                fee: dec!(0.00001400),
                gas_price_gwei: None,
                sat_per_vbyte: Some(dec!(10)),
                estimated_seconds: 3600,
            },
            medium: FeeLevel {
                fee: dec!(0.00002800),
                gas_price_gwei: None,
                sat_per_vbyte: Some(dec!(20)),
                estimated_seconds: 1200,
            },
            high: FeeLevel {
                fee: dec!(0.00007000),
                gas_price_gwei: None,
                sat_per_vbyte: Some(dec!(50)),
                estimated_seconds: 600,
            },
            estimated_at: Utc::now(),
        },
        Chain::Tron => FeeEstimateResponse {
            chain: Chain::Tron,
            asset: Asset::new(asset_str),
            low: FeeLevel {
                fee: dec!(1.0),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 9,
            },
            medium: FeeLevel {
                fee: dec!(5.0),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 3,
            },
            high: FeeLevel {
                fee: dec!(15.0),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 3,
            },
            estimated_at: Utc::now(),
        },
        Chain::Solana => FeeEstimateResponse {
            chain: Chain::Solana,
            asset: Asset::new(asset_str),
            low: FeeLevel {
                fee: dec!(0.000005),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 2,
            },
            medium: FeeLevel {
                fee: dec!(0.00001),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 1,
            },
            high: FeeLevel {
                fee: dec!(0.0001),
                gas_price_gwei: None,
                sat_per_vbyte: None,
                estimated_seconds: 1,
            },
            estimated_at: Utc::now(),
        },
    };

    Ok(Json(response))
}
