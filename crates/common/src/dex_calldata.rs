//! DEX calldata builders — Uniswap V3, Curve, token approvals, bridges.
//!
//! Real ABI-encoded calldata for on-chain DeFi interactions.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

// ---------------------------------------------------------------------------
// Uniswap V3 — exactInputSingle
// ---------------------------------------------------------------------------

/// Parameters for Uniswap V3 exactInputSingle swap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniswapV3SwapParams {
    pub token_in: [u8; 20],
    pub token_out: [u8; 20],
    pub fee: u32,           // 500, 3000, or 10000
    pub recipient: [u8; 20],
    pub amount_in: [u8; 32],
    pub amount_out_min: [u8; 32],
    pub sqrt_price_limit: [u8; 32], // 0 for no limit
}

/// Encode Uniswap V3 `exactInputSingle` calldata.
///
/// Function: exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))
/// Selector: 0x414bf389
pub fn encode_uniswap_v3_swap(params: &UniswapV3SwapParams) -> Vec<u8> {
    let mut data = Vec::with_capacity(228);
    // Function selector
    data.extend_from_slice(&[0x41, 0x4b, 0xf3, 0x89]);

    // tokenIn (address padded to 32)
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(&params.token_in);
    data.extend_from_slice(&slot);

    // tokenOut
    slot = [0u8; 32];
    slot[12..].copy_from_slice(&params.token_out);
    data.extend_from_slice(&slot);

    // fee (uint24 in 32 bytes)
    slot = [0u8; 32];
    slot[29..32].copy_from_slice(&params.fee.to_be_bytes()[1..]);
    data.extend_from_slice(&slot);

    // recipient
    slot = [0u8; 32];
    slot[12..].copy_from_slice(&params.recipient);
    data.extend_from_slice(&slot);

    // amountIn
    data.extend_from_slice(&params.amount_in);

    // amountOutMinimum
    data.extend_from_slice(&params.amount_out_min);

    // sqrtPriceLimitX96
    data.extend_from_slice(&params.sqrt_price_limit);

    data
}

// ---------------------------------------------------------------------------
// ERC-20 approve (for DEX interactions)
// ---------------------------------------------------------------------------

/// Encode ERC-20 `approve(address,uint256)` calldata.
///
/// Selector: 0x095ea7b3
pub fn encode_erc20_approve(spender: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&[0x09, 0x5e, 0xa7, 0xb3]);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(spender);
    data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    data
}

/// Encode ERC-20 `allowance(address,address)` calldata (view function).
///
/// Selector: 0xdd62ed3e
pub fn encode_erc20_allowance(owner: &[u8; 20], spender: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&[0xdd, 0x62, 0xed, 0x3e]);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(owner);
    data.extend_from_slice(&slot);
    slot = [0u8; 32];
    slot[12..].copy_from_slice(spender);
    data.extend_from_slice(&slot);
    data
}

/// Max uint256 for unlimited approval.
pub const MAX_UINT256: [u8; 32] = [0xFF; 32];

/// Check if an allowance amount represents "unlimited" (> 2^128).
pub fn is_unlimited_allowance(amount: &[u8; 32]) -> bool {
    // If any of the first 16 bytes is non-zero, it's > 2^128
    amount[..16].iter().any(|&b| b != 0)
}

// ---------------------------------------------------------------------------
// Token balance query (balanceOf)
// ---------------------------------------------------------------------------

/// Encode ERC-20 `balanceOf(address)` calldata.
///
/// Selector: 0x70a08231
pub fn encode_balance_of(account: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0x70, 0xa0, 0x82, 0x31]);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(account);
    data.extend_from_slice(&slot);
    data
}

// ---------------------------------------------------------------------------
// Cross-chain bridges
// ---------------------------------------------------------------------------

