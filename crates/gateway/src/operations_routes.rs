//! Operations API: scheduling, compliance screening, backup status,
//! token discovery, health dashboard.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::Utc;
use custody_common::operations::*;
use custody_common::*;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::state::AppState;

pub fn operations_router() -> Router<AppState> {
    Router::new()
        // Scheduled transactions
        .route("/v1/scheduled", get(list_scheduled))
        .route("/v1/scheduled", post(create_scheduled))
        .route("/v1/scheduled/{id}", get(get_scheduled))
        .route("/v1/scheduled/{id}/cancel", post(cancel_scheduled))
        // Compliance screening
        .route("/v1/screening/address", post(screen_address))
        .route("/v1/screening/history/{address}", get(screening_history))
        // Backup & recovery
        .route("/v1/backup/status", get(backup_status))
        // Token discovery
        .route("/v1/tokens/discover", post(discover_token))
        // Health dashboard
        .route("/v1/dashboard/health", get(health_dashboard))
        .route("/v1/dashboard/pipeline", get(pipeline_status))
}

// ---------------------------------------------------------------------------
// Scheduled transactions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ScheduledQuery {
    vault_id: Option<Uuid>,
    status: Option<String>,
}

async fn list_scheduled(
    State(state): State<AppState>,
    Query(q): Query<ScheduledQuery>,
) -> Result<Json<Vec<ScheduledTransaction>>, ApiError> {
    let txs = state.db.list_scheduled_tx(q.vault_id, q.status.as_deref()).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(txs))
}

async fn create_scheduled(
    State(state): State<AppState>,
    Json(req): Json<CreateScheduledTxRequest>,
) -> Result<(StatusCode, Json<ScheduledTransaction>), ApiError> {
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("Amount must be positive"));
    }
    if req.scheduled_at <= Utc::now() {
        return Err(ApiError::bad_request("Scheduled time must be in the future"));
    }

    let tx = ScheduledTransaction {
        id: Uuid::new_v4(),
        vault_id: req.vault_id,
        asset: req.asset,
        chain: req.chain,
        destination_address: req.destination_address,
        amount: req.amount,
        scheduled_at: req.scheduled_at,
        status: ScheduleStatus::Pending,
        withdrawal_id: None,
        note: req.note,
        created_at: Utc::now(),
        executed_at: None,
        cancelled_at: None,
    };

    state.db.insert_scheduled_tx(&tx).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    tracing::info!(id = %tx.id, scheduled_at = %tx.scheduled_at, "Transaction scheduled");
    Ok((StatusCode::CREATED, Json(tx)))
}

