//! Staking API — ETH via Lido (stETH).
//!
//! # Wire protocol
//!
//! Lido `submit(address _referral) payable` on the stETH contract is
//! the canonical "stake ETH" path. Mainnet stETH address:
//! `0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84`. Testnet (Holesky):
//! `0x3F1c547b21f65e10480dE3ad8E19fAAC46C95034`.
//!
//! Staking flow:
//!   1. POST /v1/staking/stake validates (chain + protocol + amount).
//!   2. Inserts a `withdrawals` row with:
//!        chain = Ethereum
//!        destination_address = stETH contract
//!        amount = stake amount
//!      The withdrawal pipeline picks it up and signs/broadcasts.
//!   3. Inserts a `staking_positions` row linked to that withdrawal,
//!      status = pending.
//!   4. The existing confirmation tracker updates the withdrawal to
//!      Finalized; a reconciler bumps the position to Confirmed
//!      (TODO: wire that in a follow-up — today `/positions` reflects
//!      the position row verbatim).
//!
//! # Unstaking (NOT YET wired)
//!
//! Lido unstake is multi-step: `approve` stETH to the WithdrawalQueue
//! contract, then `requestWithdrawals` to mint a withdrawal NFT, wait
//! several days, then `claimWithdrawal` with the NFT id. That's not a
//! single on-chain call — it needs its own state machine + NFT
//! tracking. The `POST /unstake` endpoint therefore returns 501 with
//! an honest "multi-step flow not wired" message rather than
//! pretending it works.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use custody_common::{Asset, Chain, WalletTier, Withdrawal, WithdrawalStatus};
use custody_db::staking_positions::{StakingPosition, StakingStatus};
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireSigner, RequireViewer};
use crate::state::AppState;

/// stETH (Lido) contract on Ethereum mainnet.
const LIDO_STETH_MAINNET: &str = "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84";
/// stETH on Holesky testnet (operator supplies `CUSTODY_TESTNET=1`).
const LIDO_STETH_HOLESKY: &str = "0x3F1c547b21f65e10480dE3ad8E19fAAC46C95034";

/// Function selector for `submit(address)` on the stETH contract.
///
/// keccak256("submit(address)")[0..4] = 0xa1903eab. Verified by
/// running the computation against etherscan's ABI decoder; see
/// `compute_lido_submit_selector` in the tests below for the
/// self-check that pins this constant.
const LIDO_SUBMIT_SELECTOR: [u8; 4] = [0xa1, 0x90, 0x3e, 0xab];

/// Build ERC-20-shape calldata for `submit(address _referral)`: the
/// 4-byte selector followed by the referral address left-padded to
/// 32 bytes. Zero address = no referral (Lido accepts it; no rebate).
fn lido_submit_calldata(referral: [u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&LIDO_SUBMIT_SELECTOR);
    // Left-pad address to 32 bytes.
    data.extend(std::iter::repeat_n(0u8, 12));
    data.extend_from_slice(&referral);
    data
}

/// Pick the stETH contract address based on the `CUSTODY_TESTNET`
/// env toggle the rest of the service already uses.
fn lido_address() -> &'static str {
    if std::env::var("CUSTODY_TESTNET").is_ok() {
        LIDO_STETH_HOLESKY
    } else {
        LIDO_STETH_MAINNET
    }
}

pub fn staking_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/staking/positions", get(list_positions))
        .route("/v1/staking/stake", post(stake))
        .route("/v1/staking/unstake", post(unstake))
        .route("/v1/staking/rewards", get(list_rewards))
        .route("/v1/staking/validators", get(list_validators))
        .with_state(state)
}

#[derive(Deserialize)]
struct StakeRequest {
    chain: String,
    /// Protocol identifier — currently "lido" is the only active one.
    /// Accepted synonyms: "lido", "steth", "ethereum".
    #[serde(default)]
    protocol: Option<String>,
    /// Referral address for Lido's referral program (20 bytes hex,
    /// 0x-prefixed). Defaults to zero address when omitted.
    #[serde(default)]
    referral_address: Option<String>,
    amount: String,
    asset: String,
}

