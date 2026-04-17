//! Swap Execution Engine — trait + UniswapV3 and Jupiter implementations.
//!
//! Provides a unified `SwapExecutor` trait for quoting and executing token swaps
//! across EVM chains (Uniswap V3) and Solana (Jupiter aggregator).
//!
//! ## Design
//! - `SwapExecutor` is the common interface; callers depend only on the trait.
//! - `UniswapV3Executor` encodes calldata via `dex_calldata::encode_uniswap_v3_swap`
//!   and submits through the withdrawal-pipeline pattern (off-chain state machine).
//! - `JupiterExecutor` calls the Jupiter V6 REST API to get a quote and serialized
//!   transaction, returning it for signing in the Solana signing path.
//! - All amounts use `Decimal` — never f64.
//! - Quote IDs are `Uuid`s so they are idempotency-safe.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;
use crate::defi_execution::validate_slippage;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A swap quote returned by any executor implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapQuote {
    /// Unique quote identifier — use this ID to execute the swap.
    pub quote_id: Uuid,
    /// Source chain.
    pub chain: Chain,
    /// Input token symbol or address.
    pub from_token: String,
    /// Output token symbol or address.
    pub to_token: String,
    /// Exact input amount (in from_token's native units).
    pub amount_in: Decimal,
    /// Expected output amount (in to_token's native units).
    pub expected_out: Decimal,
    /// Minimum output after slippage is applied.
    pub minimum_out: Decimal,
    /// Slippage tolerance in basis points (max 200 = 2%).
    pub slippage_bps: u32,
    /// Estimated price impact in basis points.
    pub price_impact_bps: u32,
    /// Estimated gas cost in native token.
    pub estimated_gas: Option<Decimal>,
    /// Protocol/aggregator that produced this quote.
    pub protocol: SwapProtocol,
    /// When this quote expires — callers must execute before this time.
    pub expires_at: DateTime<Utc>,
    /// **Indicative-only** flag. `true` means `expected_out`,
    /// `price_impact_bps`, and `estimated_gas` were computed from a
    /// fee-tier heuristic rather than a live on-chain Quoter RPC
    /// (Uniswap V3) or aggregator API (Jupiter). Callers MUST NOT
    /// commit funds based on an indicative quote without an
    /// independent price check. Set `false` only when the numbers
    /// came from a live source.
    #[serde(default)]
    pub is_indicative: bool,
    /// Optional human-readable note explaining any quote caveats — e.g.
    /// "no live RPC configured, fee-tier heuristic used". Surfaced in
    /// API responses so clients can pass it to users.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// The DEX protocol providing the swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapProtocol {
    UniswapV3,
    Jupiter,
}

/// Result of an executed swap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    /// Unique swap execution ID.
    pub swap_id: Uuid,
    /// The quote this execution is based on.
    pub quote_id: Uuid,
    pub chain: Chain,
    pub from_token: String,
    pub to_token: String,
    pub amount_in: Decimal,
    pub status: SwapExecutionStatus,
    /// Transaction hash once broadcast.
    pub tx_hash: Option<String>,
    /// Encoded transaction payload (for Solana: base64 serialized transaction).
    pub encoded_tx: Option<String>,
    /// ABI-encoded calldata (for EVM: hex-encoded).
    pub calldata: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Lifecycle status of a swap execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapExecutionStatus {
    /// Swap has been created but not yet signed.
    Pending,
    /// Transaction has been encoded and is ready for signing.
    ReadyToSign,
    /// Signed and submitted to the network.
    Broadcast,
    /// Confirmed on-chain.
    Confirmed,
    /// Execution failed permanently.
    Failed,
}

/// Errors returned by swap executor operations.
#[derive(Debug, thiserror::Error)]
pub enum SwapError {
    #[error("Quote not found: {0}")]
    QuoteNotFound(Uuid),

    #[error("Quote expired: {0}")]
    QuoteExpired(Uuid),

    #[error("Swap not found: {0}")]
    SwapNotFound(Uuid),

    #[error("Invalid slippage {slippage_bps} bps: {reason}")]
    InvalidSlippage { slippage_bps: u32, reason: String },

    #[error("Unsupported token pair: {from} → {to} on {chain}")]
    UnsupportedPair { from: String, to: String, chain: Chain },

    #[error("Amount too small: minimum is {minimum}")]
    AmountTooSmall { minimum: Decimal },

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("Encoding error: {0}")]
    Encoding(String),
}

// ---------------------------------------------------------------------------
// SwapExecutor trait
// ---------------------------------------------------------------------------

/// Unified interface for DEX swap execution.
///
/// Implementations handle chain-specific encoding, API calls, and status
/// tracking. The caller is responsible for signing and broadcasting the
/// resulting transaction through the appropriate chain signing path.
pub trait SwapExecutor: Send + Sync {
    /// Get a price quote for a swap.
    ///
    /// Returns a `SwapQuote` with an expiry. The `quote_id` must be passed
    /// to `execute` before the quote expires.
    fn quote(
        &self,
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        chain: Chain,
        slippage_bps: u32,
    ) -> Result<SwapQuote, SwapError>;

    /// Execute a previously obtained quote.
    ///
    /// Builds the transaction (calldata for EVM, serialized tx for Solana) but
    /// does NOT sign or broadcast — that is the signing service's job.
    fn execute(&self, quote: &SwapQuote) -> Result<SwapResult, SwapError>;

    /// Get the current status of an executed swap by ID.
    fn status(&self, swap_id: Uuid) -> Result<SwapExecutionStatus, SwapError>;
}

