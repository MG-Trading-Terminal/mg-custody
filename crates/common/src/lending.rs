//! Lending protocol calldata — Aave V3, Compound V3, Morpho.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Aave V3
// ---------------------------------------------------------------------------

pub const AAVE_V3_POOL: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";
pub const AAVE_V3_POOL_ARBITRUM: &str = "0x794a61358D6845594F94dc1DB02A252b5b4814aD";

/// Encode `supply(address,uint256,address,uint16)`.
/// Selector: 0x617ba037
pub fn encode_aave_supply(asset: &[u8; 20], amount: &[u8; 32], on_behalf_of: &[u8; 20], referral: u16) -> Vec<u8> {
    let mut data = Vec::with_capacity(132);
    data.extend_from_slice(&[0x61, 0x7b, 0xa0, 0x37]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    slot = [0u8; 32]; slot[12..].copy_from_slice(on_behalf_of); data.extend_from_slice(&slot);
    slot = [0u8; 32]; slot[30..32].copy_from_slice(&referral.to_be_bytes()); data.extend_from_slice(&slot);
    data
}

/// Encode `borrow(address,uint256,uint256,uint16,address)`.
/// Selector: 0xa415bcad
pub fn encode_aave_borrow(asset: &[u8; 20], amount: &[u8; 32], rate_mode: u8, referral: u16, on_behalf_of: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(164);
    data.extend_from_slice(&[0xa4, 0x15, 0xbc, 0xad]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    slot = [0u8; 32]; slot[31] = rate_mode; data.extend_from_slice(&slot); // 1=stable, 2=variable
    slot = [0u8; 32]; slot[30..32].copy_from_slice(&referral.to_be_bytes()); data.extend_from_slice(&slot);
    slot = [0u8; 32]; slot[12..].copy_from_slice(on_behalf_of); data.extend_from_slice(&slot);
    data
}

/// Encode `repay(address,uint256,uint256,address)`.
/// Selector: 0x573ade81
pub fn encode_aave_repay(asset: &[u8; 20], amount: &[u8; 32], rate_mode: u8, on_behalf_of: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(132);
    data.extend_from_slice(&[0x57, 0x3a, 0xde, 0x81]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    slot = [0u8; 32]; slot[31] = rate_mode; data.extend_from_slice(&slot);
    slot = [0u8; 32]; slot[12..].copy_from_slice(on_behalf_of); data.extend_from_slice(&slot);
    data
}

/// Encode `withdraw(address,uint256,address)`.
/// Selector: 0x69328dec
pub fn encode_aave_withdraw(asset: &[u8; 20], amount: &[u8; 32], to: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100);
    data.extend_from_slice(&[0x69, 0x32, 0x8d, 0xec]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    slot = [0u8; 32]; slot[12..].copy_from_slice(to); data.extend_from_slice(&slot);
    data
}

// ---------------------------------------------------------------------------
// Compound V3 (Comet)
// ---------------------------------------------------------------------------

pub const COMPOUND_V3_USDC: &str = "0xc3d688B66703497DAA19211EEdff47f25384cdc3";

/// Encode Compound V3 `supply(address,uint256)`.
/// Selector: 0xf2b9fdb8
pub fn encode_compound_supply(asset: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&[0xf2, 0xb9, 0xfd, 0xb8]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    data
}

/// Encode Compound V3 `withdraw(address,uint256)`.
/// Selector: 0xf3fef3a3
pub fn encode_compound_withdraw(asset: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&[0xf3, 0xfe, 0xf3, 0xa3]);
    let mut slot = [0u8; 32]; slot[12..].copy_from_slice(asset); data.extend_from_slice(&slot);
    data.extend_from_slice(amount);
    data
}

// ---------------------------------------------------------------------------
// Lending position types
// ---------------------------------------------------------------------------

/// A lending position (supply or borrow).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingPosition {
    pub protocol: LendingProtocol,
    pub chain: crate::Chain,
    pub asset: String,
    pub position_type: PositionType,
    pub amount: Decimal,
    pub apy: Decimal,
    pub collateral_factor: Option<Decimal>,
    pub liquidation_threshold: Option<Decimal>,
    pub health_factor: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LendingProtocol {
    AaveV3,
    CompoundV3,
    Morpho,
    Spark,
    Venus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionType { Supply, Borrow }

/// Known lending market APYs (approximate, changes constantly).
pub fn lending_markets() -> Vec<LendingMarket> {
    use rust_decimal_macros::dec;
    vec![
        LendingMarket { protocol: LendingProtocol::AaveV3, asset: "USDC".into(), supply_apy: dec!(4.5), borrow_apy: dec!(5.8), ltv: dec!(0.77) },
        LendingMarket { protocol: LendingProtocol::AaveV3, asset: "USDT".into(), supply_apy: dec!(4.2), borrow_apy: dec!(5.5), ltv: dec!(0.75) },
        LendingMarket { protocol: LendingProtocol::AaveV3, asset: "ETH".into(), supply_apy: dec!(2.1), borrow_apy: dec!(3.2), ltv: dec!(0.80) },
        LendingMarket { protocol: LendingProtocol::AaveV3, asset: "WBTC".into(), supply_apy: dec!(0.5), borrow_apy: dec!(1.8), ltv: dec!(0.73) },
        LendingMarket { protocol: LendingProtocol::CompoundV3, asset: "USDC".into(), supply_apy: dec!(5.0), borrow_apy: dec!(6.2), ltv: dec!(0.80) },
        LendingMarket { protocol: LendingProtocol::Morpho, asset: "USDC".into(), supply_apy: dec!(6.0), borrow_apy: dec!(7.0), ltv: dec!(0.86) },
        LendingMarket { protocol: LendingProtocol::Spark, asset: "DAI".into(), supply_apy: dec!(5.0), borrow_apy: dec!(5.5), ltv: dec!(0.77) },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LendingMarket {
    pub protocol: LendingProtocol,
    pub asset: String,
    pub supply_apy: Decimal,
    pub borrow_apy: Decimal,
    pub ltv: Decimal,
}

/// Calculate health factor: (collateral * liquidation_threshold) / debt.
pub fn health_factor(collateral_usd: Decimal, liquidation_threshold: Decimal, debt_usd: Decimal) -> Option<Decimal> {
    if debt_usd == Decimal::ZERO { return None; }
    Some(collateral_usd * liquidation_threshold / debt_usd)
}

/// Check if a position is at risk of liquidation (health factor < 1.0).
pub fn is_liquidatable(hf: Decimal) -> bool {
    hf < Decimal::ONE
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn aave_supply_calldata() {
        let data = encode_aave_supply(&[0xAA; 20], &[0u8; 32], &[0xBB; 20], 0);
        assert_eq!(&data[..4], &[0x61, 0x7b, 0xa0, 0x37]);
        assert_eq!(data.len(), 132);
    }

    #[test]
    fn aave_borrow_calldata() {
        let data = encode_aave_borrow(&[0xAA; 20], &[0u8; 32], 2, 0, &[0xBB; 20]);
        assert_eq!(&data[..4], &[0xa4, 0x15, 0xbc, 0xad]);
        assert_eq!(data.len(), 164);
    }

    #[test]
    fn aave_repay_calldata() {
        let data = encode_aave_repay(&[0xAA; 20], &[0u8; 32], 2, &[0xBB; 20]);
        assert_eq!(&data[..4], &[0x57, 0x3a, 0xde, 0x81]);
        assert_eq!(data.len(), 132);
    }

    #[test]
    fn aave_withdraw_calldata() {
        let data = encode_aave_withdraw(&[0xAA; 20], &[0u8; 32], &[0xBB; 20]);
        assert_eq!(&data[..4], &[0x69, 0x32, 0x8d, 0xec]);
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn compound_supply_calldata() {
        let data = encode_compound_supply(&[0xAA; 20], &[0u8; 32]);
        assert_eq!(&data[..4], &[0xf2, 0xb9, 0xfd, 0xb8]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn compound_withdraw_calldata() {
        let data = encode_compound_withdraw(&[0xAA; 20], &[0u8; 32]);
        assert_eq!(&data[..4], &[0xf3, 0xfe, 0xf3, 0xa3]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn lending_markets_count() {
        assert!(lending_markets().len() >= 7);
    }

    #[test]
    fn health_factor_calculation() {
        let hf = health_factor(dec!(10000), dec!(0.85), dec!(5000)).unwrap();
        assert_eq!(hf, dec!(1.7));
    }

    #[test]
    fn health_factor_no_debt() {
        assert!(health_factor(dec!(10000), dec!(0.85), Decimal::ZERO).is_none());
    }

    #[test]
    fn liquidation_check() {
        assert!(!is_liquidatable(dec!(1.5)));
        assert!(is_liquidatable(dec!(0.9)));
        assert!(is_liquidatable(dec!(0.99)));
    }

    #[test]
    fn lending_protocol_serialization() {
        assert_eq!(serde_json::to_string(&LendingProtocol::AaveV3).unwrap(), r#""aave_v3""#);
        assert_eq!(serde_json::to_string(&LendingProtocol::CompoundV3).unwrap(), r#""compound_v3""#);
    }
}
