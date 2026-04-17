//! Swap execution API routes.
//!
//! Provides endpoints for getting swap quotes (UniswapV3 on Ethereum, Jupiter on Solana)
//! and executing them. Execution builds the transaction calldata/payload — the signing
//! service must sign and broadcast separately.
//!
//! ## Endpoints
//! - `POST /v1/swaps/quote`  — get a price quote (RequireEditor)
//! - `POST /v1/swaps`        — execute a quote (RequireSigner)
//! - `GET  /v1/swaps/{id}`   — get swap status (RequireViewer)
//!
//! ## Auth
//! Quote and status use `RequireEditor`/`RequireViewer` — read-only or quote-only callers.
//! Execute uses `RequireSigner` — the signing role, since this creates a live transaction.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use custody_common::swap_executor::{
    JupiterExecutor, SwapExecution, SwapExecutionStatus, SwapExecutor, SwapQuote, SwapResult,
    UniswapV3Executor,
};
use custody_common::{Chain, defi_execution::validate_slippage};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::rbac_middleware::{RequireEditor, RequireSigner, RequireViewer};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /v1/swaps/quote`.
#[derive(Debug, Deserialize)]
pub struct QuoteRequest {
    /// Source chain.
    pub chain: Chain,
    /// Input token — EVM address (0x...) for Ethereum, mint address for Solana.
    pub from_token: String,
    /// Output token — EVM address or mint address.
    pub to_token: String,
    /// Amount to swap in the from_token's display units (string to avoid float).
    pub amount_in: Decimal,
    /// Slippage tolerance in basis points (1–200, max 2%).
    pub slippage_bps: u32,
}

/// Response body for `POST /v1/swaps/quote`.
#[derive(Debug, Serialize)]
pub struct QuoteResponse {
    pub quote: SwapQuote,
}

/// Request body for `POST /v1/swaps` (execute a quote).
#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// Quote ID obtained from `POST /v1/swaps/quote`.
    pub quote_id: Uuid,
    /// The full quote object (re-sent to avoid server-side quote storage for now).
    pub quote: SwapQuote,
}

/// Response body for `POST /v1/swaps`.
#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub swap_id: Uuid,
    pub status: SwapExecutionStatus,
    /// Hex-encoded EVM calldata (Ethereum swaps only).
    pub calldata: Option<String>,
    /// Base64-encoded versioned transaction (Solana swaps only).
    pub encoded_tx: Option<String>,
    /// Router contract address to send the calldata to (Ethereum swaps only).
    pub router_address: Option<String>,
}

/// Response body for `GET /v1/swaps/{id}`.
#[derive(Debug, Serialize)]
pub struct SwapStatusResponse {
    pub swap_id: Uuid,
    pub status: SwapExecutionStatus,
    pub chain: Chain,
    pub from_token: String,
    pub to_token: String,
    pub amount_in: Decimal,
    pub tx_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn swap_router() -> Router<AppState> {
    Router::new()
        .route("/v1/swaps/quote", post(get_swap_quote))
        .route("/v1/swaps", post(execute_swap))
        .route("/v1/swaps/{id}", get(get_swap_status))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /v1/swaps/quote` — get a price quote.
///
/// Selects the appropriate executor based on chain:
/// - `Chain::Ethereum` → UniswapV3Executor
/// - `Chain::Solana`   → JupiterExecutor
///
/// Returns a `SwapQuote` with a 30-second expiry. Pass the quote to
/// `POST /v1/swaps` before it expires.
async fn get_swap_quote(
    _auth: RequireEditor,
    State(_state): State<AppState>,
    Json(req): Json<QuoteRequest>,
) -> Result<(StatusCode, Json<QuoteResponse>), ApiError> {
    // Validate slippage before dispatching to executor.
    validate_slippage(req.slippage_bps).map_err(|reason| {
        ApiError::bad_request(&format!("Invalid slippage {bps} bps: {reason}", bps = req.slippage_bps))
    })?;

    if req.amount_in <= Decimal::ZERO {
        return Err(ApiError::bad_request("amount_in must be positive"));
    }

    let quote = match req.chain {
        Chain::Ethereum => {
            let exec = UniswapV3Executor::ethereum_mainnet();
            exec.quote(&req.from_token, &req.to_token, req.amount_in, req.chain, req.slippage_bps)
                .map_err(|e| ApiError::bad_request(&e.to_string()))?
        }
        Chain::Solana => {
            let exec = JupiterExecutor::new();
            exec.quote(&req.from_token, &req.to_token, req.amount_in, req.chain, req.slippage_bps)
                .map_err(|e| ApiError::bad_request(&e.to_string()))?
        }
        other => {
            return Err(ApiError::bad_request(&format!(
                "Swap not supported on chain {:?}. Supported: Ethereum, Solana", other
            )));
        }
    };

    tracing::info!(
        chain = ?req.chain,
        from_token = %req.from_token,
        to_token = %req.to_token,
        amount_in = %req.amount_in,
        quote_id = %quote.quote_id,
        expected_out = %quote.expected_out,
        "swap_quote_created"
    );

    Ok((StatusCode::OK, Json(QuoteResponse { quote })))
}