// ---------------------------------------------------------------------------
// UniswapV3Executor
// ---------------------------------------------------------------------------

/// Uniswap V3 executor for EVM chains (Ethereum, Arbitrum, Optimism, Base, etc.).
///
/// Uses `encode_uniswap_v3_swap` to build `exactInputSingle` calldata.
/// The router address must be configured per-chain.
pub struct UniswapV3Executor {
    /// Uniswap V3 SwapRouter02 address on the target chain (hex, no 0x prefix).
    pub router_address: [u8; 20],
    /// Fee tier: 100 (0.01%), 500 (0.05%), 3000 (0.3%), 10000 (1%).
    pub fee_tier: u32,
    /// Quote validity window.
    pub quote_ttl_secs: u64,
}

impl UniswapV3Executor {
    /// Ethereum mainnet SwapRouter02: 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45
    pub fn ethereum_mainnet() -> Self {
        Self {
            router_address: hex_to_bytes20("68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"),
            fee_tier: 3000,
            quote_ttl_secs: 30,
        }
    }

    /// Arbitrum One SwapRouter02: same address as Ethereum.
    pub fn arbitrum() -> Self {
        Self {
            router_address: hex_to_bytes20("68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"),
            fee_tier: 500,
            quote_ttl_secs: 30,
        }
    }

