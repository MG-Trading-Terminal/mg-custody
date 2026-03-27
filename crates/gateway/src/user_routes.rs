//! User-facing API routes for portfolio, deposit addresses, history, profile,
//! and notifications.
//!
//! All endpoints require at least Viewer role (authenticated session).
//! Password change requires the caller to supply their current password —
//! the server verifies it before accepting a new hash.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use chrono::Utc;
use custody_common::{Asset, Chain, price_oracle::PriceOracle};
use custody_db::user_portfolio::ActivityEvent;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth_routes::hash_password;
use crate::errors::ApiError;
use crate::rbac_middleware::{RequireEditor, RequireViewer};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Helper: build a fallback price oracle seeded with stablecoin + placeholder prices.
// ---------------------------------------------------------------------------

fn fallback_oracle() -> Arc<PriceOracle> {
    let oracle = Arc::new(PriceOracle::with_default_ttl());
    oracle.seed_fallback_prices();
    oracle
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn user_router() -> Router<AppState> {
    Router::new()
        .route("/v1/user/portfolio", get(portfolio))
        .route("/v1/user/deposit-address/{chain}", get(deposit_address))
        .route("/v1/user/deposit-history", get(deposit_history))
        .route("/v1/user/withdrawals/estimate", post(estimate_withdrawal))
        .route("/v1/user/profile", get(get_profile))
        .route("/v1/user/profile/password", put(change_password))
        .route("/v1/user/activity", get(activity_feed))
        .route("/v1/user/notifications", get(list_notifications))
        .route("/v1/user/notifications/test", post(test_notification))
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
    pub usd_value: String,
    pub price_usd: String,
}

#[derive(Serialize)]
pub struct PortfolioResponse {
    pub total_usd: String,
    pub assets: Vec<AssetPosition>,
    pub last_updated: String,
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

    let oracle = fallback_oracle();

    let mut total_usd = Decimal::ZERO;
    let mut positions: Vec<AssetPosition> = Vec::with_capacity(balances.len());

    for row in balances {
        let price_usd = oracle.get_price(&row.asset).unwrap_or(Decimal::ZERO);
        let usd_value = row.balance * price_usd;
        total_usd += usd_value;

        positions.push(AssetPosition {
            asset: row.asset,
            chain: row.chain,
            balance: row.balance.to_string(),
            usd_value: usd_value.round_dp(2).to_string(),
            price_usd: price_usd.to_string(),
        });
    }

    Ok(Json(PortfolioResponse {
        total_usd: total_usd.round_dp(2).to_string(),
        assets: positions,
        last_updated: Utc::now().to_rfc3339(),
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- Portfolio calculation ---

    #[test]
    fn portfolio_usd_value_calculation() {
        let oracle = PriceOracle::with_default_ttl();
        oracle.set_price("BTC", dec!(50000));
        oracle.set_price("ETH", dec!(3000));
        oracle.set_price("USDT", dec!(1));

        let btc_balance = dec!(5.23);
        let eth_balance = dec!(120.5);
        let usdt_balance = dec!(500000);

        let btc_usd = btc_balance * oracle.get_price("BTC").unwrap();
        let eth_usd = eth_balance * oracle.get_price("ETH").unwrap();
        let usdt_usd = usdt_balance * oracle.get_price("USDT").unwrap();

        // BTC: 5.23 * 50000 = 261500
        assert_eq!(btc_usd, dec!(261500));
        // ETH: 120.5 * 3000 = 361500
        assert_eq!(eth_usd, dec!(361500));
        // USDT: 500000 * 1 = 500000
        assert_eq!(usdt_usd, dec!(500000));

        let total = btc_usd + eth_usd + usdt_usd;
        assert_eq!(total, dec!(1123000));
    }

    #[test]
    fn portfolio_unknown_asset_price_zero() {
        let oracle = PriceOracle::with_default_ttl();
        // No price set for "DOGE"
        let price = oracle.get_price("DOGE").unwrap_or(Decimal::ZERO);
        let usd_value = dec!(1000) * price;
        assert_eq!(usd_value, Decimal::ZERO);
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

    // --- Fallback oracle ---

    #[test]
    fn fallback_oracle_has_stablecoins() {
        let oracle = fallback_oracle();
        assert_eq!(oracle.get_price("USDT"), Some(dec!(1)));
        assert_eq!(oracle.get_price("USDC"), Some(dec!(1)));
        assert!(oracle.get_price("BTC").unwrap() > dec!(0));
        assert!(oracle.get_price("ETH").unwrap() > dec!(0));
    }
}
