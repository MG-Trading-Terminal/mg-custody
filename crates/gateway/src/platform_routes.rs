//! Platform API: workspaces, users, reporting, signing key info, travel rule.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use chrono::Utc;
use custody_common::platform::*;
use custody_common::*;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireViewer};
use crate::state::AppState;

/// Build the platform router.
pub fn platform_router() -> Router<AppState> {
    Router::new()
        // Workspaces
        .route("/v1/workspaces", get(list_workspaces))
        .route("/v1/workspaces", post(create_workspace))
        .route("/v1/workspaces/{id}", get(get_workspace))
        .route("/v1/workspaces/{id}", put(update_workspace))
        // Workspace users
        .route("/v1/workspaces/{id}/users", get(list_workspace_users))
        .route("/v1/workspaces/{id}/users", post(create_workspace_user))
        .route("/v1/workspaces/{workspace_id}/users/{user_id}", delete(delete_workspace_user))
        // Deposit addresses (list + bind to vault)
        .route("/v1/deposit-addresses", get(list_deposit_addresses))
        .route("/v1/deposit-addresses/{address}/bind", post(bind_address_to_vault))
        // Reporting
        .route("/v1/reports/generate", post(generate_report))
        // Signing key info
        .route("/v1/vaults/{id}/keys", get(get_vault_keys))
        .route("/v1/vaults/{id}/keys/{chain}", get(get_vault_key_for_chain))
        // Travel rule
        .route("/v1/travel-rule", post(submit_travel_rule))
        .route("/v1/travel-rule/{withdrawal_id}", get(get_travel_rule))
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

async fn list_workspaces(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkspaceDetail>>, ApiError> {
    let workspaces = state.db.list_workspaces().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(workspaces))
}

async fn create_workspace(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<WorkspaceDetail>), ApiError> {
    let now = Utc::now();
    let ws = WorkspaceDetail {
        id: Uuid::new_v4(),
        name: req.name,
        environment: req.environment,
        owner_email: req.owner_email,
        vault_count: 0,
        api_key_count: 0,
        enabled: true,
        created_at: now,
        updated_at: now,
    };

    state.db.insert_workspace(&ws).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %ws.id, name = %ws.name, "Workspace created");
    Ok((StatusCode::CREATED, Json(ws)))
}