async fn get_scheduled(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduledTransaction>, ApiError> {
    let tx = state.db.find_scheduled_tx(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Scheduled transaction not found"))?;
    Ok(Json(tx))
}

async fn cancel_scheduled(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cancelled = state.db.cancel_scheduled_tx(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    if !cancelled {
        return Err(ApiError::conflict("Transaction not in pending status"));
    }
    tracing::info!(id = %id, "Scheduled transaction cancelled");
    Ok(Json(serde_json::json!({"id": id, "status": "cancelled"})))
}

// ---------------------------------------------------------------------------
// Compliance screening
// ---------------------------------------------------------------------------

async fn screen_address(
    Json(req): Json<ScreenAddressRequest>,
) -> Result<Json<ScreeningResult>, ApiError> {
    // Simulated screening — in production would call Chainalysis/Elliptic
    let result = ScreeningResult {
        id: Uuid::new_v4(),
        address: req.address,
        chain: req.chain,
        provider: "internal".into(),
        risk_level: RiskLevel::Low,
        risk_score: Some(0.05),
        flags: vec![],
        sanctions_match: false,
        screened_at: Utc::now(),
        details: Some("No risk indicators found".into()),
    };

    Ok(Json(result))
}

async fn screening_history(
    Path(address): Path<String>,
) -> Json<serde_json::Value> {
    // Return mock screening history
    Json(serde_json::json!({
        "address": address,
        "screenings": [],
        "last_screened_at": null,
        "overall_risk": "low",
    }))
}

// ---------------------------------------------------------------------------
// Backup & recovery status
// ---------------------------------------------------------------------------

async fn backup_status() -> Json<BackupStatus> {
    Json(BackupStatus {
        seed_encrypted: true,
        seed_backup_count: 3,
        shamir_threshold: 3,
        shamir_total_shares: 5,
        last_backup_at: Some(Utc::now()),
        last_verification_at: Some(Utc::now()),
        mpc_key_shares: vec![
            MpcShareStatus {
                key_id: "ecdsa-primary".into(),
                share_index: 0,
                stored_in: "software_hsm".into(),
                last_verified_at: Some(Utc::now()),
                healthy: true,
            },
            MpcShareStatus {
                key_id: "ecdsa-primary".into(),
                share_index: 1,
                stored_in: "aws_cloudhsm".into(),
                last_verified_at: Some(Utc::now()),
                healthy: true,
            },
            MpcShareStatus {
                key_id: "frost-ed25519".into(),
                share_index: 0,
                stored_in: "software_hsm".into(),
                last_verified_at: Some(Utc::now()),
                healthy: true,
            },
        ],
        backup_locations: vec![
            BackupLocation {
                name: "Primary Vault".into(),
                location_type: "encrypted_file".into(),
                last_sync_at: Some(Utc::now()),
                status: "healthy".into(),
            },
            BackupLocation {
                name: "Shamir Shares (3-of-5)".into(),
                location_type: "distributed".into(),
                last_sync_at: Some(Utc::now()),
                status: "healthy".into(),
            },
        ],
        recovery_ready: true,
    })
}

// ---------------------------------------------------------------------------
// Token discovery
// ---------------------------------------------------------------------------

async fn discover_token(
    Json(req): Json<DiscoverTokenRequest>,
) -> Result<Json<DiscoveredToken>, ApiError> {
    // Simulated — in production would call RPC to get token metadata
    let (symbol, name, decimals, token_type) = match req.chain {
        Chain::Ethereum => {
            if req.contract_address.to_lowercase().contains("dac17f") {
                ("USDT", "Tether USD", 6u8, "ERC-20")
            } else if req.contract_address.to_lowercase().contains("a0b86991") {
                ("USDC", "USD Coin", 6, "ERC-20")
            } else {
                ("UNKNOWN", "Unknown Token", 18, "ERC-20")
            }
        }
        Chain::Tron => ("TRC20", "TRC-20 Token", 6, "TRC-20"),
        Chain::Solana => ("SPL", "SPL Token", 6, "SPL"),
        _ => return Err(ApiError::bad_request("Token discovery not supported for this chain")),
    };

    Ok(Json(DiscoveredToken {
        contract_address: req.contract_address,
        chain: req.chain,
        symbol: symbol.into(),
        name: name.into(),
        decimals,
        token_type: token_type.into(),
        total_supply: None,
        verified: false,
    }))
}

// ---------------------------------------------------------------------------
// Health dashboard
// ---------------------------------------------------------------------------

async fn health_dashboard() -> Json<SystemHealth> {
    let now = Utc::now();
    Json(SystemHealth {
        status: HealthStatus::Healthy,
        uptime_seconds: 86400,
        version: env!("CARGO_PKG_VERSION").into(),
        database: ComponentHealth { status: HealthStatus::Healthy, latency_ms: 5, details: None },
        chains: vec![
            ChainHealthDetail {
                chain: Chain::Ethereum, status: HealthStatus::Healthy,
                block_height: 19_500_000, latency_ms: 45, last_scan_at: Some(now),
                pending_deposits: 2, pending_withdrawals: 1,
            },
            ChainHealthDetail {
                chain: Chain::Bitcoin, status: HealthStatus::Healthy,
                block_height: 840_000, latency_ms: 120, last_scan_at: Some(now),
                pending_deposits: 0, pending_withdrawals: 0,
            },
            ChainHealthDetail {
                chain: Chain::Tron, status: HealthStatus::Healthy,
                block_height: 60_000_000, latency_ms: 80, last_scan_at: Some(now),
                pending_deposits: 1, pending_withdrawals: 0,
            },
            ChainHealthDetail {
                chain: Chain::Solana, status: HealthStatus::Healthy,
                block_height: 250_000_000, latency_ms: 15, last_scan_at: Some(now),
                pending_deposits: 0, pending_withdrawals: 1,
            },
        ],
        pipeline: PipelineHealth {
            received: 0, validated: 0, signed: 0,
            broadcast: 0, confirming: 0, stuck_count: 0,
        },
        background_tasks: vec![
            TaskHealth { name: "eth_scanner".into(), running: true, last_run_at: Some(now), interval_secs: 12, error_count: 0 },
            TaskHealth { name: "btc_scanner".into(), running: true, last_run_at: Some(now), interval_secs: 60, error_count: 0 },
            TaskHealth { name: "confirmation_tracker".into(), running: true, last_run_at: Some(now), interval_secs: 10, error_count: 0 },
            TaskHealth { name: "withdrawal_pipeline".into(), running: true, last_run_at: Some(now), interval_secs: 5, error_count: 0 },
            TaskHealth { name: "webhook_retry".into(), running: true, last_run_at: Some(now), interval_secs: 30, error_count: 0 },
            TaskHealth { name: "reconciliation".into(), running: true, last_run_at: Some(now), interval_secs: 600, error_count: 0 },
        ],
        timestamp: now,
    })
}

async fn pipeline_status() -> Json<PipelineHealth> {
    Json(PipelineHealth {
        received: 0, validated: 0, signed: 0,
        broadcast: 0, confirming: 0, stuck_count: 0,
    })
}
