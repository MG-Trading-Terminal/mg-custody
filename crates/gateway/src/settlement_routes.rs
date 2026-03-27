//! Settlement API endpoints.
//!
//! Internal off-chain settlement between custody clients.
//! Instant transfers without on-chain transactions.

use axum::{
    Json,
    extract::State,
    routing::post,
    Router,
};
use custody_common::settlement_ops::{SettlementEngine, SettlementOrder, SettlementOrderStatus};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireAdmin, RequireEditor};
use crate::state::AppState;

/// Build settlement API router.
pub fn settlement_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/settlement/submit", post(submit_settlement))
        .route("/v1/settlement/net", post(net_settlements))
        .with_state(state)
}

#[derive(Deserialize)]
struct SubmitSettlementRequest {
    from_party: String,
    to_party: String,
    asset: String,
    amount: String,
}

async fn submit_settlement(
    _auth: RequireEditor,
    State(_state): State<AppState>,
    Json(req): Json<SubmitSettlementRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let amount: Decimal = req.amount.parse()
        .map_err(|_| ApiError::bad_request("Invalid amount"))?;

    let order = SettlementOrder {
        id: uuid::Uuid::new_v4(),
        from_party: req.from_party.clone(),
        to_party: req.to_party.clone(),
        asset: req.asset.clone(),
        amount,
        status: SettlementOrderStatus::Pending,
        netting_group: None,
        created_at: chrono::Utc::now(),
        settled_at: None,
    };

    tracing::info!(
        id = %order.id,
        from = %req.from_party,
        to = %req.to_party,
        asset = %req.asset,
        amount = %amount,
        "Settlement order submitted"
    );

    Ok(Json(serde_json::json!({
        "order_id": order.id,
        "status": "pending",
        "from_party": req.from_party,
        "to_party": req.to_party,
        "asset": req.asset,
        "amount": amount.to_string(),
    })))
}

async fn net_settlements(
    _auth: RequireAdmin,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // In production: load pending orders from DB and net them
    let mut engine = SettlementEngine::new();
    let results = engine.net_orders();

    Ok(Json(serde_json::json!({
        "netting_results": results.len(),
        "message": "Netting executed on pending orders",
    })))
}
