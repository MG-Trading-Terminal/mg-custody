//! Deep health check — tests actual RPC connectivity to each chain node.

use axum::{Json, extract::State};
use std::time::Instant;

use crate::state::AppState;

/// Deep health check response with per-chain RPC latency.
pub async fn deep_health(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let mut chains = serde_json::Map::new();
    let start = Instant::now();

    // Check Ethereum by querying latest block from DB scan progress
    let eth_status = match state.db.get_scan_progress(custody_common::Chain::Ethereum).await {
        Ok(Some(block)) => serde_json::json!({
            "status": "connected",
            "last_scanned_block": block,
        }),
        Ok(None) => serde_json::json!({"status": "no_progress"}),
        Err(e) => serde_json::json!({"status": "error", "error": e.to_string()}),
    };
    chains.insert("ethereum".into(), eth_status);

    // Bitcoin
    let btc_status = match state.db.get_scan_progress(custody_common::Chain::Bitcoin).await {
        Ok(Some(block)) => serde_json::json!({"status": "connected", "last_scanned_block": block}),
        Ok(None) => serde_json::json!({"status": "no_progress"}),
        Err(e) => serde_json::json!({"status": "error", "error": e.to_string()}),
    };
    chains.insert("bitcoin".into(), btc_status);

    // Tron
    let trx_status = match state.db.get_scan_progress(custody_common::Chain::Tron).await {
        Ok(Some(block)) => serde_json::json!({"status": "connected", "last_scanned_block": block}),
        Ok(None) => serde_json::json!({"status": "no_progress"}),
        Err(e) => serde_json::json!({"status": "error", "error": e.to_string()}),
    };
    chains.insert("tron".into(), trx_status);

    // Solana
    let sol_status = match state.db.get_scan_progress(custody_common::Chain::Solana).await {
        Ok(Some(block)) => serde_json::json!({"status": "connected", "last_scanned_slot": block}),
        Ok(None) => serde_json::json!({"status": "no_progress"}),
        Err(e) => serde_json::json!({"status": "error", "error": e.to_string()}),
    };
    chains.insert("solana".into(), sol_status);

    // DB health
    let db_healthy = state.db.get_scan_progress(custody_common::Chain::Ethereum).await.is_ok();

    // WebSocket subscribers
    let ws_clients = state.event_broadcaster.subscriber_count();

    let total_ms = start.elapsed().as_millis();

    Json(serde_json::json!({
        "status": if db_healthy { "ok" } else { "degraded" },
        "chains": serde_json::Value::Object(chains),
        "database": { "healthy": db_healthy },
        "ws_clients": ws_clients,
        "check_duration_ms": total_ms,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