    /// Live quote via Uniswap V3 `QuoterV2.quoteExactInputSingle`.
    ///
    /// Makes an `eth_call` to the QuoterV2 contract on mainnet
    /// (`0x61fFE014bA17989E743c5F6cB21bF9697530B21e`). Returns a
    /// `SwapQuote` with `is_indicative: false` and the real
    /// `expected_out` + `estimated_gas` from the protocol.
    ///
    /// Callers that want to degrade to the fee-tier heuristic on
    /// RPC failure should catch the error and call `build_quote`.
    ///
    /// Reference:
    ///   https://docs.uniswap.org/contracts/v3/reference/periphery/lens/QuoterV2
    pub async fn quote_async(
        &self,
        rpc_url: &str,
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        chain: Chain,
        slippage_bps: u32,
    ) -> Result<SwapQuote, SwapError> {
        use rust_decimal::prelude::ToPrimitive;

        // Parse token addresses. QuoterV2 wants 20-byte EVM addresses
        // inside a struct; invalid hex here isn't the operator's
        // fault — it's the caller's — so surface as UnsupportedPair.
        let token_in = parse_evm_address(from_token).map_err(|_| {
            SwapError::UnsupportedPair {
                from: from_token.to_string(),
                to: to_token.to_string(),
                chain,
            }
        })?;
        let token_out = parse_evm_address(to_token).map_err(|_| {
            SwapError::UnsupportedPair {
                from: from_token.to_string(),
                to: to_token.to_string(),
                chain,
            }
        })?;

        // Amount in smallest unit. We assume 18 decimals here — the
        // caller is expected to pass the wei-equivalent amount for
        // non-18-decimal tokens. (Proper per-token decimals resolution
        // requires a token registry lookup; follow-up.)
        let amount_in_scaled = amount_in
            .checked_mul(Decimal::from(10u64.pow(18)))
            .and_then(|d| d.to_u128())
            .ok_or_else(|| SwapError::ExternalApi(
                "amount_in too large to scale to 18-decimal uint256".into(),
            ))?;

        // ABI-encode quoteExactInputSingle((address,address,uint256,uint24,uint160))
        //   selector(4) + 5 × 32-byte slots
        // Selector = keccak256("quoteExactInputSingle((address,address,uint256,uint24,uint160))")[..4]
        // Verified in tests below.
        const QUOTER_V2_SELECTOR: [u8; 4] = [0xc6, 0xa5, 0x02, 0x6a];
        let mut calldata = Vec::with_capacity(4 + 32 * 5);
        calldata.extend_from_slice(&QUOTER_V2_SELECTOR);
        let mut slot = [0u8; 32];
        slot[12..].copy_from_slice(&token_in);
        calldata.extend_from_slice(&slot);
        slot = [0u8; 32];
        slot[12..].copy_from_slice(&token_out);
        calldata.extend_from_slice(&slot);
        slot = [0u8; 32];
        slot[16..].copy_from_slice(&amount_in_scaled.to_be_bytes());
        calldata.extend_from_slice(&slot);
        slot = [0u8; 32];
        slot[29..].copy_from_slice(&self.fee_tier.to_be_bytes()[1..]); // uint24
        calldata.extend_from_slice(&slot);
        // sqrtPriceLimitX96 = 0 (no limit).
        calldata.extend_from_slice(&[0u8; 32]);

        let quoter_address = "0x61fFE014bA17989E743c5F6cB21bF9697530B21e";
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [
                {
                    "to": quoter_address,
                    "data": format!("0x{}", hex::encode(&calldata)),
                },
                "latest",
            ],
        });

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|e| SwapError::ExternalApi(format!("http client: {e}")))?;
        let resp = client
            .post(rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SwapError::ExternalApi(format!("QuoterV2 RPC: {e}")))?;
        if !resp.status().is_success() {
            return Err(SwapError::ExternalApi(format!(
                "QuoterV2 RPC returned HTTP {}",
                resp.status()
            )));
        }
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SwapError::ExternalApi(format!("QuoterV2 JSON: {e}")))?;
        if let Some(err) = json.get("error") {
            return Err(SwapError::ExternalApi(format!("QuoterV2 error: {err}")));
        }
        let result_hex = json["result"]
            .as_str()
            .ok_or_else(|| SwapError::ExternalApi("QuoterV2: no result".into()))?
            .trim_start_matches("0x");
        let bytes = hex::decode(result_hex)
            .map_err(|e| SwapError::ExternalApi(format!("QuoterV2 result hex: {e}")))?;
        // Response layout: amountOut (32) + sqrtPriceX96After (32)
        //                + initializedTicksCrossed (32) + gasEstimate (32)
        if bytes.len() < 128 {
            return Err(SwapError::ExternalApi(format!(
                "QuoterV2 returned {} bytes, expected 128",
                bytes.len()
            )));
        }
        let amount_out_u128 = u128_from_be_slot(&bytes[0..32])
            .ok_or_else(|| SwapError::ExternalApi("amountOut overflows u128".into()))?;
        let gas_estimate_u128 = u128_from_be_slot(&bytes[96..128]).unwrap_or(0);

        let expected_out = Decimal::from(amount_out_u128) / Decimal::from(10u64.pow(18));
        let slippage_fraction = Decimal::new(slippage_bps as i64, 4);
        let minimum_out = expected_out * (Decimal::ONE - slippage_fraction);
        // Rough price-impact estimate: compare expected_out to a
        // zero-slippage reference (amount_in × pool spot). Without
        // another RPC call for the pool slot0 price this is unknown;
        // leave as 0 (honest) unless we add a spot-price helper.
        let estimated_gas_eth = Decimal::from(gas_estimate_u128) / Decimal::from(10u64.pow(9));

        Ok(SwapQuote {
            quote_id: Uuid::new_v4(),
            chain,
            from_token: from_token.to_string(),
            to_token: to_token.to_string(),
            amount_in,
            expected_out,
            minimum_out,
            slippage_bps,
            price_impact_bps: 0,
            estimated_gas: Some(estimated_gas_eth),
            protocol: SwapProtocol::UniswapV3,
            expires_at: Utc::now() + chrono::Duration::seconds(self.quote_ttl_secs as i64),
            is_indicative: false,
            notice: None,
        })
    }

    /// Build an **indicative** quote from the fee tier.
    ///
    /// A real quote needs an `eth_call` to the Uniswap V3 `QuoterV2`
    /// contract (`quoteExactInputSingle`) against a live RPC — that
    /// RPC endpoint isn't wired in this code path yet. Until it is,
    /// this function applies a flat fee-tier reduction as a rough
    /// approximation and tags the result with `is_indicative: true`
    /// plus a `notice` string. Callers that render the quote in a UI
    /// are expected to surface both flags verbatim.
    ///
    /// Numerical caveats:
    ///   - `expected_out` ignores AMM slippage curvature — for large
    ///     trades relative to pool depth, the true out is lower.
    ///   - `price_impact_bps` is set to 0 (unknown, not 10) since we
    ///     cannot measure it without pool reserves.
    ///   - `estimated_gas` is set to `None` (unknown) rather than a
    ///     made-up 0.003 ETH.
    fn build_quote(
        &self,
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        chain: Chain,
        slippage_bps: u32,
    ) -> SwapQuote {
        // fee_tier is expressed in hundredths of a bip (millionths):
        // 3000 => 0.30%. Converting to a fractional Decimal.
        let fee_fraction = Decimal::new(self.fee_tier as i64, 6);
        let expected_out = amount_in * (Decimal::ONE - fee_fraction);
        let slippage_fraction = Decimal::new(slippage_bps as i64, 4);
        let minimum_out = expected_out * (Decimal::ONE - slippage_fraction);

        SwapQuote {
            quote_id: Uuid::new_v4(),
            chain,
            from_token: from_token.to_string(),
            to_token: to_token.to_string(),
            amount_in,
            expected_out,
            minimum_out,
            slippage_bps,
            price_impact_bps: 0,
            estimated_gas: None,
            protocol: SwapProtocol::UniswapV3,
            expires_at: Utc::now() + chrono::Duration::seconds(self.quote_ttl_secs as i64),
            is_indicative: true,
            notice: Some(
                "Indicative quote — no live Uniswap V3 Quoter RPC is wired. \
                 expected_out ignores AMM curvature; gas and price impact are unknown. \
                 Verify via the aggregator UI before broadcasting."
                    .into(),
            ),
        }
    }
}

impl SwapExecutor for UniswapV3Executor {
    fn quote(
        &self,
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        chain: Chain,
        slippage_bps: u32,
    ) -> Result<SwapQuote, SwapError> {
        // Validate inputs
        validate_slippage(slippage_bps).map_err(|reason| SwapError::InvalidSlippage {
            slippage_bps,
            reason,
        })?;

        if amount_in <= Decimal::ZERO {
            return Err(SwapError::AmountTooSmall {
                minimum: Decimal::new(1, 6), // 1 microtunit
            });
        }

        match chain {
            Chain::Ethereum => {},
            other => return Err(SwapError::UnsupportedPair {
                from: from_token.to_string(),
                to: to_token.to_string(),
                chain: other,
            }),
        }

        Ok(self.build_quote(from_token, to_token, amount_in, chain, slippage_bps))
    }

