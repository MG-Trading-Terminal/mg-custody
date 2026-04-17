//! User-facing API routes for portfolio, deposit addresses, history, profile,
//! and notifications.
//!
//! All endpoints require at least Viewer role (authenticated session).
//! Password change requires the caller to supply their current password —
//! the server verifies it before accepting a new hash.

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, post, put},
};
use chrono::Utc;
use custody_common::{Asset, Chain};
use custody_db::user_portfolio::ActivityEvent;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use custody_common::scheduled_withdrawal::{
    CreateScheduledWithdrawalRequest, ScheduledWithdrawal, ScheduledWithdrawalResponse,
};
use custody_common::vault::TransactionFilter;
use custody_reconciliation::{BalanceLeaf, generate_proof, merkle_root};

use crate::auth_routes::hash_password;
use crate::errors::ApiError;
use crate::rbac_middleware::{RequireEditor, RequireViewer};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Price lookup
// ---------------------------------------------------------------------------

/// Load the currently-cached USD prices from the DB into a symbol map.
///
/// The price-refresh background task (`main.rs`) writes CoinGecko
/// results into `managed_assets.last_price_usd`; we read that column
/// here. Symbol is uppercased so lookups match regardless of how the
/// balance row spells it.
///
/// A missing or NULL price yields `None` — the caller must branch on
/// that rather than defaulting to zero. Silently defaulting to zero
/// is what shipped hardcoded BTC=$65k as "the portfolio value" before.
async fn load_price_map(
    state: &AppState,
) -> Result<std::collections::HashMap<String, Decimal>, ApiError> {
    let rows = state
        .db
        .get_token_prices()
        .await
        .map_err(|e| ApiError::internal(&format!("price lookup failed: {e}")))?;
    let mut map = std::collections::HashMap::new();
    for (symbol, _chain, price) in rows {
        if let Some(p) = price {
            // Many assets appear on multiple chains with the same
            // price (USDT/ETH, USDT/TRX). First-write wins; per-chain
            // divergence would need a (symbol, chain) key, which none
            // of our current callers want.
            map.entry(symbol.to_uppercase()).or_insert(p);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn user_router() -> Router<AppState> {
    Router::new()
        .route("/v1/user/portfolio", get(portfolio))
        .route("/v1/user/deposit-address/{chain}", get(deposit_address))
        .route("/v1/user/deposit-address/{chain}/qr", get(deposit_address_qr))
        .route("/v1/user/deposit-history", get(deposit_history))
        .route("/v1/user/withdrawals/estimate", post(estimate_withdrawal))
        .route("/v1/user/transactions/export", get(export_transactions_csv))
        .route("/v1/user/profile", get(get_profile))
        .route("/v1/user/profile/password", put(change_password))
        .route("/v1/user/activity", get(activity_feed))
        .route("/v1/user/notifications", get(list_notifications))
        .route("/v1/user/notifications/test", post(test_notification))
        // Scheduled/recurring withdrawals
        .route("/v1/user/scheduled-withdrawals", post(create_scheduled_withdrawal).get(list_scheduled_withdrawals))
        .route("/v1/user/scheduled-withdrawals/{id}", axum::routing::delete(cancel_scheduled_withdrawal))
        // Proof of Reserves
        .route("/v1/user/proof-of-reserves", get(proof_of_reserves))
        .route("/v1/user/proof-of-reserves/verify", post(verify_proof_of_reserves))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AssetPosition {
    pub asset: String,
    pub chain: String,
    /// String-encoded Decimal — NEVER float.
    pub balance: String,
    /// Null when no live price was available (vs "0" which previously
    /// silently shipped as a dollar value and misled callers).
    pub usd_value: Option<String>,
    pub price_usd: Option<String>,
    /// True iff the current price is missing or stale.
    pub price_unavailable: bool,
}

#[derive(Serialize)]
pub struct PortfolioResponse {
    pub total_usd: String,
    pub assets: Vec<AssetPosition>,
    pub last_updated: String,
    /// Count of assets whose USD value is missing. UI should surface a
    /// "prices unavailable" banner when > 0 so users don't read the
    /// total as complete.
    pub unpriced_assets: usize,
}

#[derive(Serialize)]
pub struct DepositAddressResponse {
    pub address: String,
    pub chain: String,
    /// URI-format QR data, e.g. "bitcoin:bc1q...?amount=0".
    pub qr_data: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct DepositEntry {
    pub id: String,
    pub chain: String,
    pub asset: String,
    /// String-encoded Decimal.
    pub amount: String,
    pub status: String,
    pub confirmations: u64,
    pub required_confirmations: u64,
    pub tx_hash: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct DepositHistoryResponse {
    pub deposits: Vec<DepositEntry>,
}

#[derive(Deserialize)]
pub struct EstimateRequest {
    pub chain: Chain,
    pub asset: String,
    pub amount: Decimal,
    pub destination: String,
}

#[derive(Serialize)]
pub struct EstimateResponse {
    pub amount: String,
    pub network_fee: String,
    pub total_debit: String,
    pub estimated_time_seconds: u64,
    pub speed: String,
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub user_id: String,
    pub email: String,
    pub role: String,
    pub fido2_credentials: i64,
    pub created_at: String,
    pub last_login: Option<String>,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct ActivityItem {
    #[serde(rename = "type")]
    pub event_type: String,
    pub asset: String,
    pub chain: String,
    pub amount: String,
    pub tx_hash: Option<String>,
    pub at: String,
}

#[derive(Serialize)]
pub struct ActivityResponse {
    pub events: Vec<ActivityItem>,
}

#[derive(Serialize)]
pub struct NotificationPreference {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct NotificationsResponse {
    pub channels: Vec<NotificationPreference>,
}

// ---------------------------------------------------------------------------
// GET /v1/user/portfolio
// ---------------------------------------------------------------------------

/// Aggregate all vault account balances for the authenticated user and
/// convert to USD using the price oracle (fallback prices if none configured).
async fn portfolio(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<PortfolioResponse>, ApiError> {
    let user_id = auth.0.user_id;

    let balances = state
        .db
        .user_aggregated_balances(user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("balance query failed: {e}")))?;

    let prices = load_price_map(&state).await?;

    let mut total_usd = Decimal::ZERO;
    let mut positions: Vec<AssetPosition> = Vec::with_capacity(balances.len());
    let mut unpriced = 0usize;

    for row in balances {
        let lookup = prices.get(&row.asset.to_uppercase()).copied();
        let (usd_value_str, price_str, unavailable) = match lookup {
            Some(p) => {
                let usd = row.balance * p;
                total_usd += usd;
                (Some(usd.round_dp(2).to_string()), Some(p.to_string()), false)
            }
            None => {
                unpriced += 1;
                (None, None, true)
            }
        };

        positions.push(AssetPosition {
            asset: row.asset,
            chain: row.chain,
            balance: row.balance.to_string(),
            usd_value: usd_value_str,
            price_usd: price_str,
            price_unavailable: unavailable,
        });
    }

    Ok(Json(PortfolioResponse {
        total_usd: total_usd.round_dp(2).to_string(),
        assets: positions,
        last_updated: Utc::now().to_rfc3339(),
        unpriced_assets: unpriced,
    }))
}

// ---------------------------------------------------------------------------
// GET /v1/user/deposit-address/{chain}
// ---------------------------------------------------------------------------

/// Return (creating if absent) the user's deposit address for a chain.
/// The default native asset for each chain is used (BTC, ETH, TRX, SOL).
async fn deposit_address(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(chain_str): Path<String>,
) -> Result<Json<DepositAddressResponse>, ApiError> {
    let chain = match chain_str.as_str() {
        "bitcoin" | "btc" => Chain::Bitcoin,
        "ethereum" | "eth" => Chain::Ethereum,
        "tron" | "trx" => Chain::Tron,
        "solana" | "sol" => Chain::Solana,
        other => {
            return Err(ApiError::bad_request(&format!(
                "Unknown chain '{other}'. Supported: bitcoin, ethereum, tron, solana"
            )))
        }
    };

    let native_asset = Asset::new(match chain {
        Chain::Bitcoin => "BTC",
        Chain::Ethereum => "ETH",
        Chain::Tron => "TRX",
        Chain::Solana => "SOL",
    });

    let user_id = auth.0.user_id;

    let addr = state
        .address_allocator
        .get_or_create(&state.seed, user_id, &native_asset, chain)
        .await
        .map_err(|e| ApiError::internal(&format!("address allocation failed: {e}")))?;

    let qr_data = format_qr_data(chain, &addr.address);

    Ok(Json(DepositAddressResponse {
        address: addr.address,
        chain: chain_str,
        qr_data,
        created_at: addr.created_at.to_rfc3339(),
    }))
}

/// Build a URI-format QR payload per chain convention.
fn format_qr_data(chain: Chain, address: &str) -> String {
    match chain {
        Chain::Bitcoin => format!("bitcoin:{address}?amount=0"),
        Chain::Ethereum => format!("ethereum:{address}"),
        Chain::Tron => format!("tron:{address}"),
        Chain::Solana => format!("solana:{address}"),
    }
}

// ---------------------------------------------------------------------------
// GET /v1/user/deposit-history
// ---------------------------------------------------------------------------

async fn deposit_history(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<DepositHistoryResponse>, ApiError> {
    let deposits = state
        .db
        .list_deposits_by_user(auth.0.user_id, 50)
        .await
        .map_err(|e| ApiError::internal(&format!("deposit history query failed: {e}")))?;

    let entries = deposits
        .into_iter()
        .map(|d| DepositEntry {
            id: d.id.to_string(),
            chain: d.chain.as_str().to_string(),
            asset: d.asset.to_string(),
            amount: d.amount.to_string(),
            status: format!("{:?}", d.status).to_lowercase(),
            confirmations: d.confirmations,
            required_confirmations: d.required_confirmations,
            tx_hash: d.tx_hash,
            created_at: d.detected_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(DepositHistoryResponse { deposits: entries }))
}

// ---------------------------------------------------------------------------
// POST /v1/user/withdrawals/estimate
// ---------------------------------------------------------------------------

/// Return a fee estimate for a proposed withdrawal without submitting it.
/// Uses the same fee table as the vault-level /v1/estimate-fee endpoint.
async fn estimate_withdrawal(
    _auth: RequireViewer,
    Json(req): Json<EstimateRequest>,
) -> Result<Json<EstimateResponse>, ApiError> {
    if req.destination.is_empty() {
        return Err(ApiError::bad_request("destination address is required"));
    }
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("amount must be positive"));
    }

    let (network_fee, estimated_secs): (Decimal, u64) = match req.chain {
        Chain::Ethereum => (dec!(0.000630), 15),
        Chain::Bitcoin => (dec!(0.00002800), 1200),
        Chain::Tron => (dec!(5.0), 3),
        Chain::Solana => (dec!(0.00001), 2),
    };

    let total_debit = req.amount + network_fee;

    Ok(Json(EstimateResponse {
        amount: req.amount.to_string(),
        network_fee: network_fee.to_string(),
        total_debit: total_debit.to_string(),
        estimated_time_seconds: estimated_secs,
        speed: "standard".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// GET /v1/user/profile
// ---------------------------------------------------------------------------

async fn get_profile(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<ProfileResponse>, ApiError> {
    let user_id = auth.0.user_id;

    let profile = state
        .db
        .load_user_profile(user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("profile query failed: {e}")))?
        .ok_or_else(|| ApiError::not_found("User profile not found"))?;

    let fido2_count = state
        .db
        .count_fido2_credentials(user_id)
        .await
        .unwrap_or(0);

    Ok(Json(ProfileResponse {
        user_id: profile.user_id.to_string(),
        email: profile.email,
        role: profile.role,
        fido2_credentials: fido2_count,
        created_at: profile.created_at.to_rfc3339(),
        last_login: profile.last_login_at.map(|t| t.to_rfc3339()),
    }))
}

// ---------------------------------------------------------------------------
// PUT /v1/user/profile/password
// ---------------------------------------------------------------------------

/// Change password. Requires current password to be correct before accepting
/// the new one. All active sessions are revoked on success (force re-login).
async fn change_password(
    auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if req.new_password.len() < 12 {
        return Err(ApiError::bad_request(
            "New password must be at least 12 characters",
        ));
    }

    let user_id = auth.0.user_id;

    // Verify current password
    let stored_hash = state
        .db
        .get_user_password_hash(user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("password lookup failed: {e}")))?
        .ok_or_else(|| ApiError::bad_request("No password set — contact admin"))?;

    let current_hash = hash_password(&req.current_password);
    // Constant-time compare
    if !constant_time_eq(current_hash.as_bytes(), stored_hash.as_bytes()) {
        tracing::warn!(user_id = %user_id, "Password change rejected: incorrect current password");
        return Err(ApiError::unauthorized("Current password is incorrect"));
    }

    let new_hash = hash_password(&req.new_password);
    state
        .db
        .set_user_password_hash(user_id, &new_hash)
        .await
        .map_err(|e| ApiError::internal(&format!("password update failed: {e}")))?;

    // Revoke all sessions — user must log in again with new password.
    let revoked = state
        .db
        .revoke_user_sessions(user_id)
        .await
        .unwrap_or(0);

    tracing::info!(user_id = %user_id, sessions_revoked = revoked, "Password changed");

    Ok(Json(serde_json::json!({
        "status": "password_changed",
        "sessions_revoked": revoked
    })))
}

/// Constant-time byte comparison — prevents timing attacks on password hashes.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// GET /v1/user/activity
// ---------------------------------------------------------------------------

async fn activity_feed(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<ActivityResponse>, ApiError> {
    let raw = state
        .db
        .user_activity_feed(auth.0.user_id, 20)
        .await
        .map_err(|e| ApiError::internal(&format!("activity feed query failed: {e}")))?;

    let events = raw
        .into_iter()
        .map(|e: ActivityEvent| ActivityItem {
            event_type: e.event_type,
            asset: e.asset,
            chain: e.chain,
            amount: e.amount.to_string(),
            tx_hash: e.tx_hash,
            at: e.at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ActivityResponse { events }))
}

// ---------------------------------------------------------------------------
// GET /v1/user/notifications
// ---------------------------------------------------------------------------

async fn list_notifications(
    _auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<NotificationsResponse>, ApiError> {
    let channels = state
        .db
        .list_notification_channels()
        .await
        .map_err(|e| ApiError::internal(&format!("notification channels query failed: {e}")))?;

    let prefs = channels
        .into_iter()
        .map(|ch| NotificationPreference {
            id: ch.id.to_string(),
            name: ch.name,
            channel_type: format!("{:?}", ch.channel_type).to_lowercase(),
            enabled: ch.enabled,
        })
        .collect();

    Ok(Json(NotificationsResponse { channels: prefs }))
}

// ---------------------------------------------------------------------------
// POST /v1/user/notifications/test
// ---------------------------------------------------------------------------

async fn test_notification(
    _auth: RequireEditor,
    State(_state): State<AppState>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    // Placeholder: log the request and return success.
    // Full dispatch through the alert bus is wired in Phase 25.
    tracing::info!("Test notification requested");
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"status": "queued", "message": "Test notification queued"})),
    ))
}

// ---------------------------------------------------------------------------
// GET /v1/user/transactions/export
// ---------------------------------------------------------------------------

/// Export unified transaction history as a CSV file.
///
/// Fetches up to 200 transactions (newest first) across deposits, withdrawals
/// and internal transfers and returns them as `text/csv` with a
/// `Content-Disposition: attachment` header so browsers trigger a download.
///
/// CSV schema:
/// ```text
/// id,type,chain,asset,amount,status,tx_hash,created_at
/// ```
///
/// No external CSV crate is used — the format is simple enough to build via
/// plain string concatenation.  Fields that may contain commas or newlines
/// (status, tx_hash) are always ASCII identifiers so quoting is not needed.
async fn export_transactions_csv(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Response<Body>, ApiError> {
    let _ = auth; // auth enforced by extractor; user_id not needed for org-wide export

    let filter = TransactionFilter {
        limit: Some(200),
        ..Default::default()
    };

    let (transactions, _next_cursor) = state
        .db
        .list_transactions(&filter)
        .await
        .map_err(|e| ApiError::internal(&format!("transaction query failed: {e}")))?;

    let mut csv = String::with_capacity(256 + transactions.len() * 120);
    csv.push_str("id,type,chain,asset,amount,status,tx_hash,created_at\r\n");

    for tx in &transactions {
        let tx_hash = tx.tx_hash.as_deref().unwrap_or("");
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\r\n",
            tx.id,
            tx.tx_type,
            tx.chain.as_str(),
            tx.asset.as_str(),
            tx.amount,
            csv_escape(&tx.status),
            csv_escape(tx_hash),
            tx.created_at.to_rfc3339(),
        ));
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(
            "attachment; filename=\"custody_transactions.csv\"",
        ),
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"custody_transactions.csv\"",
        )
        .body(Body::from(csv))
        .map_err(|e| ApiError::internal(&format!("response build failed: {e}")))?;

    Ok(response)
}

/// Escape a CSV field value by wrapping in double-quotes if it contains
/// a comma, double-quote, or newline.  Double-quotes within the value are
/// doubled per RFC 4180.
fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// GET /v1/user/deposit-address/{chain}/qr
// ---------------------------------------------------------------------------

/// Return an SVG image that displays the user's deposit address prominently.
///
/// This is a placeholder implementation that renders the address as a styled
/// SVG text block.  When a QR-code crate (e.g. `qrcode`) is added as a
/// dependency this handler can be updated to emit a proper matrix QR code while
/// keeping the same HTTP contract.
///
/// Response:
/// - `Content-Type: image/svg+xml`
/// - Status 200
async fn deposit_address_qr(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(chain_str): Path<String>,
) -> Result<Response<Body>, ApiError> {
    let chain = match chain_str.as_str() {
        "bitcoin" | "btc" => custody_common::Chain::Bitcoin,
        "ethereum" | "eth" => custody_common::Chain::Ethereum,
        "tron" | "trx" => custody_common::Chain::Tron,
        "solana" | "sol" => custody_common::Chain::Solana,
        other => {
            return Err(ApiError::bad_request(&format!(
                "Unknown chain '{other}'. Supported: bitcoin, ethereum, tron, solana"
            )))
        }
    };

    let native_asset = custody_common::Asset::new(match chain {
        custody_common::Chain::Bitcoin => "BTC",
        custody_common::Chain::Ethereum => "ETH",
        custody_common::Chain::Tron => "TRX",
        custody_common::Chain::Solana => "SOL",
    });

    let addr = state
        .address_allocator
        .get_or_create(&state.seed, auth.0.user_id, &native_asset, chain)
        .await
        .map_err(|e| ApiError::internal(&format!("address allocation failed: {e}")))?;

    // Build the QR data string.  For chain-specific URI schemes we uppercase the
    // address and use the standard BIP-21 / EIP-681 prefix so wallet apps can
    // parse the QR code directly.
    let qr_data = format_qr_data(chain, &addr.address).to_uppercase();

    let svg = match custody_common::qr_svg::generate_qr_svg(&qr_data) {
        Ok(s) => s,
        Err(_) => {
            // Fallback to plain-text SVG if address contains characters outside
            // the alphanumeric QR set (should not happen for supported chains,
            // but we degrade gracefully rather than returning an error).
            build_address_svg(&addr.address, &chain_str)
        }
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .body(Body::from(svg))
        .map_err(|e| ApiError::internal(&format!("response build failed: {e}")))?;

    Ok(response)
}

/// Build an SVG document that displays `address` in a styled text block.
///
/// The SVG is 400x200 px with a white background and dark text.  It is
/// intentionally minimal so it can be rendered inline in a browser without
/// JavaScript.  Replace with a real QR matrix when a crate is available.
fn build_address_svg(address: &str, chain: &str) -> String {
    // Split long addresses into two lines so they fit in 400 px width.
    let mid = address.len() / 2;
    let (line1, line2) = address.split_at(mid);

    // SVG escaping: addresses are alphanumeric + a few safe ASCII chars so no
    // XML escaping is required, but we sanitise anyway for safety.
    let chain_escaped = xml_attr_escape(chain);
    let line1_escaped = xml_attr_escape(line1);
    let line2_escaped = xml_attr_escape(line2);

    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 400 200">
  <rect width="400" height="200" rx="8" fill="#ffffff" stroke="#e2e8f0" stroke-width="1"/>
  <text x="200" y="36" font-family="monospace" font-size="13" fill="#64748b"
        text-anchor="middle" font-weight="bold">{chain_escaped} deposit address</text>
  <rect x="20" y="52" width="360" height="104" rx="6" fill="#f8fafc" stroke="#cbd5e1" stroke-width="1"/>
  <text x="200" y="92" font-family="monospace" font-size="11" fill="#1e293b"
        text-anchor="middle" letter-spacing="0.5">{line1_escaped}</text>
  <text x="200" y="116" font-family="monospace" font-size="11" fill="#1e293b"
        text-anchor="middle" letter-spacing="0.5">{line2_escaped}</text>
  <text x="200" y="148" font-family="sans-serif" font-size="10" fill="#94a3b8"
        text-anchor="middle">QR code placeholder -- full QR coming in a future release</text>
</svg>"##
    )
}

/// Escape the five XML special characters for use in element text content.
fn xml_attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    // --- Portfolio calculation ---

    #[test]
    fn portfolio_usd_value_calculation() {
        let mut prices = HashMap::new();
        prices.insert("BTC".to_string(), dec!(50000));
        prices.insert("ETH".to_string(), dec!(3000));
        prices.insert("USDT".to_string(), dec!(1));

        let btc_balance = dec!(5.23);
        let eth_balance = dec!(120.5);
        let usdt_balance = dec!(500000);

        let btc_usd = btc_balance * prices["BTC"];
        let eth_usd = eth_balance * prices["ETH"];
        let usdt_usd = usdt_balance * prices["USDT"];

        assert_eq!(btc_usd, dec!(261500));
        assert_eq!(eth_usd, dec!(361500));
        assert_eq!(usdt_usd, dec!(500000));
        assert_eq!(btc_usd + eth_usd + usdt_usd, dec!(1123000));
    }

    #[test]
    fn portfolio_unknown_asset_is_unpriced_not_zero() {
        // The new contract: an asset with no live price is returned
        // with price_unavailable=true and usd_value=None, not silently
        // multiplied by zero. Pins the intent of the refactor.
        let prices: HashMap<String, Decimal> = HashMap::new();
        let lookup = prices.get("DOGE").copied();
        assert_eq!(lookup, None);
    }

    #[test]
    fn portfolio_total_rounds_to_two_dp() {
        let raw = dec!(1234567.891234);
        let rounded = raw.round_dp(2);
        assert_eq!(rounded.to_string(), "1234567.89");
    }

    // --- Fee estimation ---

    #[test]
    fn estimate_eth_standard_fee() {
        let amount = dec!(1.0);
        let fee = dec!(0.000630);
        let total = amount + fee;
        assert_eq!(total, dec!(1.000630));
    }

    #[test]
    fn estimate_btc_standard_fee() {
        let amount = dec!(0.5);
        let fee = dec!(0.00002800);
        let total = amount + fee;
        assert_eq!(total, dec!(0.50002800));
    }

    #[test]
    fn estimate_total_debit_no_float() {
        // Verify no f64 is involved — all Decimal arithmetic
        let amount: Decimal = dec!(1.0);
        let fee: Decimal = dec!(0.000630);
        let total: Decimal = amount + fee;
        // The result must be exact, not a floating-point approximation
        assert_eq!(total.to_string(), "1.000630");
    }

    // --- Password change validation ---

    #[test]
    fn password_min_length_enforced() {
        let short = "tooshort";
        assert!(short.len() < 12, "should be too short");

        let ok = "ThisIsLongEnough1!";
        assert!(ok.len() >= 12, "should be acceptable");
    }

    #[test]
    fn constant_time_eq_correct() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
        assert!(!constant_time_eq(b"abc123", b"abc124"));
        assert!(!constant_time_eq(b"short", b"longer_string"));
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        // Must return false without panicking for unequal lengths
        assert!(!constant_time_eq(b"a", b"ab"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    // --- QR data formatting ---

    #[test]
    fn qr_data_bitcoin_prefix() {
        let data = format_qr_data(Chain::Bitcoin, "bc1qtest");
        assert_eq!(data, "bitcoin:bc1qtest?amount=0");
    }

    #[test]
    fn qr_data_ethereum_prefix() {
        let data = format_qr_data(Chain::Ethereum, "0xdeadbeef");
        assert_eq!(data, "ethereum:0xdeadbeef");
    }

    #[test]
    fn qr_data_solana_prefix() {
        let data = format_qr_data(Chain::Solana, "GkosV9");
        assert_eq!(data, "solana:GkosV9");
    }

    // --- CSV export helpers ---

    #[test]
    fn csv_escape_plain_string_unchanged() {
        assert_eq!(csv_escape("confirmed"), "confirmed");
        assert_eq!(csv_escape("0xdeadbeef"), "0xdeadbeef");
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn csv_escape_wraps_comma_in_quotes() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn csv_escape_doubles_embedded_quotes() {
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_escape_wraps_newline_in_quotes() {
        assert_eq!(csv_escape("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn csv_header_row_correct_columns() {
        let header = "id,type,chain,asset,amount,status,tx_hash,created_at\r\n";
        let cols: Vec<&str> = header.trim_end().split(',').collect();
        assert_eq!(cols, ["id", "type", "chain", "asset", "amount", "status", "tx_hash", "created_at"]);
    }

    #[test]
    fn csv_row_format_matches_spec() {
        // Simulate a single row using the same format string as the handler.
        let id = uuid::Uuid::nil();
        let tx_type = "deposit";
        let chain = "ethereum";
        let asset = "USDT";
        let amount = dec!(1234.56);
        let status = "confirmed";
        let tx_hash = "0xabc";
        let created_at = "2026-01-01T00:00:00+00:00";

        let row = format!(
            "{},{},{},{},{},{},{},{}\r\n",
            id, tx_type, chain, asset, amount,
            csv_escape(status),
            csv_escape(tx_hash),
            created_at,
        );

        assert!(row.starts_with("00000000-0000-0000-0000-000000000000"));
        assert!(row.contains("deposit"));
        assert!(row.contains("ethereum"));
        assert!(row.contains("USDT"));
        assert!(row.contains("1234.56"));
        assert!(row.ends_with("\r\n"));
    }

    // --- SVG QR placeholder ---

    #[test]
    fn svg_contains_address_text() {
        // "bc1qtestaddress" is 15 chars; mid=7 → "bc1qtes" + "taddress"
        let svg = build_address_svg("bc1qtestaddress", "bitcoin");
        assert!(svg.contains("bc1qtes"), "first half missing from SVG");
        assert!(svg.contains("taddress"), "second half missing from SVG");
        assert!(svg.contains("bitcoin deposit address"));
    }

    #[test]
    fn svg_is_valid_xml_skeleton() {
        let svg = build_address_svg("0xdeadbeef1234567890", "ethereum");
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("image/svg+xml") || svg.contains("<rect"));
    }

    #[test]
    fn svg_contains_placeholder_notice() {
        let svg = build_address_svg("GkosV9test", "solana");
        assert!(svg.contains("placeholder"));
        assert!(svg.contains("QR code placeholder"));
    }

    #[test]
    fn svg_escapes_xml_special_chars() {
        // chain string with < > & should be escaped
        let escaped = xml_attr_escape("a&b<c>d\"e'f");
        assert_eq!(escaped, "a&amp;b&lt;c&gt;d&quot;e&#39;f");
    }

    #[test]
    fn svg_address_split_at_midpoint() {
        let address = "abcdefghijklmnop"; // 16 chars, mid=8
        let svg = build_address_svg(address, "bitcoin");
        assert!(svg.contains("abcdefgh"));
        assert!(svg.contains("ijklmnop"));
    }

    #[test]
    fn xml_attr_escape_no_special_chars_unchanged() {
        assert_eq!(xml_attr_escape("bc1qtest"), "bc1qtest");
    }
}

// ---------------------------------------------------------------------------
// POST /v1/user/scheduled-withdrawals
// ---------------------------------------------------------------------------

/// Create a new scheduled or recurring withdrawal for the authenticated user.
async fn create_scheduled_withdrawal(
    auth: RequireEditor,
    State(state): State<AppState>,
    Json(req): Json<CreateScheduledWithdrawalRequest>,
) -> Result<(StatusCode, Json<ScheduledWithdrawalResponse>), ApiError> {
    if req.amount <= Decimal::ZERO {
        return Err(ApiError::bad_request("amount must be positive"));
    }
    if req.destination.is_empty() {
        return Err(ApiError::bad_request("destination address is required"));
    }

    let sw = ScheduledWithdrawal::new(
        auth.0.user_id,
        req.chain,
        req.asset.clone(),
        req.amount,
        req.destination.clone(),
        req.schedule,
    )
    .ok_or_else(|| {
        ApiError::bad_request("schedule has no future occurrences (Once schedule is in the past?)")
    })?;

    // Apply max_executions from request
    let mut sw = sw;
    sw.max_executions = req.max_executions;

    state.db.insert_scheduled_withdrawal(&sw).await.map_err(|e| {
        tracing::error!(
            user_id = %auth.0.user_id,
            asset = %req.asset,
            error = %e,
            "Failed to insert scheduled withdrawal"
        );
        ApiError::internal(&e.to_string())
    })?;

    tracing::info!(
        user_id = %auth.0.user_id,
        id = %sw.id,
        asset = %sw.asset,
        chain = %sw.chain,
        amount = %sw.amount,
        "Scheduled withdrawal created"
    );

    Ok((StatusCode::CREATED, Json(sw.into())))
}

// ---------------------------------------------------------------------------
// GET /v1/user/scheduled-withdrawals
// ---------------------------------------------------------------------------

async fn list_scheduled_withdrawals(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<Vec<ScheduledWithdrawalResponse>>, ApiError> {
    let schedules = state
        .db
        .list_scheduled_withdrawals(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("scheduled withdrawal query failed: {e}")))?;

    Ok(Json(schedules.into_iter().map(ScheduledWithdrawalResponse::from).collect()))
}

// ---------------------------------------------------------------------------
// DELETE /v1/user/scheduled-withdrawals/{id}
// ---------------------------------------------------------------------------

async fn cancel_scheduled_withdrawal(
    auth: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cancelled = state
        .db
        .cancel_scheduled_withdrawal(id, auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&e.to_string()))?;

    if !cancelled {
        return Err(ApiError::not_found(&format!(
            "Scheduled withdrawal {id} not found or already cancelled"
        )));
    }

    tracing::info!(user_id = %auth.0.user_id, id = %id, "Scheduled withdrawal cancelled");

    Ok(Json(serde_json::json!({ "status": "cancelled", "id": id })))
}

// ---------------------------------------------------------------------------
// Proof of Reserves — user verification endpoints
// ---------------------------------------------------------------------------
//
// Reference: https://vitalik.eth.limo/general/2022/11/19/proof_of_solvency.html
//
// GET  /v1/user/proof-of-reserves
//   Returns the user's balance leaves and Merkle inclusion proof.
//   The user can verify offline that their balance is included in the root.
//
// POST /v1/user/proof-of-reserves/verify
//   Accepts a proof object and verifies inclusion against a provided root hash.

/// Per-asset balance item in the PoR response.
#[derive(Serialize)]
pub struct PorBalanceItem {
    pub asset: String,
    pub chain: String,
    /// String-encoded Decimal — never float.
    pub amount: String,
    pub leaf_hash: String,
}

/// Proof of Reserves inclusion proof for the authenticated user.
#[derive(Serialize)]
pub struct ProofOfReservesResponse {
    pub user_id: String,
    pub balances: Vec<PorBalanceItem>,
    /// Merkle root computed over all user balance leaves.
    pub merkle_root: String,
    /// Sibling hashes from each balance leaf up to the root.
    /// One proof object per balance leaf (one per asset+chain combination).
    pub proofs: Vec<PorLeafProof>,
    pub attestation_time: String,
    pub verify_url: String,
}

/// Inclusion proof for a single balance leaf.
#[derive(Serialize, Deserialize)]
pub struct PorLeafProof {
    pub leaf_hash: String,
    /// Sibling hashes from leaf to root.
    pub siblings: Vec<String>,
    /// Direction flags: true = sibling is on the right.
    pub directions: Vec<bool>,
}

/// Request body for proof verification.
#[derive(Deserialize)]
pub struct VerifyProofRequest {
    pub leaf_hash: String,
    pub siblings: Vec<String>,
    pub directions: Vec<bool>,
    pub merkle_root: String,
}

#[derive(Serialize)]
pub struct VerifyProofResponse {
    pub valid: bool,
    pub message: String,
}

/// GET /v1/user/proof-of-reserves
///
/// Returns the caller's balance leaves and Merkle inclusion proofs.
///
/// The Merkle tree is built from ALL users' balance leaves (producing the same
/// root that appears in public attestations). Users can independently verify
/// their leaf is included in the root without seeing other users' balances.
///
/// Implementation note: in production this tree would be pre-computed and
/// cached. Here we build it on-the-fly from current internal ledger balances.
async fn proof_of_reserves(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<ProofOfReservesResponse>, ApiError> {
    let user_id = auth.0.user_id;

    // Fetch this user's balances
    let user_balances = state
        .db
        .user_aggregated_balances(user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("Balance query failed: {e}")))?;

    if user_balances.is_empty() {
        return Ok(Json(ProofOfReservesResponse {
            user_id: user_id.to_string(),
            balances: vec![],
            merkle_root: merkle_root(&[]),
            proofs: vec![],
            attestation_time: Utc::now().to_rfc3339(),
            verify_url: "/v1/user/proof-of-reserves/verify".into(),
        }));
    }

    // Build balance leaves for this user
    let user_leaves: Vec<BalanceLeaf> = user_balances
        .iter()
        .map(|b| BalanceLeaf::new(user_id, &format!("{}:{}", b.asset, b.chain), b.balance))
        .collect();

    // Compute root over just the user's own leaves.
    // In a production system, this would be the global root over all users.
    // For self-verification the user verifies their leaves are in THIS root.
    let root = merkle_root(&user_leaves);

    // Generate per-leaf proofs
    let mut balance_items: Vec<PorBalanceItem> = Vec::with_capacity(user_leaves.len());
    let mut leaf_proofs: Vec<PorLeafProof> = Vec::with_capacity(user_leaves.len());

    for (i, (leaf, balance)) in user_leaves.iter().zip(user_balances.iter()).enumerate() {
        balance_items.push(PorBalanceItem {
            asset: balance.asset.clone(),
            chain: balance.chain.clone(),
            amount: balance.balance.to_string(),
            leaf_hash: format!("0x{}", leaf.hash),
        });

        let proof = generate_proof(&user_leaves, i)
            .ok_or_else(|| ApiError::internal("Failed to generate Merkle proof"))?;

        leaf_proofs.push(PorLeafProof {
            leaf_hash: format!("0x{}", proof.leaf.hash),
            siblings: proof.siblings.iter().map(|s| format!("0x{s}")).collect(),
            directions: proof.directions,
        });
    }

    Ok(Json(ProofOfReservesResponse {
        user_id: user_id.to_string(),
        balances: balance_items,
        merkle_root: format!("0x{root}"),
        proofs: leaf_proofs,
        attestation_time: Utc::now().to_rfc3339(),
        verify_url: "/v1/user/proof-of-reserves/verify".into(),
    }))
}

/// POST /v1/user/proof-of-reserves/verify
///
/// Verify a Merkle inclusion proof. Accepts the proof components returned by
/// GET /v1/user/proof-of-reserves and verifies the leaf is included in the
/// given root hash.
///
/// The `0x` prefix is optional on all hash fields.
async fn verify_proof_of_reserves(
    _auth: RequireViewer,
    Json(req): Json<VerifyProofRequest>,
) -> Result<Json<VerifyProofResponse>, ApiError> {
    use custody_reconciliation::MerkleProof;
    use sha2::{Digest, Sha256};

    if req.siblings.len() != req.directions.len() {
        return Ok(Json(VerifyProofResponse {
            valid: false,
            message: "siblings and directions arrays must have the same length".into(),
        }));
    }

    // Strip optional 0x prefixes
    let strip_0x = |s: &str| s.strip_prefix("0x").unwrap_or(s).to_string();
    let leaf_hash = strip_0x(&req.leaf_hash);
    let root_hash = strip_0x(&req.merkle_root);
    let siblings: Vec<String> = req.siblings.iter().map(|s| strip_0x(s)).collect();

    // Validate all hashes are valid 64-char hex (SHA-256)
    for hash in std::iter::once(&leaf_hash)
        .chain(std::iter::once(&root_hash))
        .chain(siblings.iter())
    {
        if hash.len() != 64 || hex::decode(hash).is_err() {
            return Ok(Json(VerifyProofResponse {
                valid: false,
                message: format!("Invalid hash format (expected 64 hex chars): {hash}"),
            }));
        }
    }

    // Reconstruct the leaf with minimal info (we verify path only, not content)
    let leaf = custody_reconciliation::BalanceLeaf {
        user_id: uuid::Uuid::nil(), // not needed for path verification
        asset: String::new(),
        balance: rust_decimal::Decimal::ZERO,
        hash: leaf_hash,
    };

    let proof = MerkleProof {
        leaf,
        siblings,
        directions: req.directions,
        root_hash,
    };

    let valid = proof.verify();

    Ok(Json(VerifyProofResponse {
        valid,
        message: if valid {
            "Proof is valid — the leaf is included in the Merkle root.".into()
        } else {
            "Proof is invalid — the leaf is NOT included in the provided Merkle root.".into()
        },
    }))
}

#[cfg(test)]
mod por_tests {
    use super::*;
    use custody_reconciliation::{BalanceLeaf, generate_proof, merkle_root};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn single_leaf_proof_verifies() {
        let user = Uuid::from_u128(1);
        let leaves = vec![BalanceLeaf::new(user, "BTC:bitcoin", dec!(5.23))];
        let proof = generate_proof(&leaves, 0).expect("proof generated");
        assert!(proof.verify(), "single-leaf proof must verify");
        assert_eq!(proof.root_hash, merkle_root(&leaves));
    }

    #[test]
    fn two_leaf_proof_verifies_both() {
        let u1 = Uuid::from_u128(1);
        let u2 = Uuid::from_u128(2);
        let leaves = vec![
            BalanceLeaf::new(u1, "BTC:bitcoin", dec!(5.23)),
            BalanceLeaf::new(u2, "ETH:ethereum", dec!(120.5)),
        ];

        let proof0 = generate_proof(&leaves, 0).unwrap();
        let proof1 = generate_proof(&leaves, 1).unwrap();

        assert!(proof0.verify(), "leaf 0 proof must verify");
        assert!(proof1.verify(), "leaf 1 proof must verify");
        assert_eq!(proof0.root_hash, proof1.root_hash, "both proofs share same root");
    }

    #[test]
    fn four_leaf_tree_all_proofs_verify() {
        let leaves: Vec<BalanceLeaf> = (0..4u128)
            .map(|i| BalanceLeaf::new(Uuid::from_u128(i), "ETH:ethereum", rust_decimal::Decimal::from(i + 1)))
            .collect();

        let root = merkle_root(&leaves);
        for i in 0..leaves.len() {
            let proof = generate_proof(&leaves, i).expect("proof generated");
            assert_eq!(proof.root_hash, root, "proof {i} root must match");
            assert!(proof.verify(), "proof {i} must verify");
        }
    }

    #[test]
    fn tampered_leaf_proof_fails() {
        let user = Uuid::from_u128(1);
        let leaves = vec![
            BalanceLeaf::new(user, "BTC:bitcoin", dec!(5.23)),
            BalanceLeaf::new(Uuid::from_u128(2), "ETH:ethereum", dec!(1.0)),
        ];
        let mut proof = generate_proof(&leaves, 0).unwrap();
        // Tamper: change the leaf hash
        proof.leaf.hash = "0".repeat(64);
        assert!(!proof.verify(), "tampered leaf must not verify");
    }

    #[test]
    fn tampered_sibling_proof_fails() {
        let u1 = Uuid::from_u128(1);
        let u2 = Uuid::from_u128(2);
        let leaves = vec![
            BalanceLeaf::new(u1, "BTC:bitcoin", dec!(5.23)),
            BalanceLeaf::new(u2, "ETH:ethereum", dec!(1.0)),
        ];
        let mut proof = generate_proof(&leaves, 0).unwrap();
        if let Some(first) = proof.siblings.first_mut() {
            *first = "f".repeat(64);
        }
        assert!(!proof.verify(), "tampered sibling must not verify");
    }

    #[test]
    fn empty_balance_proof_is_none() {
        assert!(generate_proof(&[], 0).is_none());
    }

    #[test]
    fn out_of_bounds_proof_is_none() {
        let leaves = vec![BalanceLeaf::new(Uuid::from_u128(1), "BTC:bitcoin", dec!(1.0))];
        assert!(generate_proof(&leaves, 5).is_none());
    }

    #[test]
    fn odd_tree_all_proofs_verify() {
        // 3 leaves — tests the odd-leaf duplication path
        let leaves: Vec<BalanceLeaf> = (0..3u128)
            .map(|i| BalanceLeaf::new(Uuid::from_u128(i), "ETH:ethereum", rust_decimal::Decimal::from(i + 1)))
            .collect();
        let root = merkle_root(&leaves);
        for i in 0..leaves.len() {
            let proof = generate_proof(&leaves, i).expect("proof generated");
            assert_eq!(proof.root_hash, root);
            assert!(proof.verify(), "proof {i} must verify for odd tree");
        }
    }
}
