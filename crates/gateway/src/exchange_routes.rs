//! Exchange-facing API endpoints.
//!
//! These are what SelfHostedCustodyProvider calls from the exchange.
//! Extends the base /v1/* routes with additional exchange-specific functionality.

use axum::{Json, Router, extract::{Path, State, Query}, routing::get};
use serde::Deserialize;
use crate::errors::ApiError;
use crate::state::AppState;

/// Additional exchange API routes.
pub fn exchange_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/deposits", get(list_user_deposits))
        .route("/v1/addresses/chain/{chain}", get(list_chain_addresses))
        .route("/v1/system/supported-assets", get(supported_assets))
        .route("/v1/system/limits", get(system_limits))
        .with_state(state)
}

#[derive(Deserialize)]
struct DepositQuery {
    user_id: Option<uuid::Uuid>,
    status: Option<String>,
    limit: Option<u32>,
}

async fn list_user_deposits(
    State(state): State<AppState>,
    Query(query): Query<DepositQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Query deposits, optionally filtered by user_id and status
    let status = match query.status.as_deref() {
        Some("detected") => Some(custody_common::DepositStatus::Detected),
        Some("confirmed") => Some(custody_common::DepositStatus::Confirmed),
        Some("credited") => Some(custody_common::DepositStatus::Credited),
        _ => None,
    };

    let deposits = if let Some(s) = status {
        state.db.deposits_by_status(s).await
            .map_err(|e| ApiError::internal(&e.to_string()))?
    } else {
        // Return detected + confirmed
        let mut all = state.db.deposits_by_status(custody_common::DepositStatus::Detected).await
            .map_err(|e| ApiError::internal(&e.to_string()))?;
        all.extend(state.db.deposits_by_status(custody_common::DepositStatus::Confirmed).await
            .map_err(|e| ApiError::internal(&e.to_string()))?);
        all
    };

    let limit = query.limit.unwrap_or(50) as usize;
    let filtered: Vec<_> = deposits.iter()
        .filter(|d| query.user_id.map_or(true, |uid| d.user_id == uid))
        .take(limit)
        .map(|d| serde_json::json!({
            "id": d.id, "user_id": d.user_id,
            "asset": d.asset.as_str(), "chain": d.chain,
            "amount": d.amount.to_string(), "tx_hash": d.tx_hash,
            "confirmations": d.confirmations, "required": d.required_confirmations,
            "status": format!("{:?}", d.status),
            "detected_at": d.detected_at.to_rfc3339(),
        }))
        .collect();

    Ok(Json(serde_json::json!({"deposits": filtered, "count": filtered.len()})))
}

async fn list_chain_addresses(
    State(state): State<AppState>,
    Path(chain): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let chain_enum = match chain.as_str() {
        "ethereum" | "eth" => custody_common::Chain::Ethereum,
        "bitcoin" | "btc" => custody_common::Chain::Bitcoin,
        "tron" | "trx" => custody_common::Chain::Tron,
        "solana" | "sol" => custody_common::Chain::Solana,
        _ => return Err(ApiError::bad_request(&format!("Unknown chain: {chain}"))),
    };

    let addresses = state.db.all_addresses_for_chain(chain_enum).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    Ok(Json(serde_json::json!({
        "chain": chain, "count": addresses.len(), "addresses": addresses,
    })))
}

async fn supported_assets() -> Json<serde_json::Value> {
    let registry = custody_common::token_registry::TokenRegistry::with_defaults();
    let chains = [custody_common::Chain::Ethereum, custody_common::Chain::Bitcoin,
                  custody_common::Chain::Tron, custody_common::Chain::Solana];
    let symbols = ["BTC", "ETH", "SOL", "TRX", "USDT", "USDC"];

    let mut assets = Vec::new();
    for symbol in &symbols {
        for chain in &chains {
            if let Some(t) = registry.get(symbol, *chain) {
                assets.push(serde_json::json!({
                    "symbol": t.symbol, "chain": chain.as_str(),
                    "decimals": t.decimals, "is_native": t.is_native,
                    "contract": t.contract_address,
                }));
            }
        }
    }

    Json(serde_json::json!({"assets": assets, "count": assets.len()}))
}

async fn system_limits() -> Json<serde_json::Value> {
    // Load from config
    let config = custody_common::CustodyConfig::load_from("config/default.toml")
        .unwrap_or_else(|_| custody_common::CustodyConfig::load().unwrap());

    Json(serde_json::json!({
        "max_single_withdrawal_usd": config.limits.max_single_withdrawal_usd.to_string(),
        "max_hourly_outflow_usd": config.limits.max_hourly_outflow_usd.to_string(),
        "max_daily_outflow_usd": config.limits.max_daily_outflow_usd.to_string(),
        "auto_approve_threshold_usd": config.limits.auto_approve_threshold_usd.to_string(),
        "chains": {
            "ethereum": config.chains.ethereum.as_ref().map(|c| serde_json::json!({
                "enabled": c.enabled, "confirmations": c.required_confirmations,
                "max_gas_gwei": c.max_gas_price_gwei,
            })),
            "bitcoin": config.chains.bitcoin.as_ref().map(|c| serde_json::json!({
                "enabled": c.enabled, "confirmations": c.required_confirmations,
            })),
        },
    }))
}
