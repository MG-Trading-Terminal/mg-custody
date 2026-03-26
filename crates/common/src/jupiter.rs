//! Jupiter DEX aggregator (Solana) — swap instruction building, route quoting.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Jupiter program ID on Solana mainnet.
pub const JUPITER_V6_PROGRAM: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";

/// Jupiter swap route from the quote API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterRoute {
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: u64,
    pub out_amount: u64,
    pub other_amount_threshold: u64,
    pub swap_mode: SwapMode,
    pub slippage_bps: u32,
    pub price_impact_pct: Decimal,
    pub route_plan: Vec<JupiterRoutePlan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapMode {
    ExactIn,
    ExactOut,
}

/// A single step in a Jupiter route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterRoutePlan {
    pub swap_info: JupiterSwapInfo,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JupiterSwapInfo {
    pub amm_key: String,
    pub label: String,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: u64,
    pub out_amount: u64,
    pub fee_amount: u64,
    pub fee_mint: String,
}

/// Known Solana token mints.
pub mod mints {
    pub const SOL: &str = "So11111111111111111111111111111111111111112";
    pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    pub const USDT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
    pub const BONK: &str = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
    pub const JUP: &str = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN";
    pub const WIF: &str = "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm";
    pub const MSOL: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";
    pub const JITOSOL: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn";
    pub const PYTH: &str = "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3";
    pub const RAY: &str = "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R";
}

/// Build Jupiter swap instruction data.
///
/// Discriminator for Jupiter V6 route swap: first 8 bytes.
pub fn jupiter_swap_discriminator() -> [u8; 8] {
    // route instruction discriminator
    [0xe5, 0x17, 0xcb, 0x97, 0x7a, 0xe3, 0xad, 0x2a]
}

/// Build the shared accounts swap instruction data.
///
/// This is a simplified version — real Jupiter instructions use
/// the route data from the quote API response.
pub fn build_jupiter_swap_data(
    route: &JupiterRoute,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(64);

    // Discriminator
    data.extend_from_slice(&jupiter_swap_discriminator());

    // in_amount as u64 LE
    data.extend_from_slice(&route.in_amount.to_le_bytes());

    // out_amount (minimum) as u64 LE
    data.extend_from_slice(&route.other_amount_threshold.to_le_bytes());

    // slippage_bps as u16 LE
    data.extend_from_slice(&(route.slippage_bps as u16).to_le_bytes());

    // Number of route steps
    data.push(route.route_plan.len() as u8);

    data
}

/// Validate a Solana token mint address (base58, 32-44 chars).
pub fn validate_mint(mint: &str) -> bool {
    mint.len() >= 32 && mint.len() <= 44 &&
        mint.chars().all(|c| {
            c.is_ascii_alphanumeric() && c != '0' && c != 'O' && c != 'I' && c != 'l'
        })
}

/// Calculate minimum output for a given slippage.
pub fn calculate_min_output(expected_output: u64, slippage_bps: u32) -> u64 {
    let reduction = (expected_output as u128 * slippage_bps as u128) / 10_000;
    expected_output.saturating_sub(reduction as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn jupiter_discriminator_length() {
        assert_eq!(jupiter_swap_discriminator().len(), 8);
    }

    #[test]
    fn build_swap_data_basic() {
        let route = JupiterRoute {
            input_mint: mints::USDC.into(),
            output_mint: mints::SOL.into(),
            in_amount: 1_000_000, // 1 USDC
            out_amount: 6_500_000, // ~0.0065 SOL
            other_amount_threshold: 6_400_000,
            swap_mode: SwapMode::ExactIn,
            slippage_bps: 50,
            price_impact_pct: dec!(0.01),
            route_plan: vec![JupiterRoutePlan {
                swap_info: JupiterSwapInfo {
                    amm_key: "pool123".into(), label: "Raydium".into(),
                    input_mint: mints::USDC.into(), output_mint: mints::SOL.into(),
                    in_amount: 1_000_000, out_amount: 6_500_000,
                    fee_amount: 250, fee_mint: mints::USDC.into(),
                },
                percent: 100,
            }],
        };
        let data = build_jupiter_swap_data(&route);
        assert!(data.len() >= 19); // 8 + 8 + 8 + 2 + 1
        assert_eq!(&data[..8], &jupiter_swap_discriminator());
    }

    #[test]
    fn validate_known_mints() {
        assert!(validate_mint(mints::SOL));
        assert!(validate_mint(mints::USDC));
        assert!(validate_mint(mints::BONK));
        assert!(validate_mint(mints::JUP));
    }

    #[test]
    fn validate_invalid_mints() {
        assert!(!validate_mint("short"));
        assert!(!validate_mint("")); // empty
    }

    #[test]
    fn min_output_calculation() {
        assert_eq!(calculate_min_output(1_000_000, 50), 995_000); // 0.5%
        assert_eq!(calculate_min_output(1_000_000, 100), 990_000); // 1%
        assert_eq!(calculate_min_output(1_000_000, 0), 1_000_000); // no slippage
    }

    #[test]
    fn swap_mode_serialization() {
        assert_eq!(serde_json::to_string(&SwapMode::ExactIn).unwrap(), r#""exact_in""#);
        assert_eq!(serde_json::to_string(&SwapMode::ExactOut).unwrap(), r#""exact_out""#);
    }

    #[test]
    fn route_plan_multi_hop() {
        let route = JupiterRoute {
            input_mint: mints::BONK.into(), output_mint: mints::USDC.into(),
            in_amount: 1_000_000_000, out_amount: 15_000,
            other_amount_threshold: 14_850,
            swap_mode: SwapMode::ExactIn, slippage_bps: 100,
            price_impact_pct: dec!(0.5),
            route_plan: vec![
                JupiterRoutePlan {
                    swap_info: JupiterSwapInfo {
                        amm_key: "pool_a".into(), label: "Orca".into(),
                        input_mint: mints::BONK.into(), output_mint: mints::SOL.into(),
                        in_amount: 1_000_000_000, out_amount: 100_000,
                        fee_amount: 1000, fee_mint: mints::BONK.into(),
                    },
                    percent: 100,
                },
                JupiterRoutePlan {
                    swap_info: JupiterSwapInfo {
                        amm_key: "pool_b".into(), label: "Raydium".into(),
                        input_mint: mints::SOL.into(), output_mint: mints::USDC.into(),
                        in_amount: 100_000, out_amount: 15_000,
                        fee_amount: 50, fee_mint: mints::SOL.into(),
                    },
                    percent: 100,
                },
            ],
        };
        assert_eq!(route.route_plan.len(), 2);
        let data = build_jupiter_swap_data(&route);
        assert_eq!(data[data.len() - 1], 2); // 2 route steps
    }
}