async fn stake(
    auth: RequireSigner,
    State(state): State<AppState>,
    Json(req): Json<StakeRequest>,
) -> Result<(StatusCode, Json<StakingPosition>), ApiError> {
    let amount: Decimal = req.amount.parse()
        .map_err(|_| ApiError::bad_request("Invalid amount — must be string-encoded decimal"))?;

    match req.chain.as_str() {
        "ethereum" | "eth" => {}
        "solana" | "sol" => {
            return Err(ApiError {
                error: "not_implemented".into(),
                message: "Solana staking (Marinade / native delegate) is not yet wired. Ethereum via Lido works.".into(),
                request_id: Uuid::new_v4().to_string(),
                status: StatusCode::NOT_IMPLEMENTED,
            });
        }
        chain => {
            return Err(ApiError::bad_request(&format!(
                "Staking not supported for chain: {chain}"
            )));
        }
    }

    let protocol = req.protocol.as_deref().unwrap_or("lido").to_ascii_lowercase();
    if !matches!(protocol.as_str(), "lido" | "steth") {
        return Err(ApiError::bad_request(&format!(
            "Only protocol=\"lido\" is wired on Ethereum today (got \"{protocol}\"). \
             RocketPool / Ether.fi / EigenLayer will follow the same shape."
        )));
    }
    // Asset sanity-check — stake.rs expects ETH.
    if !matches!(req.asset.to_ascii_uppercase().as_str(), "ETH") {
        return Err(ApiError::bad_request("Lido staking requires asset=\"ETH\""));
    }
    custody_signing::staking::validate_eth_stake_amount(amount)
        .map_err(|e| ApiError::bad_request(&e.to_string()))?;

    let referral = parse_referral(req.referral_address.as_deref())?;
    let calldata = lido_submit_calldata(referral);
    let target = lido_address();

    // Insert the driving withdrawal. The pipeline will pick this up
    // on its next tick, sign via HSM/seed, and broadcast. Because
    // the stETH contract is a payable call (not a plain transfer),
    // we stash the calldata in the memo field — the pipeline's ETH
    // signer reads calldata from there when present. (If the memo
    // path isn't wired for arbitrary calldata in your build, this is
    // the one place to extend it.)
    let withdrawal_id = Uuid::new_v4();
    let w = Withdrawal {
        id: withdrawal_id,
        exchange_withdrawal_id: Uuid::new_v4(),
        user_id: auth.0.user_id,
        vault_id: None,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: target.to_string(),
        amount,
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None,
        block_height: None,
        confirmations: 0,
        error_message: None,
        created_at: Utc::now(),
        signed_at: None,
        broadcast_at: None,
        confirmed_at: None,
        delay_release_at: None,
        // Store the Lido submit calldata in the memo field for the
        // pipeline to pick up. Hex-encoded so it's an ordinary
        // printable string.
        memo: Some(format!("0x{}", hex::encode(&calldata))),
        destination_tag: None,
    };
    state
        .db
        .insert_withdrawal(&w)
        .await
        .map_err(|e| ApiError::internal(&format!("withdrawal insert: {e}")))?;

    let position = StakingPosition {
        id: Uuid::new_v4(),
        user_id: auth.0.user_id,
        chain: "ethereum".into(),
        protocol: "lido".into(),
        validator_address: target.to_string(),
        asset: "ETH".into(),
        amount,
        status: StakingStatus::Pending,
        withdrawal_id: Some(withdrawal_id),
        tx_hash: None,
        unstake_tx_hash: None,
        error_message: None,
        created_at: Utc::now(),
        confirmed_at: None,
        unstaked_at: None,
    };
    state
        .db
        .insert_staking_position(&position)
        .await
        .map_err(|e| ApiError::internal(&format!("position insert: {e}")))?;

    tracing::info!(
        position_id = %position.id,
        withdrawal_id = %withdrawal_id,
        user_id = %auth.0.user_id,
        amount = %amount,
        target = %target,
        referral = %hex::encode(referral),
        "Lido stake request queued — pipeline will sign + broadcast"
    );
    Ok((StatusCode::CREATED, Json(position)))
}

fn parse_referral(input: Option<&str>) -> Result<[u8; 20], ApiError> {
    let mut out = [0u8; 20];
    let Some(s) = input else {
        return Ok(out);
    };
    let hex_part = s.strip_prefix("0x").unwrap_or(s);
    if hex_part.is_empty() {
        return Ok(out);
    }
    let bytes = hex::decode(hex_part).map_err(|e| {
        ApiError::bad_request(&format!("referral_address must be hex: {e}"))
    })?;
    if bytes.len() != 20 {
        return Err(ApiError::bad_request(
            "referral_address must be exactly 20 bytes (EVM address)",
        ));
    }
    out.copy_from_slice(&bytes);
    Ok(out)
}

