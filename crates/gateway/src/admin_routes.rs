//! Admin API endpoints — system stats, list deposits/withdrawals, policy management.

use axum::{
    Json, extract::State,
    routing::get,
    Router,
};
use crate::state::AppState;
use crate::errors::ApiError;

/// Build admin API router (separate from public API).
pub fn admin_router(state: AppState) -> Router {
    Router::new()
        .route("/admin/stats", get(system_stats))
        .route("/admin/deposits", get(list_deposits))
        .route("/admin/withdrawals", get(list_withdrawals))
        .route("/admin/pending-approvals", get(pending_approvals))
        .with_state(state)
}

async fn system_stats(
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
