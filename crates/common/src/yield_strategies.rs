//! Yield strategies — Yearn V3, Convex, Pendle, auto-compound.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

/// A yield-bearing strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldStrategy {
    pub name: String,
    pub protocol: YieldProtocol,
    pub chain: Chain,
    pub underlying_asset: String,
    pub vault_address: String,
    pub apy: Decimal,
    pub tvl_usd: Option<Decimal>,
    pub risk_level: YieldRisk,
    pub auto_compound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldProtocol {
    YearnV3,
    Convex,
    Pendle,
    Eigenlayer,
    EtherFi,
    Ethena,
    MakerDsr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldRisk { Low, Medium, High }

/// Known yield strategies.
pub fn known_strategies() -> Vec<YieldStrategy> {
    use rust_decimal_macros::dec;
    vec![
        YieldStrategy {
            name: "Maker DSR (sDAI)".into(), protocol: YieldProtocol::MakerDsr,
            chain: Chain::Ethereum, underlying_asset: "DAI".into(),
            vault_address: "0x83F20F44975D03b1b09e64809B757c47f942BEeA".into(),
            apy: dec!(5.0), tvl_usd: Some(dec!(2_000_000_000)),
            risk_level: YieldRisk::Low, auto_compound: true,
        },
        YieldStrategy {
            name: "Yearn USDC".into(), protocol: YieldProtocol::YearnV3,
            chain: Chain::Ethereum, underlying_asset: "USDC".into(),
            vault_address: "0x7a364e8770418566e3eb2001A96116E6138Eb32F".into(),
            apy: dec!(4.5), tvl_usd: Some(dec!(500_000_000)),
            risk_level: YieldRisk::Low, auto_compound: true,
        },
        YieldStrategy {
            name: "Convex stETH/ETH".into(), protocol: YieldProtocol::Convex,
            chain: Chain::Ethereum, underlying_asset: "stETH".into(),
            vault_address: "0xF403C135812408BFbE8713b5A23a04b3D48AAE31".into(),
            apy: dec!(5.5), tvl_usd: Some(dec!(1_000_000_000)),
            risk_level: YieldRisk::Medium, auto_compound: true,
        },
        YieldStrategy {
            name: "Ethena USDe".into(), protocol: YieldProtocol::Ethena,
            chain: Chain::Ethereum, underlying_asset: "USDe".into(),
            vault_address: "0x4c9EDD5852cd905f086C759E8383e09bff1E68B3".into(),
            apy: dec!(15.0), tvl_usd: Some(dec!(3_000_000_000)),
            risk_level: YieldRisk::High, auto_compound: false,
        },
        YieldStrategy {
            name: "EigenLayer restaking".into(), protocol: YieldProtocol::Eigenlayer,
            chain: Chain::Ethereum, underlying_asset: "stETH".into(),
            vault_address: "0x858646372CC42E1A627fcE94aa7A7033e7CF075A".into(),
            apy: dec!(4.0), tvl_usd: Some(dec!(15_000_000_000)),
            risk_level: YieldRisk::Medium, auto_compound: false,
        },
        YieldStrategy {
            name: "EtherFi eETH".into(), protocol: YieldProtocol::EtherFi,
            chain: Chain::Ethereum, underlying_asset: "ETH".into(),
            vault_address: "0x35fA164735182de50811E8e2E824cFb9B6118ac2".into(),
            apy: dec!(3.8), tvl_usd: Some(dec!(5_000_000_000)),
            risk_level: YieldRisk::Medium, auto_compound: true,
        },
    ]
}

/// Find strategies for a given asset.
pub fn strategies_for_asset(asset: &str) -> Vec<YieldStrategy> {
    known_strategies().into_iter()
        .filter(|s| s.underlying_asset.eq_ignore_ascii_case(asset))
        .collect()
}

/// Find low-risk strategies only.
pub fn low_risk_strategies() -> Vec<YieldStrategy> {
    known_strategies().into_iter()
        .filter(|s| s.risk_level == YieldRisk::Low)
        .collect()
}

/// Compare APYs across protocols for the same asset.
pub fn best_apy_for_asset(asset: &str) -> Option<YieldStrategy> {
    strategies_for_asset(asset).into_iter()
        .max_by(|a, b| a.apy.cmp(&b.apy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn known_strategies_count() {
        assert!(known_strategies().len() >= 6);
    }

    #[test]
    fn strategies_for_dai() {
        let strats = strategies_for_asset("DAI");
        assert!(!strats.is_empty());
        assert!(strats.iter().any(|s| s.protocol == YieldProtocol::MakerDsr));
    }

    #[test]
    fn low_risk_only() {
        let strats = low_risk_strategies();
        for s in &strats {
            assert_eq!(s.risk_level, YieldRisk::Low);
        }
    }

    #[test]
    fn best_apy() {
        let best = best_apy_for_asset("stETH").unwrap();
        assert!(best.apy > Decimal::ZERO);
    }

    #[test]
    fn yield_risk_ordering() {
        assert!(YieldRisk::Low < YieldRisk::Medium);
        assert!(YieldRisk::Medium < YieldRisk::High);
    }

    #[test]
    fn ethena_high_risk() {
        let strats = known_strategies();
        let ethena = strats.iter().find(|s| s.protocol == YieldProtocol::Ethena).unwrap();
        assert_eq!(ethena.risk_level, YieldRisk::High);
        assert!(ethena.apy > dec!(10)); // high APY = high risk
    }

    #[test]
    fn yield_protocol_serialization() {
        assert_eq!(serde_json::to_string(&YieldProtocol::Eigenlayer).unwrap(), r#""eigenlayer""#);
        assert_eq!(serde_json::to_string(&YieldProtocol::MakerDsr).unwrap(), r#""maker_dsr""#);
    }

    #[test]
    fn all_strategies_have_tvl() {
        for s in known_strategies() {
            assert!(s.tvl_usd.is_some(), "{} missing TVL", s.name);
        }
    }
}
