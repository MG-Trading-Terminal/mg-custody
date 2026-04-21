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
use crate::rbac_middleware::{RequireAdmin, RequireEditor, RequireViewer};
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
    _auth: RequireViewer,
    State(state): State<AppState>,
    Query(q): Query<ScheduledQuery>,
) -> Result<Json<Vec<ScheduledTransaction>>, ApiError> {
    let txs = state.db.list_scheduled_tx(q.vault_id, q.status.as_deref()).await
        .map_err(|e| ApiError::internal(&e.to_string()))?;
    Ok(Json(txs))
}

async fn create_scheduled(
    _auth: RequireEditor,
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
    _auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduledTransaction>, ApiError> {
    let tx = state.db.find_scheduled_tx(id).await
        .map_err(|e| ApiError::internal(&e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Scheduled transaction not found"))?;
    Ok(Json(tx))
}

async fn cancel_scheduled(
    _auth: RequireEditor,
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
    _auth: RequireViewer,
    State(state): State<AppState>,
    Json(req): Json<ScreenAddressRequest>,
) -> Result<Json<ScreeningResult>, ApiError> {
    // Call through to the same ScreeningService the withdrawal
    // pipeline uses. Previous behaviour returned a hardcoded
    // "low risk" for every address — operators looking at the
    // dashboard's "screen this address" button got a green
    // checkmark for OFAC-sanctioned wallets. Classic compliance
    // bypass wrapped in a helpful UI affordance.
    //
    // When the service isn't wired (no Chainalysis/Elliptic keys
    // configured), 503 honestly tells the caller "this service
    // isn't available here" instead of lying with a fake clean
    // result. The withdrawal pipeline still runs screening on
    // every withdrawal regardless of this endpoint's availability.
    let Some(svc) = state.screening_service.as_ref() else {
        return Err(ApiError {
            error: "screening_unavailable".into(),
            message: "AML screening provider not configured. \
                      Set CUSTODY_CHAINALYSIS_API_KEY or CUSTODY_ELLIPTIC_API_KEY, \
                      or use the withdrawal submission path which runs screening \
                      regardless of this endpoint.".into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::SERVICE_UNAVAILABLE,
        });
    };

    // ScreeningService::screen_address never returns Err — it
    // maps provider failures to its configured ScreeningFallback
    // (Block → High risk, AllowAndFlag → Low with a categories
    // tag). The handler gets a concrete ScreeningResult and just
    // adapts the shape to the public API.
    let outcome = svc.screen_address(&req.address, req.chain).await;
    let sanctions_match = outcome
        .categories
        .iter()
        .any(|c| c.eq_ignore_ascii_case("sanctions"));
    Ok(Json(ScreeningResult {
        id: Uuid::new_v4(),
        address: req.address,
        chain: req.chain,
        provider: outcome.provider,
        risk_level: adapt_risk_level(outcome.risk_level),
        risk_score: Some(outcome.risk_score as f32),
        flags: outcome.categories,
        sanctions_match,
        screened_at: outcome.screened_at,
        details: outcome.details,
    }))
}

/// Map the compliance crate's RiskLevel enum onto the public API's
/// identically-shaped enum in custody_common. They're separate
/// types today for layering reasons (compliance is business logic,
/// common has the wire contract) but the variants line up 1:1.
fn adapt_risk_level(l: custody_compliance::screening::RiskLevel) -> RiskLevel {
    use custody_compliance::screening::RiskLevel as C;
    match l {
        C::Low => RiskLevel::Low,
        C::Medium => RiskLevel::Medium,
        C::High => RiskLevel::High,
        C::Severe => RiskLevel::Severe,
    }
}

async fn screening_history(
    _auth: RequireViewer,
    Path(address): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Real screening history requires a `screening_results` table
    // we haven't shipped. Previous behaviour returned an empty mock
    // array with `overall_risk: "low"` — which is worse than
    // nothing, because a compliance reviewer looking for whether
    // an address was screened would see "yes, and it came back
    // clean" when in reality NO screening of record ever occurred.
    //
    // Until the history table lands, 501 with a pointer to the
    // live screening endpoint is the honest answer.
    Err(ApiError {
        error: "not_implemented".into(),
        message: format!(
            "Screening history for {address} is not persisted yet. \
             Call POST /v1/screening/address to run a live check; \
             the pipeline's screening calls are audit-logged at the \
             withdrawal level but not indexed by address."
        ),
        request_id: Uuid::new_v4().to_string(),
        status: StatusCode::NOT_IMPLEMENTED,
    })
}

// ---------------------------------------------------------------------------
// Backup & recovery status
// ---------------------------------------------------------------------------

async fn backup_status(_auth: RequireAdmin) -> Json<BackupStatus> {
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
    _auth: RequireAdmin,
    Json(req): Json<DiscoverTokenRequest>,
) -> Result<Json<DiscoveredToken>, ApiError> {
    // Real ERC-20 metadata query. The previous implementation
    // returned hardcoded USDT/USDC by substring-matching the
    // contract address and 18-decimal "Unknown Token" for
    // anything else — meaning a scam contract with a name/symbol
    // matching a real token but wrong decimals would have
    // `decimals=18` reported, leading to amount miscalculation
    // that would silently multiply or divide the user's withdrawal
    // by 10^12. Fund-loss adjacent.
    match req.chain {
        Chain::Ethereum => {
            let rpc_url = std::env::var("CUSTODY_CHAINS__ETHEREUM__RPC_URL")
                .ok()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError {
                    error: "rpc_unavailable".into(),
                    message: "Token discovery requires CUSTODY_CHAINS__ETHEREUM__RPC_URL. \
                              Refusing to guess metadata from substring matches — \
                              wrong decimals would mis-scale every withdrawal against \
                              this contract."
                        .into(),
                    request_id: Uuid::new_v4().to_string(),
                    status: StatusCode::SERVICE_UNAVAILABLE,
                })?;
            let md = erc20_metadata(&rpc_url, &req.contract_address)
                .await
                .map_err(|e| ApiError::internal(&format!("token metadata: {e}")))?;
            Ok(Json(DiscoveredToken {
                contract_address: req.contract_address,
                chain: req.chain,
                symbol: md.symbol,
                name: md.name,
                decimals: md.decimals,
                token_type: "ERC-20".into(),
                total_supply: md.total_supply,
                verified: false,
            }))
        }
        Chain::Tron | Chain::Solana => {
            // TRC-20 / SPL metadata would need chain-specific
            // queries (/wallet/triggerconstantcontract + rent_exempt
            // math for SPL). Until wired, honest 501 beats guessing.
            Err(ApiError {
                error: "not_implemented".into(),
                message: format!(
                    "Token discovery not yet wired for {:?}. The chain-specific metadata \
                     query (TRC-20 triggerconstantcontract / SPL Mint account parse) \
                     is a follow-up.",
                    req.chain
                ),
                request_id: Uuid::new_v4().to_string(),
                status: StatusCode::NOT_IMPLEMENTED,
            })
        }
        _ => Err(ApiError::bad_request(
            "Token discovery is only defined for ERC-20 / TRC-20 / SPL chains",
        )),
    }
}

/// Minimal ERC-20 metadata struct returned by `erc20_metadata`.
struct Erc20Metadata {
    name: String,
    symbol: String,
    decimals: u8,
    total_supply: Option<String>,
}

/// Three standard ERC-20 calls over JSON-RPC `eth_call`:
///   name()         0x06fdde03
///   symbol()       0x95d89b41
///   decimals()     0x313ce567
///   totalSupply()  0x18160ddd
///
/// `name` and `symbol` decode as dynamic strings; `decimals` is
/// uint8; `totalSupply` is uint256 (returned as a decimal string
/// without scaling, since we don't know how the caller will use it).
async fn erc20_metadata(rpc_url: &str, contract: &str) -> anyhow::Result<Erc20Metadata> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let call = |data: &str| -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{"to": contract, "data": data}, "latest"],
        })
    };
    async fn fetch_hex(
        client: &reqwest::Client,
        url: &str,
        body: serde_json::Value,
    ) -> anyhow::Result<String> {
        let r: serde_json::Value = client.post(url).json(&body).send().await?.json().await?;
        if let Some(err) = r.get("error").filter(|e| !e.is_null()) {
            anyhow::bail!("eth_call RPC error: {err}");
        }
        Ok(r["result"].as_str().unwrap_or("0x").to_string())
    }
    let name_hex = fetch_hex(&client, rpc_url, call("0x06fdde03")).await?;
    let symbol_hex = fetch_hex(&client, rpc_url, call("0x95d89b41")).await?;
    let decimals_hex = fetch_hex(&client, rpc_url, call("0x313ce567")).await?;
    let total_supply_hex = fetch_hex(&client, rpc_url, call("0x18160ddd")).await?;

    // uint8 decimals: last byte of the 32-byte word.
    let decimals_raw = decimals_hex.trim_start_matches("0x");
    let decimals = if decimals_raw.len() >= 2 {
        u8::from_str_radix(&decimals_raw[decimals_raw.len() - 2..], 16).unwrap_or(0)
    } else {
        0
    };

    // totalSupply is a u256 — return as base-10 string so callers
    // can format/scale however they need. We trim leading zeros
    // and handle empty → None.
    let total_supply = {
        let raw = total_supply_hex.trim_start_matches("0x");
        if raw.is_empty() || raw.chars().all(|c| c == '0') {
            None
        } else {
            // Convert hex → decimal string. u128 is enough for any
            // realistic token supply (2^128 ≈ 3.4e38; max realistic
            // total is ~1e18 even for tokens with 6 decimals).
            u128::from_str_radix(raw, 16).ok().map(|v| v.to_string())
        }
    };

    Ok(Erc20Metadata {
        name: decode_abi_string(&name_hex),
        symbol: decode_abi_string(&symbol_hex),
        decimals,
        total_supply,
    })
}