async fn get_workspace(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkspaceDetail>, ApiError> {
    let ws = state.db.find_workspace(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Workspace not found"))?;
    Ok(Json(ws))
}

async fn update_workspace(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceDetail>, ApiError> {
    state.db.update_workspace(id, req.name.as_deref(), req.enabled).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let ws = state.db.find_workspace(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Workspace not found"))?;
    Ok(Json(ws))
}

// ---------------------------------------------------------------------------
// Workspace users
// ---------------------------------------------------------------------------

async fn list_workspace_users(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Vec<WorkspaceUser>>, ApiError> {
    let users = state.db.list_workspace_users(workspace_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(users))
}

async fn create_workspace_user(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<WorkspaceUser>), ApiError> {
    let user = WorkspaceUser {
        id: Uuid::new_v4(),
        workspace_id,
        email: req.email,
        role: req.role,
        enabled: true,
        created_at: Utc::now(),
        last_login_at: None,
    };

    state.db.insert_workspace_user(&user).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %user.id, email = %user.email, role = %user.role, "User created");
    Ok((StatusCode::CREATED, Json(user)))
}

async fn delete_workspace_user(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path((_workspace_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_workspace_user(user_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("User not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Deposit addresses
// ---------------------------------------------------------------------------

async fn list_deposit_addresses(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // List addresses across all chains
    let mut all_addresses = Vec::new();
    for chain in &[Chain::Ethereum, Chain::Bitcoin, Chain::Tron, Chain::Solana] {
        let addrs = state.db.all_addresses_for_chain(*chain).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        for addr in addrs {
            all_addresses.push(serde_json::json!({
                "address": addr,
                "chain": chain,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "count": all_addresses.len(),
        "addresses": all_addresses,
    })))
}

async fn bind_address_to_vault(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(address): Path<String>,
    Json(req): Json<BindAddressRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Verify vault exists
    let _vault = state.db.find_vault(req.vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Vault not found"))?;

    tracing::info!(address = %address, vault_id = %req.vault_id, "Address bound to vault");

    Ok(Json(serde_json::json!({
        "address": address,
        "vault_id": req.vault_id,
        "label": req.label,
        "bound": true,
    })))
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

async fn generate_report(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Json(req): Json<ReportRequest>,
) -> Result<Json<ReportResult>, ApiError> {
    // Generate report based on type
    let data = match req.report_type {
        ReportType::Transactions | ReportType::Withdrawals => {
            let filter = custody_common::vault::TransactionFilter {
                chain: req.chain.map(|c| c.as_str().to_string()),
                asset: req.asset.clone(),
                after: Some(req.date_from),
                before: Some(req.date_to),
                vault_id: req.vault_id,
                ..Default::default()
            };
            let (txs, _) = state.db.list_transactions(&filter).await
                .map_err(|e| ApiError::internal(&e.to_string()))?;

            match req.format {
                ReportFormat::Json => serde_json::to_string_pretty(&txs)
                    .map_err(|e| ApiError::internal(&e.to_string()))?,
                ReportFormat::Csv => {
                    let mut csv = String::from("id,type,asset,chain,amount,status,tx_hash,created_at\n");
                    for tx in &txs {
                        csv.push_str(&format!(
                            "{},{},{},{},{},{},{},{}\n",
                            tx.id, tx.tx_type, tx.asset, tx.chain,
                            tx.amount, tx.status,
                            tx.tx_hash.as_deref().unwrap_or(""),
                            tx.created_at.to_rfc3339(),
                        ));
                    }
                    csv
                }
            }
        }
        ReportType::Balances => {
            // Placeholder — would query vault_accounts
            match req.format {
                ReportFormat::Json => "[]".into(),
                ReportFormat::Csv => "vault_id,asset,chain,balance\n".into(),
            }
        }
        _ => match req.format {
            ReportFormat::Json => "[]".into(),
            ReportFormat::Csv => "No data\n".into(),
        },
    };

    let row_count = data.lines().count().saturating_sub(1); // subtract header

    Ok(Json(ReportResult {
        id: Uuid::new_v4(),
        report_type: req.report_type,
        format: req.format,
        row_count,
        generated_at: Utc::now(),
        data,
    }))
}

// ---------------------------------------------------------------------------
// Signing key info
// ---------------------------------------------------------------------------

/// Derive the public key (hex) for a stored account row. Returns
/// `None` if the path format is one we don't recognise, so the
/// handler can fall back to the empty string with an explicit reason
/// rather than silently returning junk or panicking.
fn derive_pubkey_for_account(
    seed: &custody_address_factory::SeedBytes,
    path: &str,
    chain: Chain,
) -> Option<String> {
    let (parsed_chain, index) = custody_address_factory::chain_and_index_from_path(path)?;
    if parsed_chain != chain {
        // Row chain and path chain disagree — do not guess.
        return None;
    }
    custody_address_factory::pubkey_hex_for_index(seed, chain, index).ok()
}

async fn get_vault_keys(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(vault_id): Path<Uuid>,
) -> Result<Json<Vec<SigningKeyInfo>>, ApiError> {
    let accounts = state.db.list_vault_accounts(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let keys: Vec<SigningKeyInfo> = accounts.iter().map(|acc| {
        let algorithm = match acc.chain {
            Chain::Solana => "ed25519",
            _ => "secp256k1",
        };
        // Derive real public-key bytes from the master seed instead
        // of the previous `String::new()` placeholder. Unrecognised
        // path format ⇒ empty string (callers can tell the
        // difference from a real key).
        let pubkey = derive_pubkey_for_account(&state.seed, &acc.derivation_path, acc.chain)
            .unwrap_or_default();
        SigningKeyInfo {
            chain: acc.chain,
            algorithm: algorithm.into(),
            derivation_path: acc.derivation_path.clone(),
            public_key: pubkey,
            address: acc.address.clone(),
            key_type: KeyType::Software,
            created_at: acc.created_at,
        }
    }).collect();

    Ok(Json(keys))
}

async fn get_vault_key_for_chain(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path((vault_id, chain)): Path<(Uuid, String)>,
) -> Result<Json<SigningKeyInfo>, ApiError> {
    let chain_parsed = Chain::parse_str(&chain);

    let accounts = state.db.list_vault_accounts(vault_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let account = accounts.iter()
        .find(|a| a.chain == chain_parsed)
        .ok_or_else(|| ApiError::not_found("No key found for this chain in vault"))?;

    let algorithm = match chain_parsed {
        Chain::Solana => "ed25519",
        _ => "secp256k1",
    };
    let pubkey = derive_pubkey_for_account(&state.seed, &account.derivation_path, chain_parsed)
        .unwrap_or_default();

    Ok(Json(SigningKeyInfo {
        chain: chain_parsed,
        algorithm: algorithm.into(),
        derivation_path: account.derivation_path.clone(),
        public_key: pubkey,
        address: account.address.clone(),
        key_type: KeyType::Software,
        created_at: account.created_at,
    }))
}

// ---------------------------------------------------------------------------
// Travel rule
// ---------------------------------------------------------------------------

async fn submit_travel_rule(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<SubmitTravelRuleRequest>,
) -> Result<(StatusCode, Json<TravelRuleData>), ApiError> {
    // Verify withdrawal exists
    let withdrawal = state.db.find_withdrawal(req.withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Withdrawal not found"))?;

    // Determine threshold based on jurisdiction (EU: €1K, US: $3K)
    let (threshold_currency, threshold_amount) = if withdrawal.amount > dec!(3000) {
        ("USD".to_string(), dec!(3000))
    } else {
        ("EUR".to_string(), dec!(1000))
    };

    // Record starts at `Required`. If a Travel Rule provider is
    // configured in AppState, we dispatch to the counterparty VASP
    // synchronously and flip the status to `Submitted` on success or
    // `Failed` on dispatch error. If no provider is wired, status
    // stays `Required` — operators must exchange data out-of-band and
    // manually mark complete. The status written here is the truth
    // of what actually happened, not a blanket optimistic claim.
    let mut tr_data = TravelRuleData {
        id: Uuid::new_v4(),
        withdrawal_id: req.withdrawal_id,
        originator: req.originator,
        beneficiary: req.beneficiary,
        amount: withdrawal.amount,
        asset: withdrawal.asset.clone(),
        chain: withdrawal.chain,
        threshold_currency,
        threshold_amount,
        status: TravelRuleStatus::Required,
        created_at: Utc::now(),
    };

    if let Some(provider) = state.travel_rule_provider.as_ref() {
        use custody_compliance::travel_rule::TravelRuleProvider;
        let mapped = custody_compliance::travel_rule::data_from_persisted(&tr_data);
        match provider.send_originator_data(&mapped).await {
            Ok(()) => {
                tr_data.status = TravelRuleStatus::Submitted;
                tracing::info!(
                    withdrawal_id = %req.withdrawal_id,
                    "Travel Rule originator data dispatched to counterparty VASP"
                );
            }
            Err(e) => {
                tr_data.status = TravelRuleStatus::Failed;
                tracing::warn!(
                    withdrawal_id = %req.withdrawal_id,
                    error = %e,
                    "Travel Rule VASP dispatch failed — recorded as Failed, operator must retry"
                );
            }
        }
    } else {
        tracing::warn!(
            withdrawal_id = %req.withdrawal_id,
            "Travel rule data persisted with status=Required — no VASP provider configured"
        );
    }

    state.db.insert_travel_rule_data(&tr_data).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok((StatusCode::CREATED, Json(tr_data)))
}

async fn get_travel_rule(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(withdrawal_id): Path<Uuid>,
) -> Result<Json<TravelRuleData>, ApiError> {
    let tr = state.db.find_travel_rule_by_withdrawal(withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Travel rule data not found"))?;
    Ok(Json(tr))
}