async fn unstake(
    _auth: RequireSigner,
    State(_state): State<AppState>,
    Json(_req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError {
        error: "not_implemented".into(),
        message:
            "Lido unstake is a multi-step flow (approve stETH → requestWithdrawals → wait 1-7 days → claimWithdrawal with NFT id). \
             Not wired yet. Track the WithdrawalQueue NFT manually on Etherscan in the interim."
                .into(),
        request_id: Uuid::new_v4().to_string(),
        status: StatusCode::NOT_IMPLEMENTED,
    })
}

async fn list_positions(
    auth: RequireViewer,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let positions = state
        .db
        .list_staking_positions_for_user(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("position list: {e}")))?;
    let count = positions.len();
    Ok(Json(serde_json::json!({
        "positions": positions,
        "count": count,
    })))
}

async fn list_rewards(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // stETH is rebasing — rewards show up as balance growth. Proper
    // reward accounting requires periodic balance snapshots and
    // delta-computation; until that landing, expose an honest 501.
    Err(ApiError {
        error: "not_implemented".into(),
        message:
            "Reward accounting requires periodic on-chain balance snapshots (stETH is rebasing). Not yet wired."
                .into(),
        request_id: Uuid::new_v4().to_string(),
        status: StatusCode::NOT_IMPLEMENTED,
    })
}

async fn list_validators(
    _auth: RequireViewer,
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    // Static protocol registry. APYs are deliberately omitted — they
    // vary daily, and publishing a stale number as a "live rate"
    // would be the same promotional-number dishonesty we just
    // excised. Clients should fetch live APY from the protocol's
    // own API when that matters.
    let mainnet = std::env::var("CUSTODY_TESTNET").is_err();
    Json(serde_json::json!({
        "network": if mainnet { "mainnet" } else { "testnet" },
        "protocols": [
            {
                "chain": "ethereum",
                "protocol": "lido",
                "asset": "ETH",
                "contract": lido_address(),
                "wired": true,
                "unstake_wired": false,
                "notes": "submit(address _referral) payable → stETH. Unstake requires multi-step WithdrawalQueue flow (not yet wired).",
            },
            {
                "chain": "ethereum",
                "protocol": "rocketpool",
                "wired": false,
                "notes": "Follow same calldata pattern via rETH contract (follow-up).",
            },
            {
                "chain": "solana",
                "protocol": "marinade",
                "wired": false,
                "notes": "Requires SPL stake program instruction builder (follow-up).",
            }
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_lido_submit_selector() {
        use sha3::{Digest, Keccak256};
        let sig = b"submit(address)";
        let hash = Keccak256::digest(sig);
        assert_eq!(&hash[..4], &LIDO_SUBMIT_SELECTOR);
    }

    #[test]
    fn lido_submit_calldata_is_36_bytes() {
        // 4-byte selector + 32-byte padded address = 36 bytes.
        let cd = lido_submit_calldata([0u8; 20]);
        assert_eq!(cd.len(), 36);
        assert_eq!(&cd[..4], &LIDO_SUBMIT_SELECTOR);
        // The 12 bytes after the selector are zero-padding, then the address.
        assert!(cd[4..16].iter().all(|&b| b == 0));
    }

    #[test]
    fn lido_submit_calldata_with_referral() {
        let mut referral = [0u8; 20];
        referral[19] = 0xAB;
        let cd = lido_submit_calldata(referral);
        assert_eq!(cd.len(), 36);
        assert_eq!(cd[35], 0xAB);
    }

    #[test]
    fn parse_referral_accepts_none() {
        assert_eq!(parse_referral(None).unwrap(), [0u8; 20]);
        assert_eq!(parse_referral(Some("")).unwrap(), [0u8; 20]);
    }

    #[test]
    fn parse_referral_accepts_0x_prefixed_address() {
        let r = parse_referral(Some("0x0000000000000000000000000000000000000001")).unwrap();
        assert_eq!(r[19], 0x01);
    }

    #[test]
    fn parse_referral_rejects_wrong_length() {
        assert!(parse_referral(Some("0xdead")).is_err());
    }
}
