//! Composite policy rules (AND/OR/NOT logic).
//!
//! Allows combining multiple conditions with boolean logic.

use custody_common::{Asset, Chain};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::rbac::Role;
use crate::rule::{DestinationType, PolicyCondition};

/// A composite condition that combines sub-conditions with boolean logic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositeCondition {
    /// Single condition (leaf node).
    Simple(PolicyCondition),
    /// All sub-conditions must match.
    And(Vec<CompositeCondition>),
    /// At least one sub-condition must match.
    Or(Vec<CompositeCondition>),
    /// Invert the sub-condition.
    Not(Box<CompositeCondition>),
}

impl CompositeCondition {
    /// Evaluate this composite condition.
    pub fn matches(
        &self,
        chain: Chain,
        asset: &Asset,
        amount_usd: Decimal,
        dest_type: &DestinationType,
        role: &Role,
    ) -> bool {
        match self {
            CompositeCondition::Simple(cond) => {
                cond.matches(chain, asset, amount_usd, dest_type, role)
            }
            CompositeCondition::And(subs) => {
                subs.iter().all(|s| s.matches(chain, asset, amount_usd, dest_type, role))
            }
            CompositeCondition::Or(subs) => {
                subs.iter().any(|s| s.matches(chain, asset, amount_usd, dest_type, role))
            }
            CompositeCondition::Not(sub) => {
                !sub.matches(chain, asset, amount_usd, dest_type, role)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn eth_context() -> (Chain, Asset, Decimal, DestinationType, Role) {
        (Chain::Ethereum, Asset::new("ETH"), dec!(5000), DestinationType::Whitelisted, Role::Editor)
    }

    #[test]
    fn simple_condition() {
        let cond = CompositeCondition::Simple(PolicyCondition::default());
        let (chain, asset, amount, dest, role) = eth_context();
        assert!(cond.matches(chain, &asset, amount, &dest, &role));
    }

    #[test]
    fn and_all_must_match() {
        let cond = CompositeCondition::And(vec![
            CompositeCondition::Simple(PolicyCondition {
                chain: Some(Chain::Ethereum),
                ..Default::default()
            }),
            CompositeCondition::Simple(PolicyCondition {
                min_amount_usd: Some(dec!(1000)),
                ..Default::default()
            }),
        ]);

        let (chain, asset, amount, dest, role) = eth_context();
        assert!(cond.matches(chain, &asset, amount, &dest, &role)); // ETH + 5000 > 1000

        // Bitcoin doesn't match the chain condition
        assert!(!cond.matches(Chain::Bitcoin, &asset, amount, &dest, &role));
    }

    #[test]
    fn or_any_can_match() {
        let cond = CompositeCondition::Or(vec![
            CompositeCondition::Simple(PolicyCondition {
                chain: Some(Chain::Bitcoin),
                ..Default::default()
            }),
            CompositeCondition::Simple(PolicyCondition {
                chain: Some(Chain::Ethereum),
                ..Default::default()
            }),
        ]);

        let (_, asset, amount, dest, role) = eth_context();
        assert!(cond.matches(Chain::Ethereum, &asset, amount, &dest, &role));
        assert!(cond.matches(Chain::Bitcoin, &asset, amount, &dest, &role));
        assert!(!cond.matches(Chain::Solana, &asset, amount, &dest, &role));
    }

    #[test]
    fn not_inverts() {
        let cond = CompositeCondition::Not(Box::new(
            CompositeCondition::Simple(PolicyCondition {
                chain: Some(Chain::Bitcoin),
                ..Default::default()
            }),
        ));

        let (_, asset, amount, dest, role) = eth_context();
        assert!(cond.matches(Chain::Ethereum, &asset, amount, &dest, &role)); // NOT bitcoin → true for ETH
        assert!(!cond.matches(Chain::Bitcoin, &asset, amount, &dest, &role)); // NOT bitcoin → false for BTC
    }

    #[test]
    fn nested_composite() {
        // (chain=ETH AND amount>1000) OR (chain=BTC)
        let cond = CompositeCondition::Or(vec![
            CompositeCondition::And(vec![
                CompositeCondition::Simple(PolicyCondition { chain: Some(Chain::Ethereum), ..Default::default() }),
                CompositeCondition::Simple(PolicyCondition { min_amount_usd: Some(dec!(1000)), ..Default::default() }),
            ]),
            CompositeCondition::Simple(PolicyCondition { chain: Some(Chain::Bitcoin), ..Default::default() }),
        ]);

        let (_, asset, _, dest, role) = eth_context();
        assert!(cond.matches(Chain::Ethereum, &asset, dec!(5000), &dest, &role)); // ETH + >1000 ✓
        assert!(!cond.matches(Chain::Ethereum, &asset, dec!(500), &dest, &role)); // ETH + <1000 ✗
        assert!(cond.matches(Chain::Bitcoin, &asset, dec!(1), &dest, &role)); // BTC always ✓
    }

    #[test]
    fn serialization_roundtrip() {
        let cond = CompositeCondition::And(vec![
            CompositeCondition::Simple(PolicyCondition::default()),
            CompositeCondition::Not(Box::new(
                CompositeCondition::Simple(PolicyCondition { chain: Some(Chain::Tron), ..Default::default() }),
            )),
        ]);
        let json = serde_json::to_string(&cond).unwrap();
        let parsed: CompositeCondition = serde_json::from_str(&json).unwrap();
        // Verify it still works after roundtrip
        let (chain, asset, amount, dest, role) = eth_context();
        assert!(parsed.matches(chain, &asset, amount, &dest, &role));
    }
}