/// Decode an ABI-encoded dynamic string returned by eth_call.
/// Layout: 0x + 32-byte offset (always 0x20) + 32-byte length +
/// UTF-8 bytes padded to 32. We don't validate the offset because
/// every ERC-20 I've looked at returns 0x20; a non-standard layout
/// returns garbage which is better than panicking.
fn decode_abi_string(hex_str: &str) -> String {
    let hex = hex_str.trim_start_matches("0x");
    if hex.len() < 128 {
        return String::new();
    }
    // Length at bytes 32..64 (hex chars 64..128).
    let len = u64::from_str_radix(&hex[64..128], 16).unwrap_or(0) as usize;
    if len == 0 {
        return String::new();
    }
    let start = 128;
    let end = start + (len * 2);
    if end > hex.len() {
        return String::new();
    }
    let raw = match hex::decode(&hex[start..end]) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    String::from_utf8(raw).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Health dashboard
// ---------------------------------------------------------------------------

async fn health_dashboard(_auth: RequireViewer) -> Json<SystemHealth> {
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

async fn pipeline_status(_auth: RequireViewer) -> Json<PipelineHealth> {
    Json(PipelineHealth {
        received: 0, validated: 0, signed: 0,
        broadcast: 0, confirming: 0, stuck_count: 0,
    })
}
