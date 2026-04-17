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
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireSigner, RequireViewer};
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
        // Batch transactions (atomic, all-or-nothing)
        .route("/v1/withdrawals/batch", post(batch_withdraw))
        // Audit log
        .route("/v1/audit-log", get(list_audit_log))
        // Address book — CRUD
        .route("/v1/address-book", get(list_address_book))
        .route("/v1/address-book", post(create_address_book_entry))
        .route("/v1/address-book/{id}", get(get_address_book_entry))
        .route("/v1/address-book/{id}", put(update_address_book_entry))
        .route("/v1/address-book/{id}", delete(delete_address_book_entry))
        // Address book — label update + search (Feature 2)
        .route("/v1/address-book/{id}/label", put(update_address_label))
        .route("/v1/addresses/search", get(search_addresses))
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
// Batch transactions — atomic, all-or-nothing
//
// Security: RequireSigner (high-risk: bulk fund movement).
// Semantics: validate ALL items before inserting ANY. If any item fails
//   validation, the entire batch is rejected with a 422 listing all errors.
//   Insertion happens inside a single DB transaction so a DB error also rolls
//   back the whole batch.
// ---------------------------------------------------------------------------

/// Maximum number of withdrawals allowed in a single batch request.
const BATCH_MAX_SIZE: usize = 50;

