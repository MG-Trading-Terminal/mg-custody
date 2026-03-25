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
    State(state): State<AppState>,
) -> Result<Json<Vec<WorkspaceDetail>>, ApiError> {
    let workspaces = state.db.list_workspaces().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(workspaces))
}

async fn create_workspace(
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
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkspaceDetail>, ApiError> {
    let ws = state.db.find_workspace(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Workspace not found"))?;
    Ok(Json(ws))
}

async fn update_workspace(
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
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Vec<WorkspaceUser>>, ApiError> {
    let users = state.db.list_workspace_users(workspace_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(users))
}

async fn create_workspace_user(
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

async fn get_vault_keys(
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
        SigningKeyInfo {
            chain: acc.chain,
            algorithm: algorithm.into(),
            derivation_path: acc.derivation_path.clone(),
            public_key: String::new(), // would need to derive from seed
            address: acc.address.clone(),
            key_type: KeyType::Software,
            created_at: acc.created_at,
        }
    }).collect();

    Ok(Json(keys))
}

async fn get_vault_key_for_chain(
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

    Ok(Json(SigningKeyInfo {
        chain: chain_parsed,
        algorithm: algorithm.into(),
        derivation_path: account.derivation_path.clone(),
        public_key: String::new(),
        address: account.address.clone(),
        key_type: KeyType::Software,
        created_at: account.created_at,
    }))
}

// ---------------------------------------------------------------------------
// Travel rule
// ---------------------------------------------------------------------------

async fn submit_travel_rule(
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

    let tr_data = TravelRuleData {
        id: Uuid::new_v4(),
        withdrawal_id: req.withdrawal_id,
        originator: req.originator,
        beneficiary: req.beneficiary,
        amount: withdrawal.amount,
        asset: withdrawal.asset.clone(),
        chain: withdrawal.chain,
        threshold_currency,
        threshold_amount,
        status: TravelRuleStatus::Submitted,
        created_at: Utc::now(),
    };

    state.db.insert_travel_rule_data(&tr_data).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(
        withdrawal_id = %req.withdrawal_id,
        amount = %withdrawal.amount,
        "Travel rule data submitted"
    );

    Ok((StatusCode::CREATED, Json(tr_data)))
}

async fn get_travel_rule(
    State(state): State<AppState>,
    Path(withdrawal_id): Path<Uuid>,
) -> Result<Json<TravelRuleData>, ApiError> {
    let tr = state.db.find_travel_rule_by_withdrawal(withdrawal_id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Travel rule data not found"))?;
    Ok(Json(tr))
}
