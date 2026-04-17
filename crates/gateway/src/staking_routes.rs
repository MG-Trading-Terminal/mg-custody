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

use crate::errors::{require_feature, ApiError};
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

/// RocketPool `RocketDepositPool` proxy contract on mainnet.
/// Users POST ETH here with `deposit() payable`; rETH is minted at
/// the current exchange rate and returned to the caller.
const ROCKETPOOL_DEPOSIT_MAINNET: &str = "0xDD3f50F8A6CafbE9b31a427582963f465E745AF8";
/// RocketPool deposit on Holesky testnet.
const ROCKETPOOL_DEPOSIT_HOLESKY: &str = "0x320f3aAB9405e38b955178BBe75c477dECBA0C27";

/// Function selector for `deposit()` — four bytes, no args.
/// keccak256("deposit()")[0..4] = 0xd0e30db0.
const ROCKETPOOL_DEPOSIT_SELECTOR: [u8; 4] = [0xd0, 0xe3, 0x0d, 0xb0];

/// Lido `WithdrawalQueueERC721` contract on Ethereum mainnet.
/// Holds the unstake queue; minted ERC-721 NFTs represent pending
/// withdrawals until they can be claimed.
const LIDO_WITHDRAWAL_QUEUE_MAINNET: &str = "0x889edC2eDab5f40e902b864aD4d7AdE8E412F9B1";
/// Lido WithdrawalQueue on Holesky testnet.
const LIDO_WITHDRAWAL_QUEUE_HOLESKY: &str = "0xc7cc160b58F8Bb0baC94b80847E2CF2800565C50";

/// Function selector for ERC-20 `approve(address,uint256)`.
/// keccak256("approve(address,uint256)")[0..4] = 0x095ea7b3.
const ERC20_APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];

/// Function selector for Lido `requestWithdrawals(uint256[],address)`.
/// keccak256("requestWithdrawals(uint256[],address)")[0..4] = 0xd66810426 — verified in tests.
const LIDO_REQUEST_WITHDRAWALS_SELECTOR: [u8; 4] = [0xd6, 0x68, 0x10, 0x42];

/// Pick the Lido WithdrawalQueue contract for the current network.
fn lido_withdrawal_queue_address() -> &'static str {
    if std::env::var("CUSTODY_TESTNET").is_ok() {
        LIDO_WITHDRAWAL_QUEUE_HOLESKY
    } else {
        LIDO_WITHDRAWAL_QUEUE_MAINNET
    }
}

/// Build ERC-20 `approve(spender, amount)` calldata. Returns 68 bytes:
/// 4-byte selector + 32-byte spender + 32-byte amount (both big-endian
/// right-aligned).
fn erc20_approve_calldata(spender_hex: &str, amount_wei: alloy::primitives::U256) -> Result<Vec<u8>, ApiError> {
    let spender: alloy::primitives::Address = spender_hex
        .parse()
        .map_err(|e| ApiError::internal(&format!("spender parse: {e}")))?;
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&ERC20_APPROVE_SELECTOR);
    // Left-pad spender to 32 bytes.
    data.extend(std::iter::repeat_n(0u8, 12));
    data.extend_from_slice(spender.as_slice());
    data.extend_from_slice(&amount_wei.to_be_bytes::<32>());
    Ok(data)
}

/// Build Lido `requestWithdrawals(uint256[] _amounts, address _owner)`
/// calldata for a single-entry amount array — the common case.
///
/// ABI head/tail layout (4 + 32 + 32 + 32 + 32 = 132 bytes):
///   [0..4]   selector
///   head:
///     [4..36]   offset to _amounts tail = 0x40 (64)
///     [36..68]  _owner (padded)
///   tail:
///     [68..100]  length = 1
///     [100..132] amount
fn lido_request_withdrawals_calldata(
    amount_wei: alloy::primitives::U256,
    owner_hex: &str,
) -> Result<Vec<u8>, ApiError> {
    let owner: alloy::primitives::Address = owner_hex
        .parse()
        .map_err(|e| ApiError::internal(&format!("owner parse: {e}")))?;
    let mut data = Vec::with_capacity(132);
    data.extend_from_slice(&LIDO_REQUEST_WITHDRAWALS_SELECTOR);
    // Offset to dynamic tail: 0x40 (two head slots of 32 bytes).
    let mut offset = [0u8; 32];
    offset[31] = 0x40;
    data.extend_from_slice(&offset);
    // Owner.
    data.extend(std::iter::repeat_n(0u8, 12));
    data.extend_from_slice(owner.as_slice());
    // Length = 1.
    let mut len = [0u8; 32];
    len[31] = 1;
    data.extend_from_slice(&len);
    // Amount.
    data.extend_from_slice(&amount_wei.to_be_bytes::<32>());
    Ok(data)
}

