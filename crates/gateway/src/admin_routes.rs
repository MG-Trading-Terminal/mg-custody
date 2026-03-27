//! Admin API endpoints — system stats, list deposits/withdrawals, policy management.
//!
//! Admin routes are protected by X-Admin-Key header (if CUSTODY_ADMIN_KEY is set).

use axum::{
    Json, extract::{State, Query},
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::errors::ApiError;
use crate::rbac_middleware::RequireAdmin;
use crate::rate_limit::RateLimiter;
use crate::state::AppState;

/// Build admin API router with separate auth + tighter rate limiting.
pub fn admin_router(state: AppState) -> Router {
    let admin_limiter = Arc::new(RateLimiter::new(20, std::time::Duration::from_secs(60)));
    Router::new()
        .route("/admin/stats", get(system_stats))
        .route("/admin/deposits", get(list_deposits))
        .route("/admin/withdrawals", get(list_withdrawals))
        .route("/admin/pending-approvals", get(pending_approvals))
        .route("/admin/ledger", get(ledger_entries))
        .route("/admin/webhooks/failed", get(failed_webhooks))
        .route("/admin/ws-clients", get(ws_client_count))
        .layer(axum::middleware::from_fn(admin_auth))
        .layer(axum::middleware::from_fn_with_state(admin_limiter, admin_rate_limit))
        .with_state(state)
}

/// Admin authentication middleware.
/// Checks X-Admin-Key header if CUSTODY_ADMIN_KEY env var is set.
async fn admin_auth(request: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    if let Ok(expected_key) = std::env::var("CUSTODY_ADMIN_KEY") {
        let provided = request.headers()
            .get("X-Admin-Key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if provided != expected_key {
            tracing::warn!(path = %request.uri().path(), "Admin auth failed");
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(next.run(request).await)
}

/// Separate rate limiter for admin endpoints (20 req/min).
async fn admin_rate_limit(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let key = request.headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("admin")
        .to_string();

    match limiter.check(&key) {
        Ok(_) => Ok(next.run(request).await),
        Err(_) => {
            tracing::warn!(key = %key, "Admin rate limited");
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

async fn system_stats(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let eth_progress = state.db.get_scan_progress(custody_common::Chain::Ethereum).await.unwrap_or(None);
    let btc_progress = state.db.get_scan_progress(custody_common::Chain::Bitcoin).await.unwrap_or(None);

    let detected = state.db.deposits_by_status(custody_common::DepositStatus::Detected).await
        .map(|d| d.len()).unwrap_or(0);
    let confirmed = state.db.deposits_by_status(custody_common::DepositStatus::Confirmed).await
        .map(|d| d.len()).unwrap_or(0);

    let received = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::Received).await
        .map(|w| w.len()).unwrap_or(0);
    let signed = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::Signed).await
        .map(|w| w.len()).unwrap_or(0);
    let pending_approval = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::PendingApproval).await
        .map(|w| w.len()).unwrap_or(0);

    Ok(Json(serde_json::json!({
        "scan_progress": {
            "ethereum": eth_progress,
            "bitcoin": btc_progress,
        },
        "deposits": {
            "detected": detected,
            "confirmed": confirmed,
        },
        "withdrawals": {
            "received": received,
            "signed": signed,
            "pending_approval": pending_approval,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

async fn list_deposits(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let detected = state.db.deposits_by_status(custody_common::DepositStatus::Detected).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    let confirmed = state.db.deposits_by_status(custody_common::DepositStatus::Confirmed).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({
        "detected": detected.len(),
        "confirmed": confirmed.len(),
        "recent_detected": detected.iter().take(10).map(|d| serde_json::json!({
            "id": d.id, "asset": d.asset.as_str(), "chain": d.chain,
            "amount": d.amount.to_string(), "tx_hash": d.tx_hash,
            "confirmations": d.confirmations, "status": format!("{:?}", d.status),
        })).collect::<Vec<_>>(),
    })))
}

async fn list_withdrawals(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let received = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::Received).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    let signed = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::Signed).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let all: Vec<_> = received.iter().chain(signed.iter())
        .take(20)
        .map(|w| serde_json::json!({
            "id": w.id, "exchange_id": w.exchange_withdrawal_id,
            "asset": w.asset.as_str(), "chain": w.chain,
            "amount": w.amount.to_string(), "destination": w.destination_address,
            "status": format!("{:?}", w.status), "tx_hash": w.tx_hash,
        }))
        .collect();

    Ok(Json(serde_json::json!({
        "total_pending": received.len() + signed.len(),
        "withdrawals": all,
    })))
}

async fn pending_approvals(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pending = state.db.withdrawals_by_status(custody_common::WithdrawalStatus::PendingApproval).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let items: Vec<_> = pending.iter().map(|w| serde_json::json!({
        "id": w.id, "exchange_id": w.exchange_withdrawal_id,
        "asset": w.asset.as_str(), "chain": w.chain,
        "amount": w.amount.to_string(), "destination": w.destination_address,
        "created_at": w.created_at.to_rfc3339(),
    })).collect();

    Ok(Json(serde_json::json!({
        "count": items.len(),
        "approvals": items,
    })))
}

#[derive(Deserialize)]
struct LedgerQuery {
    account: String,
    limit: Option<i64>,
}

async fn ledger_entries(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Query(query): Query<LedgerQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = query.limit.unwrap_or(20);
    let entries = state.db.recent_entries(&query.account, limit).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let items: Vec<_> = entries.iter().map(|e| serde_json::json!({
        "id": e.id,
        "type": format!("{:?}", e.entry_type),
        "account": e.account,
        "amount": e.amount.to_string(),
        "asset": e.asset,
        "chain": e.chain,
        "reference_id": e.reference_id,
        "reference_type": e.reference_type,
        "created_at": e.created_at.to_rfc3339(),
    })).collect();

    Ok(Json(serde_json::json!({
        "account": query.account,
        "count": items.len(),
        "entries": items,
    })))
}

async fn failed_webhooks(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let failed = state.db.failed_webhook_deliveries(50).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    let items: Vec<_> = failed.iter().map(|w| serde_json::json!({
        "id": w.id,
        "event_type": w.event_type,
        "attempts": w.attempts,
        "max_attempts": w.max_attempts,
        "last_error": w.last_error,
        "created_at": w.created_at.to_rfc3339(),
    })).collect();

    Ok(Json(serde_json::json!({
        "count": items.len(),
        "failed_deliveries": items,
    })))
}

async fn ws_client_count(
    _auth: RequireAdmin,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let count = state.event_broadcaster.subscriber_count();
    Json(serde_json::json!({
        "ws_clients": count,
    }))
}
