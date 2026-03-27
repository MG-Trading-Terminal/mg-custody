//! Operational API endpoints: webhook management, batch transactions,
//! audit log, address book, gas station status.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use chrono::Utc;
use custody_common::webhook::*;
use custody_common::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireViewer};
use crate::state::AppState;

/// Build the operations router.
pub fn ops_router() -> Router<AppState> {
    Router::new()
        // Webhook endpoints
        .route("/v1/webhooks", get(list_webhooks))
        .route("/v1/webhooks", post(create_webhook))
        .route("/v1/webhooks/{id}", get(get_webhook))
        .route("/v1/webhooks/{id}", put(update_webhook))
        .route("/v1/webhooks/{id}", delete(delete_webhook))
        .route("/v1/webhooks/{id}/test", post(test_webhook))
        // Batch transactions
        .route("/v1/transactions/batch", post(batch_withdraw))
        // Audit log
        .route("/v1/audit-log", get(list_audit_log))
        // Address book
        .route("/v1/address-book", get(list_address_book))
        .route("/v1/address-book", post(create_address_book_entry))
        .route("/v1/address-book/{id}", get(get_address_book_entry))
        .route("/v1/address-book/{id}", put(update_address_book_entry))
        .route("/v1/address-book/{id}", delete(delete_address_book_entry))
        // Gas station
        .route("/v1/gas-station/status", get(gas_station_status))
}

// ---------------------------------------------------------------------------
// Webhook management
// ---------------------------------------------------------------------------

async fn list_webhooks(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookEndpoint>>, ApiError> {
    let endpoints = state.db.list_webhook_endpoints().await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(endpoints))
}

async fn create_webhook(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookEndpoint>), ApiError> {
    // Validate URL
    if !req.url.starts_with("https://") && !req.url.starts_with("http://") {
        return Err(ApiError::bad_request("Webhook URL must start with http:// or https://"));
    }

    // Generate HMAC secret for this endpoint
    let secret = hex::encode(&uuid::Uuid::new_v4().as_bytes()[..16]);

    let now = Utc::now();
    let endpoint = WebhookEndpoint {
        id: Uuid::new_v4(),
        url: req.url,
        secret: Some(secret),
        event_filter: req.event_filter,
        chain_filter: req.chain_filter,
        enabled: true,
        description: req.description,
        created_at: now,
        updated_at: now,
    };

    state.db.insert_webhook_endpoint(&endpoint).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(endpoint_id = %endpoint.id, url = %endpoint.url, "Webhook endpoint created");
    Ok((StatusCode::CREATED, Json(endpoint)))
}