/// Supported bridge protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeProtocol {
    /// Native Ethereum L2 bridge (Arbitrum/Optimism canonical)
    NativeL2,
    /// Wormhole (multi-chain)
    Wormhole,
    /// LayerZero (multi-chain)
    LayerZero,
    /// Across Protocol (fast L2 bridges)
    Across,
    /// Stargate (StableSwap bridges)
    Stargate,
}

/// A cross-chain bridge route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeRoute {
    pub protocol: BridgeProtocol,
    pub source_chain: Chain,
    pub dest_chain_id: u64,
    pub token: String,
    pub estimated_time_secs: u32,
    pub fee_percent: Decimal,
    pub min_amount: Decimal,
    pub max_amount: Option<Decimal>,
}

/// Known bridge routes.
pub fn available_bridge_routes() -> Vec<BridgeRoute> {
    use rust_decimal_macros::dec;
    vec![
        // ETH → Arbitrum
        BridgeRoute {
            protocol: BridgeProtocol::NativeL2,
            source_chain: Chain::Ethereum, dest_chain_id: 42161,
            token: "ETH".into(), estimated_time_secs: 600, fee_percent: dec!(0),
            min_amount: dec!(0.001), max_amount: None,
        },
        BridgeRoute {
            protocol: BridgeProtocol::Across,
            source_chain: Chain::Ethereum, dest_chain_id: 42161,
            token: "USDC".into(), estimated_time_secs: 60, fee_percent: dec!(0.05),
            min_amount: dec!(10), max_amount: Some(dec!(1000000)),
        },
        // ETH → Optimism
        BridgeRoute {
            protocol: BridgeProtocol::NativeL2,
            source_chain: Chain::Ethereum, dest_chain_id: 10,
            token: "ETH".into(), estimated_time_secs: 600, fee_percent: dec!(0),
            min_amount: dec!(0.001), max_amount: None,
        },
        // ETH → Base
        BridgeRoute {
            protocol: BridgeProtocol::NativeL2,
            source_chain: Chain::Ethereum, dest_chain_id: 8453,
            token: "ETH".into(), estimated_time_secs: 600, fee_percent: dec!(0),
            min_amount: dec!(0.001), max_amount: None,
        },
        // Multi-chain USDC via Stargate
        BridgeRoute {
            protocol: BridgeProtocol::Stargate,
            source_chain: Chain::Ethereum, dest_chain_id: 137,
            token: "USDC".into(), estimated_time_secs: 120, fee_percent: dec!(0.06),
            min_amount: dec!(1), max_amount: Some(dec!(5000000)),
        },
        // SOL <-> ETH via Wormhole
        BridgeRoute {
            protocol: BridgeProtocol::Wormhole,
            source_chain: Chain::Solana, dest_chain_id: 1,
            token: "USDC".into(), estimated_time_secs: 900, fee_percent: dec!(0.1),
            min_amount: dec!(10), max_amount: Some(dec!(500000)),
        },
    ]
}

/// Find bridge routes for a given source chain and token.
pub fn find_bridge_routes(source: Chain, token: &str) -> Vec<BridgeRoute> {
    available_bridge_routes().into_iter()
        .filter(|r| r.source_chain == source && r.token.eq_ignore_ascii_case(token))
        .collect()
}

// ---------------------------------------------------------------------------
// Curve pool calldata
// ---------------------------------------------------------------------------