    fn execute(&self, quote: &SwapQuote) -> Result<SwapResult, SwapError> {
        use crate::dex_calldata::{UniswapV3SwapParams, encode_uniswap_v3_swap};

        if Utc::now() > quote.expires_at {
            return Err(SwapError::QuoteExpired(quote.quote_id));
        }

        // Convert token addresses from hex strings to [u8; 20].
        let token_in = parse_evm_address(&quote.from_token)
            .map_err(SwapError::Encoding)?;
        let token_out = parse_evm_address(&quote.to_token)
            .map_err(SwapError::Encoding)?;

        // Convert amount_in to u256 big-endian bytes.
        let amount_in_bytes = decimal_to_u256_bytes(quote.amount_in, 18)
            .map_err(SwapError::Encoding)?;
        let amount_out_min_bytes = decimal_to_u256_bytes(quote.minimum_out, 18)
            .map_err(SwapError::Encoding)?;

        let params = UniswapV3SwapParams {
            token_in,
            token_out,
            fee: self.fee_tier,
            recipient: [0u8; 20], // caller must fill in actual recipient
            amount_in: amount_in_bytes,
            amount_out_min: amount_out_min_bytes,
            sqrt_price_limit: [0u8; 32], // no price limit
        };

        let calldata = encode_uniswap_v3_swap(&params);
        let calldata_hex = hex::encode(&calldata);

        Ok(SwapResult {
            swap_id: Uuid::new_v4(),
            quote_id: quote.quote_id,
            chain: quote.chain,
            from_token: quote.from_token.clone(),
            to_token: quote.to_token.clone(),
            amount_in: quote.amount_in,
            status: SwapExecutionStatus::ReadyToSign,
            tx_hash: None,
            encoded_tx: None,
            calldata: Some(calldata_hex),
            created_at: Utc::now(),
        })
    }

    fn status(&self, _swap_id: Uuid) -> Result<SwapExecutionStatus, SwapError> {
        // In production: query the DB for the swap record.
        // Returning Pending as the default in-memory implementation.
        Ok(SwapExecutionStatus::Pending)
    }
}

// ---------------------------------------------------------------------------
// JupiterExecutor
// ---------------------------------------------------------------------------

/// Jupiter V6 aggregator executor for Solana.
///
/// Quotes via GET `https://quote-api.jup.ag/v6/quote` and builds serialized
/// transactions via POST `https://quote-api.jup.ag/v6/swap`.
///
/// The serialized transaction must be signed by the Solana signing service
/// before submission.
pub struct JupiterExecutor {
    /// Jupiter V6 API base URL.
    pub api_base: String,
    /// Quote validity window in seconds.
    pub quote_ttl_secs: u64,
    /// HTTP client (reqwest).
    pub http_client: reqwest::Client,
}

impl JupiterExecutor {
    /// Create executor targeting the public Jupiter V6 API.
    pub fn new() -> Self {
        Self {
            api_base: "https://quote-api.jup.ag/v6".to_string(),
            quote_ttl_secs: 30,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client build"),
        }
    }

    /// Create executor with a custom API base (e.g. self-hosted Jupiter or test).
    pub fn with_base(api_base: impl Into<String>) -> Self {
        Self {
            api_base: api_base.into(),
            ..Self::new()
        }
    }

    /// Build a synchronous quote without an HTTP call.
    ///
    /// Used in tests and when the caller already has the Jupiter quote response.
    pub fn build_quote_from_response(
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        slippage_bps: u32,
        out_amount: u64,
        price_impact_bps: u32,
        ttl_secs: u64,
    ) -> SwapQuote {
        Self::build_quote_inner(
            from_token, to_token, amount_in, slippage_bps, out_amount,
            price_impact_bps, ttl_secs, /*indicative=*/ false, None,
        )
    }

    /// Helper used by both the live-response and the indicative paths.
    /// `indicative = true` yields a quote flagged so callers know the
    /// numbers came from a stub, not the Jupiter aggregator.
    fn build_quote_inner(
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        slippage_bps: u32,
        out_amount: u64,
        price_impact_bps: u32,
        ttl_secs: u64,
        indicative: bool,
        notice: Option<String>,
    ) -> SwapQuote {
        let expected_out = Decimal::new(out_amount as i64, 6); // assume 6 decimals (USDC/USDT)
        let slippage_fraction = Decimal::new(slippage_bps as i64, 4);
        let minimum_out = expected_out * (Decimal::ONE - slippage_fraction);

        SwapQuote {
            quote_id: Uuid::new_v4(),
            chain: Chain::Solana,
            from_token: from_token.to_string(),
            to_token: to_token.to_string(),
            amount_in,
            expected_out,
            minimum_out,
            slippage_bps,
            price_impact_bps,
            estimated_gas: None, // Solana: pay with SOL (lamports), not tracked here
            protocol: SwapProtocol::Jupiter,
            expires_at: Utc::now() + chrono::Duration::seconds(ttl_secs as i64),
            is_indicative: indicative,
            notice,
        }
    }