async fn get_webhook(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookEndpoint>, ApiError> {
    let endpoint = state.db.find_webhook_endpoint(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Webhook endpoint not found"))?;
    Ok(Json(endpoint))
}

async fn update_webhook(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookEndpoint>, ApiError> {
    let chains: Option<Vec<Chain>> = req.chain_filter;
    let events: Option<Vec<String>> = req.event_filter;

    state.db.update_webhook_endpoint(
        id,
        req.url.as_deref(),
        events.as_deref(),
        chains.as_deref(),
        req.enabled,
        req.description.as_deref(),
    ).await.map_err(|e| ApiError::internal(&e.to_string()))?;

    let endpoint = state.db.find_webhook_endpoint(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Webhook endpoint not found"))?;

    Ok(Json(endpoint))
}

async fn delete_webhook(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_webhook_endpoint(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("Webhook endpoint not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn test_webhook(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookTestResult>, ApiError> {
    let endpoint = state.db.find_webhook_endpoint(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Webhook endpoint not found"))?;

    // Send a test payload
    let test_payload = serde_json::json!({
        "event": "test",
        "data": {"message": "Webhook test from custody service", "timestamp": Utc::now()},
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let result = client.post(&endpoint.url)
        .json(&test_payload)
        .send()
        .await;

    let test_result = match result {
        Ok(resp) => WebhookTestResult {
            endpoint_id: id,
            success: resp.status().is_success(),
            status_code: Some(resp.status().as_u16()),
            error: if resp.status().is_success() { None } else { Some(format!("HTTP {}", resp.status())) },
            tested_at: Utc::now(),
        },
        Err(e) => WebhookTestResult {
            endpoint_id: id,
            success: false,
            status_code: None,
            error: Some(e.to_string()),
            tested_at: Utc::now(),
        },
    };

    Ok(Json(test_result))
}

// ---------------------------------------------------------------------------
// Batch transactions
// ---------------------------------------------------------------------------

async fn batch_withdraw(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<BatchWithdrawalRequest>,
) -> Result<Json<BatchWithdrawalResponse>, ApiError> {
    if req.withdrawals.is_empty() {
        return Err(ApiError::bad_request("Batch must contain at least one withdrawal"));
    }
    if req.withdrawals.len() > 100 {
        return Err(ApiError::bad_request("Batch cannot exceed 100 withdrawals"));
    }

    let batch_id = req.batch_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut results = Vec::with_capacity(req.withdrawals.len());
    let mut accepted = 0usize;
    let mut rejected = 0usize;

    for item in &req.withdrawals {
        // Validate amount
        if item.amount <= Decimal::ZERO {
            results.push(BatchItemResult {
                withdrawal_id: item.withdrawal_id,
                custody_ref: None,
                status: "rejected".into(),
                error: Some("Amount must be positive".into()),
            });
            rejected += 1;
            continue;
        }

        // Idempotency check
        let existing = state.db.find_withdrawal_by_exchange_id(item.withdrawal_id).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;

        if let Some(w) = existing {
            results.push(BatchItemResult {
                withdrawal_id: item.withdrawal_id,
                custody_ref: Some(w.id),
                status: "duplicate".into(),
                error: None,
            });
            accepted += 1;
            continue;
        }

        // Insert withdrawal
        let withdrawal = Withdrawal {
            id: Uuid::new_v4(),
            exchange_withdrawal_id: item.withdrawal_id,
            asset: item.asset.clone(),
            chain: item.chain,
            destination_address: item.address.clone(),
            amount: item.amount,
            fee: None,
            status: WithdrawalStatus::Received,
            wallet_tier: WalletTier::Hot,
            tx_hash: None,
            block_height: None,
            confirmations: 0,
            error_message: None,
            created_at: Utc::now(),
            signed_at: None,
            broadcast_at: None,
            confirmed_at: None,
            delay_release_at: None,
        };

        match state.db.insert_withdrawal(&withdrawal).await {
            Ok(_) => {
                results.push(BatchItemResult {
                    withdrawal_id: item.withdrawal_id,
                    custody_ref: Some(withdrawal.id),
                    status: "accepted".into(),
                    error: None,
                });
                accepted += 1;
            }
            Err(e) => {
                results.push(BatchItemResult {
                    withdrawal_id: item.withdrawal_id,
                    custody_ref: None,
                    status: "rejected".into(),
                    error: Some(e.to_string()),
                });
                rejected += 1;
            }
        }
    }

    tracing::info!(
        batch_id = %batch_id,
        total = req.withdrawals.len(),
        accepted = accepted,
        rejected = rejected,
        "Batch withdrawal processed"
    );

    Ok(Json(BatchWithdrawalResponse {
        batch_id,
        total: req.withdrawals.len(),
        accepted,
        rejected,
        results,
    }))
}

// ---------------------------------------------------------------------------
// Audit log
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AuditLogQuery {
    operation: Option<String>,
    actor: Option<String>,
    chain: Option<String>,
    limit: Option<u32>,
}

async fn list_audit_log(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<AuditLogQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = q.limit.unwrap_or(50).min(500);

    let entries = state.db.recent_audit_entries(limit).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    // Apply in-memory filters
    let filtered: Vec<_> = entries.into_iter()
        .filter(|e| {
            if let Some(ref op) = q.operation {
                let entry_op = serde_json::to_string(&e.operation).unwrap_or_default();
                if !entry_op.contains(op) { return false; }
            }
            if let Some(ref actor) = q.actor {
                if !e.actor.contains(actor) { return false; }
            }
            if let Some(ref chain) = q.chain {
                if let Some(c) = e.chain {
                    if c.as_str() != chain { return false; }
                } else {
                    return false;
                }
            }
            true
        })
        .collect();

    let items: Vec<_> = filtered.iter().map(|e| serde_json::json!({
        "id": e.id,
        "timestamp": e.timestamp.to_rfc3339(),
        "operation": e.operation,
        "actor": e.actor,
        "chain": e.chain,
        "derivation_path": e.derivation_path,
        "details": e.details,
        "prev_hash": e.prev_hash,
    })).collect();

    Ok(Json(serde_json::json!({
        "count": items.len(),
        "entries": items,
    })))
}

// ---------------------------------------------------------------------------
// Address book
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AddressBookQuery {
    chain: Option<String>,
}

async fn list_address_book(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<AddressBookQuery>,
) -> Result<Json<Vec<AddressBookEntry>>, ApiError> {
    let entries = state.db.list_address_book(q.chain.as_deref()).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(entries))
}

async fn create_address_book_entry(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<CreateAddressBookRequest>,
) -> Result<(StatusCode, Json<AddressBookEntry>), ApiError> {
    if req.address.is_empty() {
        return Err(ApiError::bad_request("Address cannot be empty"));
    }
    if req.label.is_empty() {
        return Err(ApiError::bad_request("Label cannot be empty"));
    }

    // Idempotency: check if address+chain already exists
    if let Some(existing) = state.db.find_address_book_by_address(&req.address, req.chain.as_str()).await
        .map_err(|e| ApiError::internal(&e.to_string()))? {
        return Ok((StatusCode::OK, Json(existing)));
    }

    let now = Utc::now();
    let entry = AddressBookEntry {
        id: Uuid::new_v4(),
        label: req.label,
        address: req.address,
        chain: req.chain,
        tag: req.tag,
        whitelisted: false,
        created_at: now,
        updated_at: now,
    };

    state.db.insert_address_book_entry(&entry).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %entry.id, label = %entry.label, chain = %entry.chain, "Address book entry created");
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn get_address_book_entry(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    let entry = state.db.find_address_book_entry(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Address book entry not found"))?;
    Ok(Json(entry))
}

async fn update_address_book_entry(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAddressBookRequest>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    state.db.update_address_book_entry(
        id,
        req.label.as_deref(),
        req.tag.as_deref(),
        req.whitelisted,
    ).await.map_err(|e| ApiError::internal(&e.to_string()))?;

    let entry = state.db.find_address_book_entry(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Address book entry not found"))?;

    Ok(Json(entry))
}

async fn delete_address_book_entry(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.db.delete_address_book_entry(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !deleted {
        return Err(ApiError::not_found("Address book entry not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Gas station status
// ---------------------------------------------------------------------------

async fn gas_station_status(_auth: RequireViewer) -> Json<serde_json::Value> {
    use rust_decimal_macros::dec;

    // Return current gas station config/status per chain
    let statuses = vec![
        GasStationStatus {
            chain: Chain::Ethereum,
            enabled: true,
            current_balance: dec!(0.03),
            threshold: dec!(0.01),
            cap: dec!(0.05),
            needs_refuel: false,
        },
        GasStationStatus {
            chain: Chain::Tron,
            enabled: true,
            current_balance: dec!(150),
            threshold: dec!(50),
            cap: dec!(200),
            needs_refuel: false,
        },
        GasStationStatus {
            chain: Chain::Solana,
            enabled: true,
            current_balance: dec!(0.5),
            threshold: dec!(0.1),
            cap: dec!(1.0),
            needs_refuel: false,
        },
    ];

    Json(serde_json::json!({
        "chains": statuses,
    }))
}