/// Build calldata for Lido `submit(address _referral)`: 4-byte
/// selector + 20-byte referral left-padded to 32 bytes. Zero address
/// = no referral (accepted; no rebate).
fn lido_submit_calldata(referral: [u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&LIDO_SUBMIT_SELECTOR);
    data.extend(std::iter::repeat_n(0u8, 12));
    data.extend_from_slice(&referral);
    data
}

/// Build calldata for RocketPool `deposit()` — just the 4-byte
/// selector, no arguments. Referrals don't exist in this protocol.
fn rocketpool_deposit_calldata() -> Vec<u8> {
    ROCKETPOOL_DEPOSIT_SELECTOR.to_vec()
}

/// Pick the stETH contract based on `CUSTODY_TESTNET`.
fn lido_address() -> &'static str {
    if std::env::var("CUSTODY_TESTNET").is_ok() {
        LIDO_STETH_HOLESKY
    } else {
        LIDO_STETH_MAINNET
    }
}

/// Pick the RocketPool deposit contract based on `CUSTODY_TESTNET`.
fn rocketpool_address() -> &'static str {
    if std::env::var("CUSTODY_TESTNET").is_ok() {
        ROCKETPOOL_DEPOSIT_HOLESKY
    } else {
        ROCKETPOOL_DEPOSIT_MAINNET
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
    // Gate at the handler boundary. Accepting a stake when the ETH
    // scanner/broadcaster isn't reachable means the driving withdrawal
    // sits in the DB forever — the user wires up thinking it worked.
    require_feature(&state.readiness, "scanner.ethereum")?;

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
    let (target, calldata, protocol_label) = match protocol.as_str() {
        "lido" | "steth" => {
            let referral = parse_referral(req.referral_address.as_deref())?;
            (lido_address(), lido_submit_calldata(referral), "lido")
        }
        "rocketpool" | "reth" => {
            // RocketPool has no referral mechanism; if the caller
            // supplied one, reject loudly — silently dropping would
            // be the same "accepted but ignored" dishonesty we
            // excised elsewhere.
            if req.referral_address.as_deref().is_some_and(|s| !s.is_empty() && s != "0x") {
                return Err(ApiError::bad_request(
                    "RocketPool has no referral program — omit referral_address",
                ));
            }
            (rocketpool_address(), rocketpool_deposit_calldata(), "rocketpool")
        }
        other => {
            return Err(ApiError::bad_request(&format!(
                "Unsupported ETH protocol: \"{other}\". Supported: lido, rocketpool."
            )));
        }
    };

    if !matches!(req.asset.to_ascii_uppercase().as_str(), "ETH") {
        return Err(ApiError::bad_request("ETH staking requires asset=\"ETH\""));
    }
    custody_signing::staking::validate_eth_stake_amount(amount)
        .map_err(|e| ApiError::bad_request(&e.to_string()))?;

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
                depends_on_withdrawal_id: None,
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
        protocol: protocol_label.into(),
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
        protocol = protocol_label,
        "ETH stake request queued — pipeline will sign + broadcast"
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

#[derive(Deserialize)]
struct UnstakeRequest {
    /// Existing staking_positions.id to unstake from. We read the
    /// protocol + amount off the row so callers can't double-unstake
    /// or pass a bogus amount.
    position_id: Uuid,
}

#[derive(serde::Serialize)]
struct UnstakeResponse {
    position_id: Uuid,
    approve_withdrawal_id: Uuid,
    request_withdrawal_id: Uuid,
    note: &'static str,
}

async fn unstake(
    auth: RequireSigner,
    State(state): State<AppState>,
    Json(req): Json<UnstakeRequest>,
) -> Result<(StatusCode, Json<UnstakeResponse>), ApiError> {
    // Same gate as stake(): unstake queues two ETH transactions that
    // must be signed and broadcast. Without a live RPC they sit in the
    // DB forever.
    require_feature(&state.readiness, "scanner.ethereum")?;

    let positions = state
        .db
        .list_staking_positions_for_user(auth.0.user_id)
        .await
        .map_err(|e| ApiError::internal(&format!("position lookup: {e}")))?;
    let position = positions
        .into_iter()
        .find(|p| p.id == req.position_id)
        .ok_or_else(|| ApiError::not_found("Staking position not found or not yours"))?;

    if position.protocol != "lido" {
        return Err(ApiError {
            error: "not_implemented".into(),
            message: format!(
                "Unstake is wired only for protocol=\"lido\" today (this position is \"{}\"). \
                 RocketPool rETH exit via burn, Marinade mSOL unstake — follow-ups.",
                position.protocol
            ),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::NOT_IMPLEMENTED,
        });
    }
    if !matches!(
        position.status,
        StakingStatus::Confirmed,
    ) {
        return Err(ApiError::bad_request(&format!(
            "Position status is {:?} — only confirmed positions can start unstake",
            position.status
        )));
    }

    // Resolve the user's on-chain ETH address so we can build both
    // (a) the stETH allowance subject and (b) the requestWithdrawals
    // `_owner` argument. Without it we cannot produce a correctly
    // formed unstake chain, so refuse early.
    let owner_hex = state
        .db
        .find_native_address_for_chain(auth.0.user_id, Chain::Ethereum)
        .await
        .map_err(|e| ApiError::internal(&format!("address lookup: {e}")))?
        .ok_or_else(|| {
            ApiError::conflict(
                "User has no Ethereum address on file — create one via GET /v1/user/deposit-address/ethereum first",
            )
        })?;

    // Convert the stored stake amount (ETH) to wei. stETH is 1:1 with
    // ETH at the protocol level (rebasing changes balances, but
    // requestWithdrawals takes the shares/stETH amount and Lido
    // resolves the payout at claim time). For the allowance we use
    // the same number.
    let amount_wei = custody_scanner::decimal_to_uint256(position.amount, 18);
    let steth = lido_address();
    let withdrawal_queue = lido_withdrawal_queue_address();

    // Step 1 — approve stETH → WithdrawalQueue.
    let approve_calldata = erc20_approve_calldata(withdrawal_queue, amount_wei)?;
    let approve_id = Uuid::new_v4();
    let approve = Withdrawal {
        id: approve_id,
        exchange_withdrawal_id: Uuid::new_v4(),
        user_id: auth.0.user_id,
        vault_id: None,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: steth.to_string(),
        amount: Decimal::ZERO,
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
        memo: Some(format!("0x{}", hex::encode(&approve_calldata))),
        destination_tag: None,
        depends_on_withdrawal_id: None,
    };
    state.db.insert_withdrawal(&approve).await
        .map_err(|e| ApiError::internal(&format!("approve insert: {e}")))?;

    // Step 2 — requestWithdrawals, gated on step 1 finalizing.
    let request_calldata = lido_request_withdrawals_calldata(amount_wei, &owner_hex)?;
    let request_id = Uuid::new_v4();
    let request_tx = Withdrawal {
        id: request_id,
        exchange_withdrawal_id: Uuid::new_v4(),
        user_id: auth.0.user_id,
        vault_id: None,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: withdrawal_queue.to_string(),
        amount: Decimal::ZERO,
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
        memo: Some(format!("0x{}", hex::encode(&request_calldata))),
        destination_tag: None,
        depends_on_withdrawal_id: Some(approve_id),
    };
    state.db.insert_withdrawal(&request_tx).await
        .map_err(|e| ApiError::internal(&format!("request insert: {e}")))?;

    state
        .db
        .update_staking_position_status(position.id, StakingStatus::Unstaking, None)
        .await
        .map_err(|e| ApiError::internal(&format!("position update: {e}")))?;

    tracing::info!(
        position_id = %position.id,
        approve_id = %approve_id,
        request_id = %request_id,
        amount = %position.amount,
        owner = %owner_hex,
        "Lido unstake sequence queued (approve → requestWithdrawals)"
    );

    Ok((
        StatusCode::CREATED,
        Json(UnstakeResponse {
            position_id: position.id,
            approve_withdrawal_id: approve_id,
            request_withdrawal_id: request_id,
            note: "Approve + requestWithdrawals queued. After the request tx finalizes, \
                   Lido mints a withdrawal NFT to the owner address. Parse the receipt's \
                   `WithdrawalRequested` event for the request id, then claim via \
                   claimWithdrawal after the protocol finalization window (typically 1–7 days). \
                   Claim endpoint is a follow-up.",
        }),
    ))
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
                "asset": "ETH",
                "contract": rocketpool_address(),
                "wired": true,
                "unstake_wired": false,
                "notes": "deposit() payable → rETH at current exchange rate. Burn rETH to exit when RP queue has liquidity (not yet wired).",
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

    #[test]
    fn compute_rocketpool_deposit_selector() {
        use sha3::{Digest, Keccak256};
        let hash = Keccak256::digest(b"deposit()");
        assert_eq!(&hash[..4], &ROCKETPOOL_DEPOSIT_SELECTOR);
    }

    #[test]
    fn rocketpool_deposit_calldata_is_selector_only() {
        let cd = rocketpool_deposit_calldata();
        assert_eq!(cd, &ROCKETPOOL_DEPOSIT_SELECTOR);
        assert_eq!(cd.len(), 4);
    }

    #[test]
    fn addresses_differ_by_network() {
        // Sanity: never mix mainnet + testnet constants.
        assert_ne!(LIDO_STETH_MAINNET, LIDO_STETH_HOLESKY);
        assert_ne!(ROCKETPOOL_DEPOSIT_MAINNET, ROCKETPOOL_DEPOSIT_HOLESKY);
        assert_ne!(LIDO_STETH_MAINNET, ROCKETPOOL_DEPOSIT_MAINNET);
        assert_ne!(LIDO_WITHDRAWAL_QUEUE_MAINNET, LIDO_WITHDRAWAL_QUEUE_HOLESKY);
    }

    #[test]
    fn compute_erc20_approve_selector() {
        use sha3::{Digest, Keccak256};
        let hash = Keccak256::digest(b"approve(address,uint256)");
        assert_eq!(&hash[..4], &ERC20_APPROVE_SELECTOR);
    }

    #[test]
    fn compute_lido_request_withdrawals_selector() {
        use sha3::{Digest, Keccak256};
        let hash = Keccak256::digest(b"requestWithdrawals(uint256[],address)");
        assert_eq!(&hash[..4], &LIDO_REQUEST_WITHDRAWALS_SELECTOR);
    }

    #[test]
    fn erc20_approve_calldata_shape() {
        use alloy::primitives::U256;
        let cd = erc20_approve_calldata(
            "0x889edC2eDab5f40e902b864aD4d7AdE8E412F9B1",
            U256::from(10_000_000_000_000_000_000u128), // 10 ETH
        )
        .unwrap();
        // 4 + 32 + 32 = 68 bytes
        assert_eq!(cd.len(), 68);
        assert_eq!(&cd[..4], &ERC20_APPROVE_SELECTOR);
        // Address is in bytes 16..36 (left-pad 12 zeros + 20 bytes).
        assert_eq!(&cd[16..36], &[
            0x88, 0x9e, 0xdC, 0x2e, 0xDa, 0xb5, 0xf4, 0x0e, 0x90, 0x2b,
            0x86, 0x4a, 0xD4, 0xd7, 0xAd, 0xE8, 0xE4, 0x12, 0xF9, 0xB1,
        ]);
    }

    #[test]
    fn lido_request_withdrawals_calldata_shape() {
        use alloy::primitives::U256;
        let cd = lido_request_withdrawals_calldata(
            U256::from(1_000_000_000_000_000_000u128), // 1 ETH
            "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
        )
        .unwrap();
        // 4 + 32 + 32 + 32 + 32 = 132 bytes
        assert_eq!(cd.len(), 132);
        assert_eq!(&cd[..4], &LIDO_REQUEST_WITHDRAWALS_SELECTOR);
        // Offset to tail = 64 = 0x40 (big-endian in last byte of slot).
        assert_eq!(cd[35], 0x40);
        // Length slot = 1.
        assert_eq!(cd[99], 1);
    }
}