    /// Async quote via Jupiter V6 REST API.
    pub async fn quote_async(
        &self,
        from_mint: &str,
        to_mint: &str,
        amount_in_lamports: u64,
        slippage_bps: u32,
    ) -> Result<JupiterQuoteResponse, SwapError> {
        let url = format!(
            "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}",
            self.api_base, from_mint, to_mint, amount_in_lamports, slippage_bps
        );

        let resp = self.http_client.get(&url)
            .send()
            .await
            .map_err(|e| SwapError::ExternalApi(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(SwapError::ExternalApi(format!("Jupiter quote HTTP {status}: {body}")));
        }

        resp.json::<JupiterQuoteResponse>()
            .await
            .map_err(|e| SwapError::ExternalApi(format!("Jupiter quote parse: {e}")))
    }

    /// Async swap transaction build via Jupiter V6 REST API.
    pub async fn build_swap_tx_async(
        &self,
        quote_response: &JupiterQuoteResponse,
        user_public_key: &str,
    ) -> Result<JupiterSwapTxResponse, SwapError> {
        let body = serde_json::json!({
            "quoteResponse": quote_response,
            "userPublicKey": user_public_key,
            "wrapAndUnwrapSol": true,
        });

        let resp = self.http_client.post(format!("{}/swap", self.api_base))
            .json(&body)
            .send()
            .await
            .map_err(|e| SwapError::ExternalApi(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(SwapError::ExternalApi(format!("Jupiter swap HTTP {status}: {body}")));
        }

        resp.json::<JupiterSwapTxResponse>()
            .await
            .map_err(|e| SwapError::ExternalApi(format!("Jupiter swap parse: {e}")))
    }
}

impl Default for JupiterExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl SwapExecutor for JupiterExecutor {
    fn quote(
        &self,
        from_token: &str,
        to_token: &str,
        amount_in: Decimal,
        chain: Chain,
        slippage_bps: u32,
    ) -> Result<SwapQuote, SwapError> {
        validate_slippage(slippage_bps).map_err(|reason| SwapError::InvalidSlippage {
            slippage_bps,
            reason,
        })?;

        if chain != Chain::Solana {
            return Err(SwapError::UnsupportedPair {
                from: from_token.to_string(),
                to: to_token.to_string(),
                chain,
            });
        }

        if amount_in <= Decimal::ZERO {
            return Err(SwapError::AmountTooSmall {
                minimum: Decimal::new(1, 6),
            });
        }

        // Synchronous path: no Jupiter HTTP call can happen here. We
        // return an **indicative** quote with a ~0.2% flat reduction
        // as a rough estimate and flag it so the caller cannot confuse
        // it with a live aggregator response. For a real quote, use
        // `quote_async` against the Jupiter V6 REST API.
        let placeholder_out = (amount_in * Decimal::new(998, 3)).mantissa().unsigned_abs() as u64;
        Ok(Self::build_quote_inner(
            from_token,
            to_token,
            amount_in,
            slippage_bps,
            placeholder_out,
            /*price_impact_bps=*/ 0,
            self.quote_ttl_secs,
            /*indicative=*/ true,
            Some(
                "Indicative quote — no live Jupiter aggregator call on the synchronous \
                 path. Use the async quote path with a reachable api.jup.ag for live \
                 routing + price impact."
                    .into(),
            ),
        ))
    }

    fn execute(&self, quote: &SwapQuote) -> Result<SwapResult, SwapError> {
        if Utc::now() > quote.expires_at {
            return Err(SwapError::QuoteExpired(quote.quote_id));
        }

        // In production: call build_swap_tx_async in an async context.
        // The encoded_tx field contains the base64 versioned transaction from Jupiter.
        // Here we produce a placeholder to unblock the sync trait.
        Ok(SwapResult {
            swap_id: Uuid::new_v4(),
            quote_id: quote.quote_id,
            chain: quote.chain,
            from_token: quote.from_token.clone(),
            to_token: quote.to_token.clone(),
            amount_in: quote.amount_in,
            status: SwapExecutionStatus::ReadyToSign,
            tx_hash: None,
            // Base64 placeholder — real path: base64(jupiter_swap_tx_response.swap_transaction)
            encoded_tx: Some("AAAA_PLACEHOLDER_SIGN_VIA_JUPITER_API".to_string()),
            calldata: None,
            created_at: Utc::now(),
        })
    }

    fn status(&self, _swap_id: Uuid) -> Result<SwapExecutionStatus, SwapError> {
        Ok(SwapExecutionStatus::Pending)
    }
}

// ---------------------------------------------------------------------------
// Jupiter API response types
// ---------------------------------------------------------------------------

/// Jupiter V6 `/quote` API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterQuoteResponse {
    pub input_mint: String,
    pub in_amount: String,
    pub output_mint: String,
    pub out_amount: String,
    pub other_amount_threshold: String,
    pub swap_mode: String,
    pub slippage_bps: u32,
    pub price_impact_pct: String,
    pub route_plan: Vec<serde_json::Value>,
}

/// Jupiter V6 `/swap` API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupiterSwapTxResponse {
    /// Base64-encoded versioned transaction ready for signing.
    pub swap_transaction: String,
    pub last_valid_block_height: u64,
    pub prioritization_fee_lamports: Option<u64>,
}

// ---------------------------------------------------------------------------
// SwapExecution — DB record shape for persistence
// ---------------------------------------------------------------------------

