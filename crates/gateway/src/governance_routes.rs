//! Governance API: vault policies, approval workflows, supported networks,
//! transaction notes, staking execution.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use chrono::{Duration, Utc};
use custody_common::governance::*;
use custody_common::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireSigner, RequireViewer};
use crate::state::AppState;

pub fn governance_router() -> Router<AppState> {
    Router::new()
        // Vault policies
        .route("/v1/vaults/{id}/policies", get(list_vault_policies))
        .route("/v1/vaults/{id}/policies", post(create_vault_policy))
        .route("/v1/policies/{id}", get(get_vault_policy))
        .route("/v1/policies/{id}", put(update_vault_policy))
        .route("/v1/policies/{id}", delete(delete_vault_policy))
        // Approval workflows
        .route("/v1/approvals/{withdrawal_id}", get(get_approval_workflow))
        .route("/v1/approvals/{withdrawal_id}/decide", post(submit_approval_decision))
        .route("/v1/vaults/{id}/approvals", get(list_vault_pending_approvals))
        // Supported networks
        .route("/v1/supported-networks", get(list_supported_networks))
        .route("/v1/supported-networks/{chain}", get(get_network_detail))
        .route("/v1/supported-networks/{chain}/tokens", get(list_network_tokens))
        // Transaction notes
        .route("/v1/transactions/{id}/notes", get(list_transaction_notes))
        .route("/v1/transactions/{id}/notes", post(create_transaction_note))
        // Staking routes moved to staking_routes.rs to avoid overlap
}

// ---------------------------------------------------------------------------
// Vault policies
// ---------------------------------------------------------------------------

async fn list_vault_policies(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
) -> Result<Json<Vec<VaultPolicy>>, ApiError> {
    let policies = state.db.list_vault_policies(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(policies))
}

async fn create_vault_policy(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
    Json(req): Json<CreateVaultPolicyRequest>,
) -> Result<(StatusCode, Json<VaultPolicy>), ApiError> {
    let now = Utc::now();
    let policy = VaultPolicy {
        id: Uuid::new_v4(),
        vault_id,
        name: req.name,
        rules: req.rules,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    state.db.insert_vault_policy(&policy).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %policy.id, vault = %vault_id, name = %policy.name, "Vault policy created");
    Ok((StatusCode::CREATED, Json(policy)))
}