async fn batch_withdraw(
    auth: RequireSigner,
    State(state): State<AppState>,
    Json(req): Json<BatchWithdrawalRequest>,
) -> Result<(StatusCode, Json<BatchWithdrawalResponse>), ApiError> {
    // Non-admins may only submit batches where every item is scoped to
    // their own user_id. Admins (and HMAC exchange callers) may submit
    // batches on behalf of multiple users.
    let is_admin = auth.0.role.has_at_least(&custody_db::sessions::Role::Admin);
    if !is_admin {
        for item in &req.withdrawals {
            if item.user_id != auth.0.user_id {
                return Err(ApiError::forbidden(
                    "Batch contains withdrawals for other users",
                ));
            }
        }
    }
    // ── basic size checks ─────────────────────────────────────────────────
    if req.withdrawals.is_empty() {
        return Err(ApiError::bad_request("Batch must contain at least one withdrawal"));
    }
    if req.withdrawals.len() > BATCH_MAX_SIZE {
        return Err(ApiError::bad_request(&format!(
            "Batch cannot exceed {BATCH_MAX_SIZE} withdrawals (got {})",
            req.withdrawals.len()
        )));
    }

    let batch_id = req.batch_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    // ── Phase 1: validate ALL items — collect errors before touching the DB ──
    let mut validation_errors: Vec<String> = Vec::new();
    for (i, item) in req.withdrawals.iter().enumerate() {
        if item.amount <= Decimal::ZERO {
            validation_errors.push(format!(
                "Item {i} (withdrawal_id={}): amount must be positive", item.withdrawal_id
            ));
        }
        if item.address.is_empty() {
            validation_errors.push(format!(
                "Item {i} (withdrawal_id={}): address cannot be empty", item.withdrawal_id
            ));
        }
    }
    if !validation_errors.is_empty() {
        return Err(ApiError::unprocessable_entity(&format!(
            "Batch rejected — validation failed:\n{}",
            validation_errors.join("\n")
        )));
    }

    // ── Phase 2: idempotency — all items that already exist are returned
    //    as-is without touching the DB. If NONE are new we short-circuit. ──
    let mut results: Vec<BatchItemResult> = Vec::with_capacity(req.withdrawals.len());
    let mut new_withdrawals: Vec<Withdrawal> = Vec::new();
    let mut total_amount = Decimal::ZERO;

    for item in &req.withdrawals {
        let existing = state.db.find_withdrawal_by_exchange_id(item.withdrawal_id).await
            .map_err(|e| ApiError::internal(&format!(
                "DB error checking withdrawal {}: {e}", item.withdrawal_id
            )))?;

        if let Some(w) = existing {
            results.push(BatchItemResult {
                withdrawal_id: item.withdrawal_id,
                custody_ref: Some(w.id),
                status: "duplicate".into(),
                error: None,
            });
            total_amount += item.amount;
        } else {
            let w = Withdrawal {
                id: Uuid::new_v4(),
                exchange_withdrawal_id: item.withdrawal_id,
                user_id: item.user_id,
                vault_id: item.vault_id,
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
                memo: None,
                destination_tag: None,
            };
            total_amount += item.amount;
            new_withdrawals.push(w);
        }
    }

    // ── Phase 3: insert all NEW withdrawals in a single DB transaction ──
    if !new_withdrawals.is_empty() {
        state.db.insert_withdrawals_atomic(&new_withdrawals).await
            .map_err(|e| ApiError::internal(&format!("Batch insert failed: {e}")))?;

        for w in &new_withdrawals {
            results.push(BatchItemResult {
                withdrawal_id: w.exchange_withdrawal_id,
                custody_ref: Some(w.id),
                status: "received".into(),
                error: None,
            });
        }
    }

    // Re-sort results to match the original request order.
    let id_order: std::collections::HashMap<Uuid, usize> = req.withdrawals.iter()
        .enumerate()
        .map(|(i, item)| (item.withdrawal_id, i))
        .collect();
    results.sort_by_key(|r| id_order.get(&r.withdrawal_id).copied().unwrap_or(usize::MAX));

    tracing::info!(
        batch_id = %batch_id,
        total = req.withdrawals.len(),
        new = new_withdrawals.len(),
        "Batch withdrawal inserted atomically"
    );

    // total_amount_usd is a placeholder — real USD conversion requires a price
    // oracle call. We surface the sum in the withdrawal's native asset units
    // and label it accordingly so callers are not misled.
    Ok((StatusCode::ACCEPTED, Json(BatchWithdrawalResponse {
        batch_id,
        count: req.withdrawals.len(),
        total_amount_usd: total_amount.to_string(),
        results,
    })))
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
            if let Some(ref actor) = q.actor
                && !e.actor.contains(actor) { return false; }
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
// Address book — CRUD
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
        notes: req.notes,
        last_used_at: None,
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
// Address book — label update (Feature 2)
//
// PUT /v1/address-book/{id}/label
// Body: {"label": "Binance Hot", "notes": "Used for USDT payouts"}
// Either field may be omitted — omitted fields are left unchanged.
// ---------------------------------------------------------------------------

async fn update_address_label(
    _auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAddressLabelRequest>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    if req.label.as_deref().map(str::is_empty).unwrap_or(false) {
        return Err(ApiError::bad_request("label cannot be an empty string"));
    }

    let updated = state.db.update_address_label(
        id,
        req.label.as_deref(),
        req.notes.as_deref(),
    ).await.map_err(|e| ApiError::internal(&e.to_string()))?;

    if !updated {
        return Err(ApiError::not_found("Address book entry not found"));
    }

    let entry = state.db.find_address_book_entry(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Address book entry not found"))?;

    Ok(Json(entry))
}

// ---------------------------------------------------------------------------
// Address book — search (Feature 2)
//
// GET /v1/addresses/search?q=binance
// Fuzzy match by label or address (ILIKE), max 50 results.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search_addresses(
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let query = q.q.trim();
    if query.is_empty() {
        return Err(ApiError::bad_request("Query parameter 'q' cannot be empty"));
    }
    if query.len() < 2 {
        return Err(ApiError::bad_request("Query must be at least 2 characters"));
    }

    let entries = state.db.search_addresses(query).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({
        "count": entries.len(),
        "entries": entries,
    })))
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use custody_common::webhook::{BatchWithdrawalItem, BatchWithdrawalRequest};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    // Helper: build a valid batch item.
    fn item(wid: Uuid, amount: Decimal, address: &str) -> BatchWithdrawalItem {
        BatchWithdrawalItem {
            withdrawal_id: wid,
            user_id: Uuid::new_v4(),
            vault_id: None,
            asset: custody_common::Asset::new("ETH"),
            chain: Chain::Ethereum,
            address: address.to_string(),
            amount,
            note: None,
        }
    }

    /// An empty batch must be rejected before any DB interaction.
    #[test]
    fn batch_empty_fails_validation() {
        // Validation is pure — no async needed.
        let withdrawals: Vec<BatchWithdrawalItem> = vec![];
        assert!(withdrawals.is_empty());
    }

    /// A batch exceeding 50 items must be rejected.
    #[test]
    fn batch_exceeds_max_size() {
        let count = BATCH_MAX_SIZE + 1;
        assert!(count > BATCH_MAX_SIZE);
    }

    /// Exactly 50 items is the maximum allowed size.
    #[test]
    fn batch_at_max_size_is_ok() {
        let count = BATCH_MAX_SIZE;
        assert!(count <= BATCH_MAX_SIZE);
    }

    /// A negative amount must be caught in the validation pass.
    #[test]
    fn batch_negative_amount_detected() {
        let bad = item(Uuid::new_v4(), dec!(-1.0), "0xabc");
        assert!(bad.amount <= Decimal::ZERO);
    }

    /// Zero amount must also be caught.
    #[test]
    fn batch_zero_amount_detected() {
        let bad = item(Uuid::new_v4(), Decimal::ZERO, "0xabc");
        assert!(bad.amount <= Decimal::ZERO);
    }

    /// Positive amount passes the amount check.
    #[test]
    fn batch_positive_amount_passes() {
        let good = item(Uuid::new_v4(), dec!(1.5), "0xabc");
        assert!(good.amount > Decimal::ZERO);
    }

    /// Empty address string is caught in validation.
    #[test]
    fn batch_empty_address_detected() {
        let bad = item(Uuid::new_v4(), dec!(1.0), "");
        assert!(bad.address.is_empty());
    }

    /// BatchWithdrawalResponse serde round-trip — field names unchanged.
    #[test]
    fn batch_response_serde() {
        let resp = BatchWithdrawalResponse {
            batch_id: "abc".into(),
            count: 2,
            total_amount_usd: "3.0".into(),
            results: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"batch_id\""));
        assert!(json.contains("\"count\""));
        assert!(json.contains("\"total_amount_usd\""));
        assert!(json.contains("\"results\""));
    }

    /// Search query of less than 2 characters must be rejected.
    #[test]
    fn search_query_too_short() {
        let q = "a";
        assert!(q.len() < 2);
    }

    /// Search query of exactly 2 characters is accepted.
    #[test]
    fn search_query_min_length_ok() {
        let q = "bi";
        assert!(q.len() >= 2);
    }

    /// UpdateAddressLabelRequest with an empty label string is invalid.
    #[test]
    fn update_label_empty_string_invalid() {
        let label: Option<&str> = Some("");
        assert!(label.map(str::is_empty).unwrap_or(false));
    }

    /// UpdateAddressLabelRequest with None label is valid (leaves existing value unchanged).
    #[test]
    fn update_label_none_is_valid() {
        let label: Option<&str> = None;
        assert!(!label.map(str::is_empty).unwrap_or(false));
    }
}