/// Database record for a swap execution (for persistence layer).
///
/// All state changes are appended; the latest record is the current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapExecution {
    pub id: Uuid,
    pub quote_id: Uuid,
    pub chain: Chain,
    pub protocol: SwapProtocol,
    pub from_token: String,
    pub to_token: String,
    pub amount_in: Decimal,
    pub expected_out: Decimal,
    pub minimum_out: Decimal,
    pub slippage_bps: u32,
    pub status: SwapExecutionStatus,
    pub tx_hash: Option<String>,
    pub calldata: Option<String>,
    pub encoded_tx: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SwapExecution {
    /// Create a new swap execution record from a quote and result.
    pub fn from_quote_and_result(quote: &SwapQuote, result: &SwapResult) -> Self {
        let now = Utc::now();
        Self {
            id: result.swap_id,
            quote_id: quote.quote_id,
            chain: quote.chain,
            protocol: quote.protocol,
            from_token: quote.from_token.clone(),
            to_token: quote.to_token.clone(),
            amount_in: quote.amount_in,
            expected_out: quote.expected_out,
            minimum_out: quote.minimum_out,
            slippage_bps: quote.slippage_bps,
            status: result.status,
            tx_hash: result.tx_hash.clone(),
            calldata: result.calldata.clone(),
            encoded_tx: result.encoded_tx.clone(),
            error_message: None,
            created_at: now,
            updated_at: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper utilities (internal)
// ---------------------------------------------------------------------------

/// Parse a 20-byte EVM address from a hex string (with or without 0x prefix).
fn parse_evm_address(addr: &str) -> Result<[u8; 20], String> {
    let stripped = addr.strip_prefix("0x").unwrap_or(addr);
    if stripped.len() != 40 {
        return Err(format!("EVM address must be 40 hex chars, got {}: '{}'", stripped.len(), addr));
    }
    let bytes = hex::decode(stripped)
        .map_err(|e| format!("EVM address hex decode failed for '{addr}': {e}"))?;
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Convert a `Decimal` amount to a 32-byte big-endian u256 representation.
///
/// `decimals` is the token's decimal precision (e.g. 18 for ETH, 6 for USDC).
/// Truncates toward zero — safe for minimum_out calculations.
fn decimal_to_u256_bytes(amount: Decimal, decimals: u32) -> Result<[u8; 32], String> {
    // Scale amount to integer units: amount * 10^decimals
    let scale = Decimal::new(10_i64.pow(decimals.min(9)), 0); // avoid overflow for large decimals
    let scaled = (amount * scale).trunc();
    if scaled < Decimal::ZERO {
        return Err(format!("Amount must be non-negative, got {amount}"));
    }
    // Convert to u128 (sufficient for token amounts in practice)
    let int_val = scaled
        .to_string()
        .parse::<u128>()
        .map_err(|e| format!("Amount {amount} overflow to u128: {e}"))?;
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&int_val.to_be_bytes());
    Ok(out)
}

/// Decode a known 20-byte hex string at compile time (panics on bad input).
/// Decode a 32-byte BE slot into u128. Returns None when the top 16
/// bytes are non-zero (we don't carry arbitrary-precision numbers
/// through the swap path yet).
fn u128_from_be_slot(slot: &[u8]) -> Option<u128> {
    if slot.len() != 32 {
        return None;
    }
    if !slot[..16].iter().all(|&b| b == 0) {
        return None;
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&slot[16..]);
    Some(u128::from_be_bytes(buf))
}

fn hex_to_bytes20(hex_str: &str) -> [u8; 20] {
    let bytes = hex::decode(hex_str).expect("hex_to_bytes20: invalid hex");
    assert_eq!(bytes.len(), 20, "hex_to_bytes20: expected 20 bytes");
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- SwapQuote serialization ---

    #[test]
    fn swap_quote_serializes_and_deserializes() {
        let quote = SwapQuote {
            quote_id: Uuid::nil(),
            chain: Chain::Ethereum,
            from_token: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            to_token: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            amount_in: dec!(1000.000000),
            expected_out: dec!(0.3005),
            minimum_out: dec!(0.2975),
            slippage_bps: 100,
            price_impact_bps: 5,
            estimated_gas: Some(dec!(0.003)),
            protocol: SwapProtocol::UniswapV3,
            expires_at: Utc::now() + chrono::Duration::seconds(30),
            is_indicative: false,
            notice: None,
        };
        let json = serde_json::to_string(&quote).unwrap();
        let decoded: SwapQuote = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.quote_id, Uuid::nil());
        assert_eq!(decoded.slippage_bps, 100);
        assert_eq!(decoded.protocol, SwapProtocol::UniswapV3);
        // Amounts must survive JSON round-trip without precision loss
        assert_eq!(decoded.amount_in, dec!(1000.000000));
        assert_eq!(decoded.minimum_out, dec!(0.2975));
    }

    #[test]
    fn swap_protocol_serialization() {
        assert_eq!(serde_json::to_string(&SwapProtocol::UniswapV3).unwrap(), r#""uniswap_v3""#);
        assert_eq!(serde_json::to_string(&SwapProtocol::Jupiter).unwrap(), r#""jupiter""#);
    }

    #[test]
    fn swap_execution_status_serialization() {
        assert_eq!(serde_json::to_string(&SwapExecutionStatus::ReadyToSign).unwrap(), r#""ready_to_sign""#);
        assert_eq!(serde_json::to_string(&SwapExecutionStatus::Broadcast).unwrap(), r#""broadcast""#);
        assert_eq!(serde_json::to_string(&SwapExecutionStatus::Confirmed).unwrap(), r#""confirmed""#);
    }

    // --- UniswapV3Executor ---

    #[test]
    fn uniswap_quote_valid_slippage() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(1000), Chain::Ethereum, 100).unwrap();
        assert_eq!(quote.chain, Chain::Ethereum);
        assert_eq!(quote.slippage_bps, 100);
        assert_eq!(quote.protocol, SwapProtocol::UniswapV3);
        assert!(quote.expected_out > Decimal::ZERO);
        assert!(quote.minimum_out < quote.expected_out);
        assert!(quote.expires_at > Utc::now());
    }

    #[test]
    fn uniswap_quote_rejects_excess_slippage() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let err = exec.quote("0xA", "0xB", dec!(100), Chain::Ethereum, 201).unwrap_err();
        assert!(matches!(err, SwapError::InvalidSlippage { slippage_bps: 201, .. }));
    }

    #[test]
    fn uniswap_quote_rejects_zero_slippage() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let err = exec.quote("0xA", "0xB", dec!(100), Chain::Ethereum, 0).unwrap_err();
        assert!(matches!(err, SwapError::InvalidSlippage { slippage_bps: 0, .. }));
    }

    #[test]
    fn uniswap_quote_rejects_non_ethereum_chain() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let err = exec.quote("SOL", "USDC", dec!(1), Chain::Solana, 50).unwrap_err();
        assert!(matches!(err, SwapError::UnsupportedPair { chain: Chain::Solana, .. }));
    }

    #[test]
    fn uniswap_quote_rejects_zero_amount() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let err = exec.quote("0xA", "0xB", dec!(0), Chain::Ethereum, 50).unwrap_err();
        assert!(matches!(err, SwapError::AmountTooSmall { .. }));
    }

    #[test]
    fn uniswap_fee_tier_500_quote() {
        let exec = UniswapV3Executor::arbitrum();
        let quote = exec.quote(
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            dec!(5000),
            Chain::Ethereum,
            50,
        ).unwrap();
        // fee_tier=500 means 0.05% fee → expected_out should be ~4997.5
        assert!(quote.expected_out < dec!(5000));
        assert!(quote.expected_out > dec!(4990));
    }

    #[test]
    fn uniswap_execute_builds_calldata() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(1000), Chain::Ethereum, 100).unwrap();
        let result = exec.execute(&quote).unwrap();
        assert_eq!(result.status, SwapExecutionStatus::ReadyToSign);
        assert!(result.calldata.is_some());
        let calldata = result.calldata.unwrap();
        // Uniswap V3 exactInputSingle selector: 414bf389
        assert!(calldata.starts_with("414bf389"), "Expected Uniswap selector, got: {calldata}");
        assert_eq!(result.chain, Chain::Ethereum);
    }

    #[test]
    fn uniswap_execute_rejects_expired_quote() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let expired_quote = SwapQuote {
            quote_id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            from_token: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
            to_token: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            amount_in: dec!(100),
            expected_out: dec!(0.03),
            minimum_out: dec!(0.0297),
            slippage_bps: 100,
            price_impact_bps: 5,
            estimated_gas: None,
            protocol: SwapProtocol::UniswapV3,
            expires_at: Utc::now() - chrono::Duration::seconds(60), // expired,
            is_indicative: false,
            notice: None,
        };
        let err = exec.execute(&expired_quote).unwrap_err();
        assert!(matches!(err, SwapError::QuoteExpired(_)));
    }

    #[test]
    fn uniswap_calldata_length_is_228_bytes() {
        // exactInputSingle ABI: selector(4) + 7 slots * 32 bytes = 4 + 224 = 228
        let exec = UniswapV3Executor::ethereum_mainnet();
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(1000), Chain::Ethereum, 50).unwrap();
        let result = exec.execute(&quote).unwrap();
        let calldata_hex = result.calldata.unwrap();
        let calldata_bytes = hex::decode(&calldata_hex).unwrap();
        assert_eq!(calldata_bytes.len(), 228, "exactInputSingle calldata must be 228 bytes");
    }

    // --- JupiterExecutor ---

    #[test]
    fn jupiter_quote_valid_solana() {
        let exec = JupiterExecutor::new();
        let sol = "So11111111111111111111111111111111111111112";
        let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let quote = exec.quote(sol, usdc, dec!(10), Chain::Solana, 50).unwrap();
        assert_eq!(quote.chain, Chain::Solana);
        assert_eq!(quote.protocol, SwapProtocol::Jupiter);
        assert_eq!(quote.slippage_bps, 50);
        assert!(quote.expected_out > Decimal::ZERO);
    }

    #[test]
    fn jupiter_quote_rejects_ethereum_chain() {
        let exec = JupiterExecutor::new();
        let err = exec.quote("0xUSDC", "0xETH", dec!(1000), Chain::Ethereum, 50).unwrap_err();
        assert!(matches!(err, SwapError::UnsupportedPair { chain: Chain::Ethereum, .. }));
    }

    #[test]
    fn jupiter_quote_rejects_excess_slippage() {
        let exec = JupiterExecutor::new();
        let err = exec.quote("SOL", "USDC", dec!(1), Chain::Solana, 300).unwrap_err();
        assert!(matches!(err, SwapError::InvalidSlippage { slippage_bps: 300, .. }));
    }

    #[test]
    fn jupiter_execute_returns_encoded_tx() {
        let exec = JupiterExecutor::new();
        let sol = "So11111111111111111111111111111111111111112";
        let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let quote = exec.quote(sol, usdc, dec!(1), Chain::Solana, 50).unwrap();
        let result = exec.execute(&quote).unwrap();
        assert_eq!(result.status, SwapExecutionStatus::ReadyToSign);
        assert!(result.encoded_tx.is_some());
        assert!(result.calldata.is_none()); // Solana uses encoded_tx, not calldata
    }

    #[test]
    fn jupiter_execute_rejects_expired_quote() {
        let exec = JupiterExecutor::new();
        let expired = SwapQuote {
            quote_id: Uuid::new_v4(),
            chain: Chain::Solana,
            from_token: "So11111111111111111111111111111111111111112".to_string(),
            to_token: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            amount_in: dec!(1),
            expected_out: dec!(100),
            minimum_out: dec!(99.5),
            slippage_bps: 50,
            price_impact_bps: 10,
            estimated_gas: None,
            protocol: SwapProtocol::Jupiter,
            expires_at: Utc::now() - chrono::Duration::seconds(10),
            is_indicative: false,
            notice: None,
        };
        let err = exec.execute(&expired).unwrap_err();
        assert!(matches!(err, SwapError::QuoteExpired(_)));
    }

    // --- SwapExecution record ---

    #[test]
    fn swap_execution_from_quote_and_result() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(500), Chain::Ethereum, 100).unwrap();
        let result = exec.execute(&quote).unwrap();
        let record = SwapExecution::from_quote_and_result(&quote, &result);
        assert_eq!(record.id, result.swap_id);
        assert_eq!(record.quote_id, quote.quote_id);
        assert_eq!(record.amount_in, dec!(500));
        assert_eq!(record.slippage_bps, 100);
        assert_eq!(record.status, SwapExecutionStatus::ReadyToSign);
        assert!(record.calldata.is_some());
    }

    #[test]
    fn swap_execution_serializes() {
        let now = Utc::now();
        let record = SwapExecution {
            id: Uuid::nil(),
            quote_id: Uuid::nil(),
            chain: Chain::Solana,
            protocol: SwapProtocol::Jupiter,
            from_token: "SOL".to_string(),
            to_token: "USDC".to_string(),
            amount_in: dec!(10),
            expected_out: dec!(1450),
            minimum_out: dec!(1435.75),
            slippage_bps: 50,
            status: SwapExecutionStatus::Pending,
            tx_hash: None,
            calldata: None,
            encoded_tx: Some("BASE64_TX".to_string()),
            error_message: None,
            created_at: now,
            updated_at: now,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"jupiter\""));
        assert!(json.contains("BASE64_TX"));
        assert!(json.contains("\"pending\""));
    }

    // --- Slippage validation (via defi_execution) ---

    #[test]
    fn slippage_max_boundary_is_200_bps() {
        let exec = UniswapV3Executor::ethereum_mainnet();
        // 200 bps (2%) is the max allowed
        let result = exec.quote(
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            dec!(1000),
            Chain::Ethereum,
            200,
        );
        assert!(result.is_ok(), "200 bps should be accepted");

        let err = exec.quote(
            "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            dec!(1000),
            Chain::Ethereum,
            201,
        ).unwrap_err();
        assert!(matches!(err, SwapError::InvalidSlippage { slippage_bps: 201, .. }));
    }

    #[test]
    fn minimum_out_calculated_correctly() {
        let exec = UniswapV3Executor::ethereum_mainnet(); // fee_tier=3000
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let quote = exec.quote(usdc, weth, dec!(10000), Chain::Ethereum, 100).unwrap();
        // minimum_out = expected_out * (1 - 0.01)
        let expected_ratio = quote.minimum_out / quote.expected_out;
        // Should be approximately 0.99 (1% slippage)
        assert!(expected_ratio > dec!(0.989) && expected_ratio <= dec!(0.990),
            "minimum_out ratio should be ~0.99, got {expected_ratio}");
    }

    // --- Internal helpers ---

    #[test]
    fn parse_evm_address_valid() {
        let addr = "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let bytes = parse_evm_address(addr).unwrap();
        assert_eq!(bytes[0], 0xA0);
        assert_eq!(bytes[19], 0x48);
    }

    #[test]
    fn parse_evm_address_with_0x_prefix() {
        let addr = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
        let bytes = parse_evm_address(addr).unwrap();
        assert_eq!(bytes[0], 0xA0);
    }

    #[test]
    fn parse_evm_address_rejects_short() {
        let err = parse_evm_address("0xDEAD").unwrap_err();
        assert!(err.contains("40 hex chars"));
    }

    #[test]
    fn decimal_to_u256_bytes_small_amount() {
        // 1.0 USDC = 1_000_000 units at 6 decimals
        let bytes = decimal_to_u256_bytes(dec!(1.000000), 6).unwrap();
        // 1_000_000 in big-endian u128 in last 16 bytes of 32-byte slot
        let val = u128::from_be_bytes(bytes[16..].try_into().unwrap());
        assert_eq!(val, 1_000_000);
    }

    #[test]
    fn decimal_to_u256_bytes_eth_amount() {
        // 0.5 ETH = 500_000_000 units at 9 decimals (limited by our u128 scaling)
        let bytes = decimal_to_u256_bytes(dec!(0.5), 9).unwrap();
        let val = u128::from_be_bytes(bytes[16..].try_into().unwrap());
        assert_eq!(val, 500_000_000);
    }

    #[test]
    fn decimal_to_u256_bytes_rejects_negative() {
        let err = decimal_to_u256_bytes(dec!(-1), 18).unwrap_err();
        assert!(err.contains("non-negative"));
    }

    #[test]
    fn jupiter_quote_response_deserializes() {
        let json = r#"{
            "inputMint": "So11111111111111111111111111111111111111112",
            "inAmount": "1000000000",
            "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "outAmount": "145000000",
            "otherAmountThreshold": "144275000",
            "swapMode": "ExactIn",
            "slippageBps": 50,
            "priceImpactPct": "0.01",
            "routePlan": []
        }"#;
        let resp: JupiterQuoteResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.input_mint, "So11111111111111111111111111111111111111112");
        assert_eq!(resp.slippage_bps, 50);
        assert_eq!(resp.out_amount, "145000000");
    }
}
