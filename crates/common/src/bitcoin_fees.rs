//! Bitcoin fee estimation — mempool analysis, RBF, CPFP.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Mempool fee rate buckets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolFeeRates {
    pub fastest: Decimal,       // next block
    pub half_hour: Decimal,     // ~3 blocks
    pub hour: Decimal,          // ~6 blocks
    pub economy: Decimal,       // ~12+ blocks
    pub minimum: Decimal,       // 1 sat/vB
}

impl MempoolFeeRates {
    /// Default fee rates (in sat/vB).
    pub fn default_rates() -> Self {
        use rust_decimal_macros::dec;
        Self {
            fastest: dec!(50),
            half_hour: dec!(30),
            hour: dec!(15),
            economy: dec!(5),
            minimum: dec!(1),
        }
    }
}

/// RBF (Replace-By-Fee) parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbfParams {
    pub original_fee_rate: Decimal,
    pub new_fee_rate: Decimal,
    pub original_tx_size_vb: u32,
    pub bump_fee: Decimal,
    pub total_new_fee: Decimal,
}

/// Calculate RBF fee bump (BIP-125 minimum: original fee + relay fee).
pub fn calculate_rbf_bump(
    original_fee_rate: Decimal,
    original_size_vb: u32,
    new_fee_rate: Decimal,
) -> RbfParams {
    let original_fee = original_fee_rate * Decimal::new(original_size_vb as i64, 0);
    let new_fee = new_fee_rate * Decimal::new(original_size_vb as i64, 0);
    let bump = new_fee - original_fee;

    RbfParams {
        original_fee_rate,
        new_fee_rate,
        original_tx_size_vb: original_size_vb,
        bump_fee: bump,
        total_new_fee: new_fee,
    }
}

/// CPFP (Child-Pays-For-Parent) effective fee rate calculation.
pub fn cpfp_effective_rate(
    parent_size_vb: u32,
    parent_fee: Decimal,
    child_size_vb: u32,
    child_fee: Decimal,
) -> Decimal {
    let total_size = Decimal::new((parent_size_vb + child_size_vb) as i64, 0);
    let total_fee = parent_fee + child_fee;
    if total_size > Decimal::ZERO { total_fee / total_size } else { Decimal::ZERO }
}

/// Required child fee for CPFP to hit target rate.
pub fn cpfp_required_child_fee(
    parent_size_vb: u32,
    parent_fee: Decimal,
    child_size_vb: u32,
    target_rate: Decimal,
) -> Decimal {
    let total_size = Decimal::new((parent_size_vb + child_size_vb) as i64, 0);
    let required_total = target_rate * total_size;
    (required_total - parent_fee).max(Decimal::ZERO)
}

/// Estimate confirmation time based on fee rate.
pub fn estimate_confirmation_blocks(fee_rate: Decimal, mempool: &MempoolFeeRates) -> u32 {
    if fee_rate >= mempool.fastest { 1 }
    else if fee_rate >= mempool.half_hour { 3 }
    else if fee_rate >= mempool.hour { 6 }
    else if fee_rate >= mempool.economy { 12 }
    else { 24 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn default_fee_rates() {
        let rates = MempoolFeeRates::default_rates();
        assert!(rates.fastest > rates.half_hour);
        assert!(rates.half_hour > rates.hour);
        assert!(rates.hour > rates.economy);
        assert!(rates.economy > rates.minimum);
    }

    #[test]
    fn rbf_bump_calculation() {
        let rbf = calculate_rbf_bump(dec!(10), 200, dec!(20));
        assert_eq!(rbf.original_fee_rate, dec!(10));
        assert_eq!(rbf.new_fee_rate, dec!(20));
        assert_eq!(rbf.bump_fee, dec!(2000)); // (20-10) * 200
        assert_eq!(rbf.total_new_fee, dec!(4000)); // 20 * 200
    }

    #[test]
    fn cpfp_effective_rate_calc() {
        let rate = cpfp_effective_rate(200, dec!(400), 150, dec!(3000));
        // (400 + 3000) / (200 + 150) = 3400/350 ≈ 9.71
        assert!(rate > dec!(9));
        assert!(rate < dec!(10));
    }

    #[test]
    fn cpfp_required_fee() {
        let child_fee = cpfp_required_child_fee(200, dec!(400), 150, dec!(20));
        // target total = 20 * 350 = 7000, minus parent fee 400 = 6600
        assert_eq!(child_fee, dec!(6600));
    }

    #[test]
    fn cpfp_already_sufficient() {
        let child_fee = cpfp_required_child_fee(200, dec!(5000), 150, dec!(10));
        // target total = 10 * 350 = 3500, parent already has 5000 > 3500
        assert_eq!(child_fee, Decimal::ZERO); // no extra needed
    }

    #[test]
    fn confirmation_fastest() {
        let mempool = MempoolFeeRates::default_rates();
        assert_eq!(estimate_confirmation_blocks(dec!(50), &mempool), 1);
        assert_eq!(estimate_confirmation_blocks(dec!(100), &mempool), 1);
    }

    #[test]
    fn confirmation_half_hour() {
        let mempool = MempoolFeeRates::default_rates();
        assert_eq!(estimate_confirmation_blocks(dec!(30), &mempool), 3);
    }

    #[test]
    fn confirmation_economy() {
        let mempool = MempoolFeeRates::default_rates();
        assert_eq!(estimate_confirmation_blocks(dec!(5), &mempool), 12);
    }

    #[test]
    fn confirmation_slow() {
        let mempool = MempoolFeeRates::default_rates();
        assert_eq!(estimate_confirmation_blocks(dec!(1), &mempool), 24);
    }

    #[test]
    fn fee_rates_serialization() {
        let rates = MempoolFeeRates::default_rates();
        let json = serde_json::to_string(&rates).unwrap();
        assert!(json.contains("fastest"));
        assert!(json.contains("economy"));
    }
}