/// Encode Curve `exchange(int128,int128,uint256,uint256)` calldata.
///
/// Selector: 0x3df02124
pub fn encode_curve_exchange(i: i128, j: i128, dx: &[u8; 32], min_dy: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(132);
    data.extend_from_slice(&[0x3d, 0xf0, 0x21, 0x24]);

    // i as int128 in 32 bytes
    let mut slot = [0u8; 32];
    slot[16..].copy_from_slice(&(i as u128).to_be_bytes());
    data.extend_from_slice(&slot);

    // j as int128
    slot = [0u8; 32];
    slot[16..].copy_from_slice(&(j as u128).to_be_bytes());
    data.extend_from_slice(&slot);

    // dx
    data.extend_from_slice(dx);
    // min_dy
    data.extend_from_slice(min_dy);

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniswap_v3_swap_calldata() {
        let params = UniswapV3SwapParams {
            token_in: [0xAA; 20],
            token_out: [0xBB; 20],
            fee: 3000,
            recipient: [0xCC; 20],
            amount_in: [0u8; 32],
            amount_out_min: [0u8; 32],
            sqrt_price_limit: [0u8; 32],
        };
        let data = encode_uniswap_v3_swap(&params);
        assert_eq!(&data[..4], &[0x41, 0x4b, 0xf3, 0x89]);
        assert_eq!(data.len(), 228);
        // token_in at offset 16..36
        assert_eq!(&data[16..36], &[0xAA; 20]);
        // fee at offset 97..100 (last 3 bytes of slot)
        assert_eq!(data[97], 0x00);
        assert_eq!(data[98], 0x0B); // 3000 = 0x0BB8
        assert_eq!(data[99], 0xB8);
    }

    #[test]
    fn erc20_approve_calldata() {
        let spender = [0xDD; 20];
        let amount = MAX_UINT256;
        let data = encode_erc20_approve(&spender, &amount);
        assert_eq!(&data[..4], &[0x09, 0x5e, 0xa7, 0xb3]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn erc20_allowance_calldata() {
        let owner = [0xAA; 20];
        let spender = [0xBB; 20];
        let data = encode_erc20_allowance(&owner, &spender);
        assert_eq!(&data[..4], &[0xdd, 0x62, 0xed, 0x3e]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn balance_of_calldata() {
        let account = [0x42; 20];
        let data = encode_balance_of(&account);
        assert_eq!(&data[..4], &[0x70, 0xa0, 0x82, 0x31]);
        assert_eq!(data.len(), 36);
        assert_eq!(&data[16..36], &[0x42; 20]);
    }

    #[test]
    fn unlimited_allowance_detection() {
        assert!(is_unlimited_allowance(&MAX_UINT256));
        let mut small = [0u8; 32];
        small[31] = 0xFF;
        assert!(!is_unlimited_allowance(&small));
    }

    #[test]
    fn curve_exchange_calldata() {
        let dx = [0u8; 32];
        let min_dy = [0u8; 32];
        let data = encode_curve_exchange(0, 1, &dx, &min_dy);
        assert_eq!(&data[..4], &[0x3d, 0xf0, 0x21, 0x24]);
        assert_eq!(data.len(), 132);
    }

    #[test]
    fn bridge_routes_eth_to_arb() {
        let routes = find_bridge_routes(Chain::Ethereum, "ETH");
        assert!(routes.len() >= 3); // native to arb/op/base
    }

    #[test]
    fn bridge_routes_usdc() {
        let routes = find_bridge_routes(Chain::Ethereum, "USDC");
        assert!(!routes.is_empty());
    }

    #[test]
    fn bridge_routes_sol_wormhole() {
        let routes = find_bridge_routes(Chain::Solana, "USDC");
        assert!(!routes.is_empty());
        assert_eq!(routes[0].protocol, BridgeProtocol::Wormhole);
    }

    #[test]
    fn bridge_protocol_serialization() {
        assert_eq!(serde_json::to_string(&BridgeProtocol::Wormhole).unwrap(), r#""wormhole""#);
        assert_eq!(serde_json::to_string(&BridgeProtocol::LayerZero).unwrap(), r#""layer_zero""#);
    }

    #[test]
    fn all_bridge_routes_have_fees() {
        for route in available_bridge_routes() {
            assert!(route.fee_percent >= Decimal::ZERO);
            assert!(route.estimated_time_secs > 0);
        }
    }

    #[test]
    fn max_uint256_all_ff() {
        assert!(MAX_UINT256.iter().all(|&b| b == 0xFF));
    }
}