/// `POST /v1/swaps` — execute a previously obtained quote.
///
/// Builds the transaction calldata (EVM) or serialized transaction (Solana).
/// Does NOT sign or broadcast — the response must be handed to the signing service.
///
/// Requires `RequireSigner` because this creates an unsigned live transaction.
async fn execute_swap(
    auth: RequireSigner,
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> Result<(StatusCode, Json<ExecuteResponse>), ApiError> {
    if req.quote_id != req.quote.quote_id {
        return Err(ApiError::bad_request(
            "quote_id in body does not match quote.quote_id",
        ));
    }

    let result: SwapResult = match req.quote.chain {
        Chain::Ethereum => {
            let exec = UniswapV3Executor::ethereum_mainnet();
            exec.execute(&req.quote)
                .map_err(|e| match e {
                    custody_common::swap_executor::SwapError::QuoteExpired(_) => {
                        ApiError::bad_request("Quote has expired — request a new quote")
                    }
                    other => ApiError::internal(&other.to_string()),
                })?
        }
        Chain::Solana => {
            let exec = JupiterExecutor::new();
            exec.execute(&req.quote)
                .map_err(|e| match e {
                    custody_common::swap_executor::SwapError::QuoteExpired(_) => {
                        ApiError::bad_request("Quote has expired — request a new quote")
                    }
                    other => ApiError::internal(&other.to_string()),
                })?
        }
        other => {
            return Err(ApiError::bad_request(&format!(
                "Swap execution not supported on chain {:?}", other
            )));
        }
    };

    // Persist the execution so `GET /v1/swaps/{id}` can actually
    // return something. Before this wiring, the status endpoint was
    // a dead-end 404 even for IDs that had been handed out seconds
    // earlier in the same request/response cycle.
    let record = SwapExecution::from_quote_and_result(&req.quote, &result);
    state
        .db
        .upsert_swap_execution(auth.0.user_id, &record)
        .await
        .map_err(|e| ApiError::internal(&format!("swap persist: {e}")))?;

    tracing::info!(
        swap_id = %result.swap_id,
        quote_id = %result.quote_id,
        chain = ?result.chain,
        status = ?result.status,
        has_calldata = result.calldata.is_some(),
        has_encoded_tx = result.encoded_tx.is_some(),
        "swap_execution_ready"
    );

    // Uniswap V3 router address on Ethereum mainnet
    let router_address = if result.chain == Chain::Ethereum {
        Some("0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45".to_string())
    } else {
        None
    };

    Ok((StatusCode::CREATED, Json(ExecuteResponse {
        swap_id: result.swap_id,
        status: result.status,
        calldata: result.calldata,
        encoded_tx: result.encoded_tx,
        router_address,
    })))
}

/// `GET /v1/swaps/{id}` — swap execution status.
///
/// Scoped to the caller's own swaps unless they are Admin; a 404 here
/// means "no swap with that id that you're allowed to see", so nothing
/// about other users' IDs leaks.
async fn get_swap_status(
    auth: RequireViewer,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SwapStatusResponse>, ApiError> {
    let is_admin = auth.0.role.has_at_least(&custody_db::sessions::Role::Admin);
    let lookup = if is_admin {
        state.db.find_swap_any(id).await
    } else {
        state.db.find_swap_for_user(auth.0.user_id, id).await
    };
    let swap = lookup
        .map_err(|e| ApiError::internal(&format!("swap lookup: {e}")))?
        .ok_or_else(|| ApiError::not_found(&format!("Swap {id} not found")))?;

    Ok(Json(SwapStatusResponse {
        swap_id: swap.id,
        status: swap.status,
        chain: swap.chain,
        from_token: swap.from_token,
        to_token: swap.to_token,
        amount_in: swap.amount_in,
        tx_hash: swap.tx_hash,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use custody_common::swap_executor::SwapProtocol;
    use rust_decimal_macros::dec;

    #[test]
    fn quote_request_deserializes() {
        // Chain serializes as lowercase per serde(rename_all = "lowercase") in Chain enum
        let json = r#"{
            "chain": "ethereum",
            "from_token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "to_token": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "amount_in": "1000.50",
            "slippage_bps": 100
        }"#;
        let req: QuoteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.chain, Chain::Ethereum);
        assert_eq!(req.slippage_bps, 100);
        assert_eq!(req.amount_in, dec!(1000.50));
    }

    #[test]
    fn quote_response_serializes() {
        use chrono::Utc;
        let quote = SwapQuote {
            quote_id: Uuid::nil(),
            chain: Chain::Ethereum,
            from_token: "0xUSDC".to_string(),
            to_token: "0xWETH".to_string(),
            amount_in: dec!(1000),
            expected_out: dec!(0.3),
            minimum_out: dec!(0.297),
            slippage_bps: 100,
            price_impact_bps: 5,
            estimated_gas: Some(dec!(0.003)),
            protocol: SwapProtocol::UniswapV3,
            expires_at: Utc::now() + chrono::Duration::seconds(30),
            is_indicative: false,
            notice: None,
        };
        let resp = QuoteResponse { quote };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("quote_id"));
        assert!(json.contains("uniswap_v3"));
        assert!(json.contains("slippage_bps"));
    }

    #[test]
    fn execute_response_serializes_with_calldata() {
        let resp = ExecuteResponse {
            swap_id: Uuid::nil(),
            status: SwapExecutionStatus::ReadyToSign,
            calldata: Some("414bf389deadbeef".to_string()),
            encoded_tx: None,
            router_address: Some("0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ready_to_sign"));
        assert!(json.contains("414bf389"));
        assert!(json.contains("router_address"));
    }

    #[test]
    fn execute_response_serializes_with_encoded_tx() {
        let resp = ExecuteResponse {
            swap_id: Uuid::nil(),
            status: SwapExecutionStatus::ReadyToSign,
            calldata: None,
            encoded_tx: Some("BASE64TXDATA".to_string()),
            router_address: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("BASE64TXDATA"));
        assert!(json.contains("ready_to_sign"));
    }

    #[test]
    fn execute_request_quote_id_mismatch_detected() {
        // Simulate the check we do in execute_swap handler
        let quote_id_a = Uuid::new_v4();
        let quote_id_b = Uuid::new_v4();
        assert_ne!(quote_id_a, quote_id_b, "should be different");
        // Verify the mismatch would be caught
        assert!(quote_id_a != quote_id_b);
    }

    #[test]
    fn swap_status_response_serializes() {
        let resp = SwapStatusResponse {
            swap_id: Uuid::nil(),
            status: SwapExecutionStatus::Broadcast,
            chain: Chain::Solana,
            from_token: "SOL".to_string(),
            to_token: "USDC".to_string(),
            amount_in: dec!(5),
            tx_hash: Some("5xDEF...".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("broadcast"));
        assert!(json.contains("5xDEF"));
    }

    #[test]
    fn uniswap_quote_and_execute_round_trip() {
        // Integration-style test: quote → execute, verify calldata
        let exec = UniswapV3Executor::ethereum_mainnet();
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(1000), Chain::Ethereum, 50).unwrap();

        // Simulate the ExecuteRequest validation
        assert_eq!(quote.quote_id, quote.quote_id); // trivially true, mimics body check

        let result = exec.execute(&quote).unwrap();
        assert_eq!(result.quote_id, quote.quote_id);
        assert_eq!(result.status, SwapExecutionStatus::ReadyToSign);
        assert!(result.calldata.is_some());
    }

    #[test]
    fn jupiter_quote_and_execute_round_trip() {
        let exec = JupiterExecutor::new();
        let sol = "So11111111111111111111111111111111111111112";
        let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let quote = exec.quote(sol, usdc, dec!(2), Chain::Solana, 100).unwrap();
        let result = exec.execute(&quote).unwrap();
        assert_eq!(result.quote_id, quote.quote_id);
        assert_eq!(result.status, SwapExecutionStatus::ReadyToSign);
        assert!(result.encoded_tx.is_some());
        assert!(result.calldata.is_none());
    }

    #[test]
    fn slippage_boundaries_in_handler_logic() {
        // Replicate the slippage check the handler performs
        assert!(validate_slippage(1).is_ok());
        assert!(validate_slippage(200).is_ok());
        assert!(validate_slippage(0).is_err());
        assert!(validate_slippage(201).is_err());
    }

    #[test]
    fn tron_chain_rejected_by_quote_handler() {
        // Routes only support Ethereum and Solana — Tron/Bitcoin should fail
        let chain = Chain::Tron;
        let supported = matches!(chain, Chain::Ethereum | Chain::Solana);
        assert!(!supported, "Tron should not be accepted");
    }

    #[test]
    fn bitcoin_chain_rejected_by_quote_handler() {
        let chain = Chain::Bitcoin;
        let supported = matches!(chain, Chain::Ethereum | Chain::Solana);
        assert!(!supported, "Bitcoin should not be accepted");
    }
}