async fn get_vault_policy(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VaultPolicy>, ApiError> {
    let policy = state.db.find_vault_policy(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Policy not found"))?;
    Ok(Json(policy))
}

async fn update_vault_policy(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateVaultPolicyRequest>,
) -> Result<Json<VaultPolicy>, ApiError> {
    state.db.update_vault_policy(id, req.name.as_deref(), req.rules.as_deref(), req.enabled).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let policy = state.db.find_vault_policy(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Policy not found"))?;
    Ok(Json(policy))
}

async fn delete_vault_policy(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_vault_policy(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted { return Err(ApiError::not_found("Policy not found")); }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Approval workflows
// ---------------------------------------------------------------------------

async fn get_approval_workflow(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(withdrawal_id): Path<Uuid>,
) -> Result<Json<ApprovalWorkflow>, ApiError> {
    let wf = state.db.find_approval_workflow(withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Approval workflow not found"))?;
    Ok(Json(wf))
}

async fn submit_approval_decision(
    _auth: RequireSigner,
    State(state): State<AppState>,
    Path(withdrawal_id): Path<Uuid>,
    Json(req): Json<SubmitApprovalRequest>,
) -> Result<Json<ApprovalWorkflow>, ApiError> {
    let wf = state.db.find_approval_workflow(withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Approval workflow not found"))?;

    if wf.status != ApprovalStatus::Pending {
        return Err(ApiError::conflict(&format!("Workflow already {}", wf.status)));
    }

    // Check duplicate approver
    if wf.decisions.iter().any(|d| d.approver_id == req.approver_id) {
        return Err(ApiError::conflict("Approver has already submitted a decision"));
    }

    let decision = ApprovalDecision {
        approver_id: req.approver_id,
        approver_email: req.approver_email,
        decision: req.decision,
        reason: req.reason,
        decided_at: Utc::now(),
    };

    state.db.add_approval_decision_to_workflow(wf.id, &decision).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Re-fetch to get updated state
    let updated = state.db.find_approval_workflow(withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::internal("Workflow disappeared"))?;

    tracing::info!(
        withdrawal_id = %withdrawal_id,
        decision = ?req.decision,
        status = %updated.status,
        "Approval decision recorded"
    );

    Ok(Json(updated))
}

async fn list_vault_pending_approvals(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
) -> Result<Json<Vec<ApprovalWorkflow>>, ApiError> {
    let wfs = state.db.list_pending_approvals_for_vault(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(wfs))
}

// ---------------------------------------------------------------------------
// Supported networks
// ---------------------------------------------------------------------------

async fn list_supported_networks(_auth: RequireViewer) -> Json<Vec<NetworkInfo>> {
    Json(all_networks())
}

async fn get_network_detail(
    _auth: RequireViewer,
    Path(chain): Path<String>,
) -> Result<Json<NetworkInfo>, ApiError> {
    let chain_parsed = Chain::parse_str(&chain);
    all_networks().into_iter()
        .find(|n| n.chain == chain_parsed)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Network not found"))
}

async fn list_network_tokens(
    _auth: RequireViewer,
    Path(chain): Path<String>,
) -> Json<Vec<NetworkToken>> {
    let chain_parsed = Chain::parse_str(&chain);
    let tokens = match chain_parsed {
        Chain::Ethereum => vec![
            NetworkToken { symbol: "ETH".into(), name: "Ether".into(), chain: Chain::Ethereum, contract_address: None, decimals: 18, token_type: "native".into() },
            NetworkToken { symbol: "USDT".into(), name: "Tether USD".into(), chain: Chain::Ethereum, contract_address: Some("0xdAC17F958D2ee523a2206206994597C13D831ec7".into()), decimals: 6, token_type: "ERC-20".into() },
            NetworkToken { symbol: "USDC".into(), name: "USD Coin".into(), chain: Chain::Ethereum, contract_address: Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into()), decimals: 6, token_type: "ERC-20".into() },
        ],
        Chain::Bitcoin => vec![
            NetworkToken { symbol: "BTC".into(), name: "Bitcoin".into(), chain: Chain::Bitcoin, contract_address: None, decimals: 8, token_type: "native".into() },
        ],
        Chain::Tron => vec![
            NetworkToken { symbol: "TRX".into(), name: "TRON".into(), chain: Chain::Tron, contract_address: None, decimals: 6, token_type: "native".into() },
            NetworkToken { symbol: "USDT".into(), name: "Tether USD".into(), chain: Chain::Tron, contract_address: Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".into()), decimals: 6, token_type: "TRC-20".into() },
        ],
        Chain::Solana => vec![
            NetworkToken { symbol: "SOL".into(), name: "Solana".into(), chain: Chain::Solana, contract_address: None, decimals: 9, token_type: "native".into() },
            NetworkToken { symbol: "USDT".into(), name: "Tether USD".into(), chain: Chain::Solana, contract_address: Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".into()), decimals: 6, token_type: "SPL".into() },
            NetworkToken { symbol: "USDC".into(), name: "USD Coin".into(), chain: Chain::Solana, contract_address: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into()), decimals: 6, token_type: "SPL".into() },
        ],
    };
    Json(tokens)
}

fn all_networks() -> Vec<NetworkInfo> {
    vec![
        NetworkInfo {
            chain: Chain::Ethereum, name: "Ethereum Mainnet".into(),
            native_asset: "ETH".into(), native_decimals: 18,
            address_format: "0x-prefixed hex, 42 chars, EIP-55 checksum".into(),
            required_confirmations: 12, average_block_time_secs: 12,
            supports_eip1559: true, supports_rbf: false,
            supports_staking: true, explorer_url: "https://etherscan.io".into(),
            testnet_available: true,
        },
        NetworkInfo {
            chain: Chain::Bitcoin, name: "Bitcoin Mainnet".into(),
            native_asset: "BTC".into(), native_decimals: 8,
            address_format: "Bech32 (bc1q...), P2WPKH".into(),
            required_confirmations: 3, average_block_time_secs: 600,
            supports_eip1559: false, supports_rbf: true,
            supports_staking: false, explorer_url: "https://mempool.space".into(),
            testnet_available: true,
        },
        NetworkInfo {
            chain: Chain::Tron, name: "TRON Mainnet".into(),
            native_asset: "TRX".into(), native_decimals: 6,
            address_format: "Base58Check, T-prefix".into(),
            required_confirmations: 20, average_block_time_secs: 3,
            supports_eip1559: false, supports_rbf: false,
            supports_staking: true, explorer_url: "https://tronscan.org".into(),
            testnet_available: true,
        },
        NetworkInfo {
            chain: Chain::Solana, name: "Solana Mainnet".into(),
            native_asset: "SOL".into(), native_decimals: 9,
            address_format: "Base58, 32-44 chars".into(),
            required_confirmations: 1, average_block_time_secs: 1,
            supports_eip1559: false, supports_rbf: false,
            supports_staking: true, explorer_url: "https://explorer.solana.com".into(),
            testnet_available: true,
        },
    ]
}

// ---------------------------------------------------------------------------
// Transaction notes
// ---------------------------------------------------------------------------

async fn list_transaction_notes(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(tx_id): Path<Uuid>,
) -> Result<Json<Vec<TransactionNote>>, ApiError> {
    let notes = state.db.list_transaction_notes(tx_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(notes))
}

async fn create_transaction_note(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(tx_id): Path<Uuid>,
    Json(req): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<TransactionNote>), ApiError> {
    let note = TransactionNote {
        id: Uuid::new_v4(),
        transaction_id: tx_id,
        note: req.note,
        author: req.author.unwrap_or_else(|| "system".into()),
        created_at: Utc::now(),
    };

    state.db.insert_transaction_note(&note).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok((StatusCode::CREATED, Json(note)))
}

// ---------------------------------------------------------------------------
// Staking execution
// ---------------------------------------------------------------------------

async fn list_staking_positions(_auth: RequireViewer) -> Json<Vec<StakingPositionSummary>> {
    // Placeholder — would query from staking_positions table
    Json(vec![])
}

async fn stake(
    _auth: RequireSigner,
    Json(req): Json<StakeRequest>,
) -> Result<Json<StakingPositionSummary>, ApiError> {
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("Stake amount must be positive"));
    }

    // Validate minimum stake
    let min = match req.chain {
        Chain::Ethereum => dec!(32),
        Chain::Solana => dec!(1),
        _ => return Err(ApiError::bad_request(&format!("Staking not supported on {}", req.chain))),
    };

    if req.amount < min {
        return Err(ApiError::bad_request(&format!(
            "Minimum stake for {} is {}", req.chain, min
        )));
    }

    let position = StakingPositionSummary {
        id: Uuid::new_v4(),
        vault_id: req.vault_id,
        chain: req.chain,
        validator: req.validator,
        amount_staked: req.amount,
        rewards_earned: Decimal::ZERO,
        status: "pending".into(),
        staked_at: Utc::now(),
        unbonding_at: None,
    };

    tracing::info!(
        vault = %req.vault_id, chain = %req.chain,
        amount = %req.amount, "Stake initiated"
    );

    Ok(Json(position))
}

async fn unstake(
    _auth: RequireSigner,
    Json(req): Json<UnstakeRequest>,
) -> Result<Json<StakingPositionSummary>, ApiError> {
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("Unstake amount must be positive"));
    }

    let unbonding_days = match req.chain {
        Chain::Ethereum => 27,
        Chain::Solana => 3,
        _ => return Err(ApiError::bad_request(&format!("Staking not supported on {}", req.chain))),
    };

    let position = StakingPositionSummary {
        id: Uuid::new_v4(),
        vault_id: req.vault_id,
        chain: req.chain,
        validator: req.validator,
        amount_staked: req.amount,
        rewards_earned: Decimal::ZERO,
        status: "unstaking".into(),
        staked_at: Utc::now(),
        unbonding_at: Some(Utc::now() + Duration::days(unbonding_days)),
    };

    tracing::info!(
        vault = %req.vault_id, chain = %req.chain,
        amount = %req.amount, unbonding_days, "Unstake initiated"
    );

    Ok(Json(position))
}
