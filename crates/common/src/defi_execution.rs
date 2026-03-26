//! DeFi execution — swap routing, DEX aggregation, slippage protection.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// A DEX swap route — series of hops through liquidity pools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRoute {
    pub id: Uuid,
    pub chain: Chain,
    pub from_token: String,
    pub to_token: String,
    pub amount_in: Decimal,
    pub expected_out: Decimal,
    pub minimum_out: Decimal,
    pub slippage_bps: u32,
    pub price_impact_bps: u32,
    pub hops: Vec<SwapHop>,
    pub aggregator: String,
    pub expires_at: DateTime<Utc>,
}

/// A single hop in a swap route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapHop {
    pub dex: String,
    pub pool_address: String,
    pub token_in: String,
    pub token_out: String,
    pub fee_bps: u32,
}

/// Validate slippage tolerance (max 2% = 200 bps for institutional).
pub fn validate_slippage(slippage_bps: u32) -> Result<(), String> {
    if slippage_bps == 0 {
        return Err("Slippage must be greater than 0".into());
    }
    if slippage_bps > 200 {
        return Err(format!("Slippage {slippage_bps} bps exceeds max 200 bps (2%)"));
    }
    Ok(())
}

/// Calculate minimum output with slippage protection.
pub fn minimum_output(expected: Decimal, slippage_bps: u32) -> Decimal {
    let slippage = Decimal::new(slippage_bps as i64, 4); // bps to decimal
    expected * (Decimal::ONE - slippage)
}

/// Check if price impact is acceptable (< 1% for large trades).
pub fn check_price_impact(impact_bps: u32, threshold_bps: u32) -> Result<(), String> {
    if impact_bps > threshold_bps {
        return Err(format!(
            "Price impact {impact_bps} bps exceeds threshold {threshold_bps} bps"
        ));
    }
    Ok(())
}

/// Known DEX protocols per chain.
pub fn supported_dexes(chain: Chain) -> Vec<DexInfo> {
    match chain {
        Chain::Ethereum => vec![
            DexInfo { name: "Uniswap V3".into(), router: "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45".into(), fee_tiers: vec![100, 500, 3000, 10000] },
            DexInfo { name: "SushiSwap".into(), router: "0xd9e1cE17f2641f24aE83637ab66a2cca9C378B9F".into(), fee_tiers: vec![3000] },
            DexInfo { name: "Curve".into(), router: "0x99a58482BD75cbab83b27EC03CA68fF489b5788f".into(), fee_tiers: vec![4] },
        ],
        Chain::Solana => vec![
            DexInfo { name: "Jupiter".into(), router: "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".into(), fee_tiers: vec![] },
            DexInfo { name: "Raydium".into(), router: "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".into(), fee_tiers: vec![25] },
        ],
        Chain::Tron => vec![
            DexInfo { name: "SunSwap".into(), router: "TKzxdSv2FZKQrEqkKVgp5DcwEXBEKMg2Ax".into(), fee_tiers: vec![30] },
        ],
        Chain::Bitcoin => vec![], // no DEXes on Bitcoin L1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexInfo {
    pub name: String,
    pub router: String,
    pub fee_tiers: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn validate_slippage_ok() {
        assert!(validate_slippage(50).is_ok());
        assert!(validate_slippage(200).is_ok());
    }

    #[test]
    fn validate_slippage_too_high() {
        assert!(validate_slippage(201).is_err());
        assert!(validate_slippage(500).is_err());
    }

    #[test]
    fn validate_slippage_zero() {
        assert!(validate_slippage(0).is_err());
    }

    #[test]
    fn minimum_output_calculation() {
        let min = minimum_output(dec!(1000), 50); // 0.5%
        assert_eq!(min, dec!(995.0));
    }

    #[test]
    fn minimum_output_2_percent() {
        let min = minimum_output(dec!(1000), 200); // 2%
        assert_eq!(min, dec!(980.0));
    }

    #[test]
    fn price_impact_ok() {
        assert!(check_price_impact(50, 100).is_ok());
    }

    #[test]
    fn price_impact_too_high() {
        assert!(check_price_impact(150, 100).is_err());
    }

    #[test]
    fn ethereum_has_dexes() {
        let dexes = supported_dexes(Chain::Ethereum);
        assert!(dexes.len() >= 3);
        assert!(dexes.iter().any(|d| d.name.contains("Uniswap")));
    }

    #[test]
    fn bitcoin_no_dexes() {
        assert!(supported_dexes(Chain::Bitcoin).is_empty());
    }

    #[test]
    fn swap_route_serialization() {
        let route = SwapRoute {
            id: Uuid::nil(), chain: Chain::Ethereum,
            from_token: "USDC".into(), to_token: "ETH".into(),
            amount_in: dec!(1000), expected_out: dec!(0.3),
            minimum_out: dec!(0.297), slippage_bps: 100,
            price_impact_bps: 5,
            hops: vec![SwapHop {
                dex: "Uniswap V3".into(),
                pool_address: "0xpool".into(),
                token_in: "USDC".into(), token_out: "ETH".into(),
                fee_bps: 500,
            }],
            aggregator: "1inch".into(),
            expires_at: Utc::now(),
        };
        let json = serde_json::to_string(&route).unwrap();
        assert!(json.contains("Uniswap"));
        assert!(json.contains("1inch"));
    }
}
