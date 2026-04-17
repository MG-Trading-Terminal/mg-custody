//! Settlement API endpoints — **feature gated off**.
//!
//! Internal off-chain settlement between custody clients was scaffolded
//! as a pair of routes but the execution path was a lie:
//!
//!   - `POST /v1/settlement/submit` took an order, built an in-memory
//!     `SettlementOrder`, logged it, and returned `status: "pending"`.
//!     The row was never persisted; `/v1/settlement/net` could not
//!     find it because it queried a fresh `SettlementEngine::new()`
//!     every call, not the DB.
//!   - `POST /v1/settlement/net` built a fresh empty engine, ran
//!     `net_orders()` on zero orders, and returned
//!     `"Netting executed on pending orders"` — a phrase that is
//!     *literally false* when no orders exist and no DB table is
//!     even queried.
//!
//! A future real implementation needs (a) a `settlement_orders`
//! table, (b) a per-party balance view, (c) an atomic netting pass
//! that debits/credits the double-entry ledger — none of which is
//! wired. Until that lands both routes return 501 so callers can't
//! build workflows on fake success.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    routing::post,
    Router,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

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

fn settlement_unavailable(detail: &'static str) -> ApiError {
    ApiError {
        error: "not_implemented".into(),
        message: format!(
            "Off-chain settlement is not wired to a persistence layer in this build. {detail}"
        ),
        request_id: Uuid::new_v4().to_string(),
        status: StatusCode::NOT_IMPLEMENTED,
    }
}

async fn submit_settlement(
    _auth: RequireEditor,
    State(_state): State<AppState>,
    Json(req): Json<SubmitSettlementRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Parse + validate up front so the shape feedback is still useful
    // for integrators testing the API surface before the feature lands.
    let amount: Decimal = req.amount.parse()
        .map_err(|_| ApiError::bad_request("Invalid amount — must be string-encoded decimal"))?;
    if amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("Amount must be positive"));
    }
    if req.from_party.is_empty() || req.to_party.is_empty() {
        return Err(ApiError::bad_request("from_party and to_party are required"));
    }
    if req.from_party == req.to_party {
        return Err(ApiError::bad_request("from_party and to_party must differ"));
    }

    tracing::warn!(
        from = %req.from_party,
        to = %req.to_party,
        asset = %req.asset,
        amount = %amount,
        "Settlement submit rejected — no persistence wired (501)"
    );

    Err(settlement_unavailable(
        "Orders are not persisted; netting has no inputs. Wire the settlement_orders migration before enabling.",
    ))
}

async fn net_settlements(
    _auth: RequireAdmin,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::warn!("Netting rejected — no persistence wired (501)");
    Err(settlement_unavailable(
        "Netting requires loading persisted orders and journaling results against the ledger; neither is wired.",
    ))
}
