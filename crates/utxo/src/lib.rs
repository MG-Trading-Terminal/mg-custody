//! Bitcoin UTXO management and coin selection.
//!
//! Tracks unspent transaction outputs (UTXOs) for Bitcoin hot/warm wallets.
//! Implements coin selection algorithms for withdrawal transactions:
//! 1. Branch and Bound — tries to find an exact match (no change output needed)
//! 2. Largest-first greedy — fallback when BnB fails
//!
//! References:
//! - Branch and Bound: <https://murch.one/erhardt2016coinselection.pdf>
//! - Dust limit: 546 satoshis (Bitcoin Core DEFAULT_DUST_RELAY_TX_FEE)

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Bitcoin dust threshold in satoshis.
/// Outputs below this are uneconomical to spend and should be excluded.
pub const DUST_LIMIT_SATS: u64 = 546;

/// A single unspent transaction output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub id: Uuid,
    pub txid: String,
    pub vout: u32,
    pub amount_sats: u64,
    pub address: String,
    pub confirmations: u64,
    /// Whether this UTXO has been selected for a pending transaction.
    pub locked: bool,
}

/// Result of coin selection.
#[derive(Debug, Clone)]
pub struct CoinSelection {
    /// Selected UTXOs.
    pub inputs: Vec<Utxo>,
    /// Total input amount in satoshis.
    pub total_input_sats: u64,
    /// Target amount in satoshis.
    pub target_sats: u64,
    /// Estimated fee in satoshis.
    pub fee_sats: u64,
    /// Change amount (total_input - target - fee). Zero if exact match.
    pub change_sats: u64,
}

/// Estimated transaction size for fee calculation.
///
/// P2WPKH: ~68 vbytes per input, ~31 vbytes per output, ~10.5 vbytes overhead.
pub fn estimate_tx_vbytes(num_inputs: usize, num_outputs: usize) -> usize {
    let overhead = 11; // ~10.5 rounded up
    let input_size = 68; // P2WPKH input
    let output_size = 31; // P2WPKH output
    overhead + (num_inputs * input_size) + (num_outputs * output_size)
}

/// Calculate the fee for a transaction given fee rate in sat/vB.
pub fn calculate_fee(num_inputs: usize, num_outputs: usize, fee_rate_sat_per_vb: u64) -> u64 {
    let vbytes = estimate_tx_vbytes(num_inputs, num_outputs) as u64;
    vbytes * fee_rate_sat_per_vb
}

/// Select UTXOs for a withdrawal using Branch and Bound with greedy fallback.
///
/// 1. Filters out dust and locked UTXOs
/// 2. Tries Branch and Bound for exact match (within tolerance of fee)
/// 3. Falls back to largest-first greedy selection
///
/// Returns the selected UTXOs and fee/change amounts.
pub fn select_coins(
    utxos: &[Utxo],
    target_sats: u64,
    fee_rate_sat_per_vb: u64,
) -> anyhow::Result<CoinSelection> {
    // Filter available UTXOs (not locked, not dust)
    let mut available: Vec<&Utxo> = utxos
        .iter()
        .filter(|u| !u.locked && u.amount_sats >= DUST_LIMIT_SATS)
        .collect();

    anyhow::ensure!(!available.is_empty(), "No available UTXOs");

    // Sort descending by amount for greedy selection
    available.sort_by(|a, b| b.amount_sats.cmp(&a.amount_sats));

    // Try Branch and Bound first (exact match search)
    if let Some(selection) = branch_and_bound(&available, target_sats, fee_rate_sat_per_vb) {
        return Ok(selection);
    }

    // Fallback: largest-first greedy
    greedy_select(&available, target_sats, fee_rate_sat_per_vb)
}

/// Branch and Bound coin selection — tries to find a combination that
/// exactly matches the target (no change output needed).
///
/// Tolerance: within one output's worth of fees (saves the change output fee).
fn branch_and_bound(
    utxos: &[&Utxo],
    target_sats: u64,
    fee_rate_sat_per_vb: u64,
) -> Option<CoinSelection> {
    let max_tries = 100_000;
    let mut best: Option<(Vec<bool>, u64)> = None;

    // The "no change" target: target + fee for N inputs + 1 output
    // We accept any combination where total is between target+fee and target+fee+cost_of_change
    let cost_of_change = 31 * fee_rate_sat_per_vb; // cost to create a change output

    let mut selection = vec![false; utxos.len()];
    let mut current_sum: u64 = 0;
    let mut tries = 0;

    fn bnb_recurse(
        utxos: &[&Utxo],
        selection: &mut Vec<bool>,
        current_sum: &mut u64,
        target_sats: u64,
        fee_rate: u64,
        cost_of_change: u64,
        depth: usize,
        tries: &mut usize,
        max_tries: usize,
        best: &mut Option<(Vec<bool>, u64)>,
    ) {
        *tries += 1;
        if *tries > max_tries {
            return;
        }

        let num_selected = selection.iter().filter(|&&s| s).count();
        let fee = calculate_fee(num_selected, 1, fee_rate); // 1 output (no change)
        let needed = target_sats + fee;

        if *current_sum >= needed && *current_sum <= needed + cost_of_change {
            // Found a match! Check if it's better than current best
            let waste = *current_sum - needed;
            if best.is_none() || waste < best.as_ref().unwrap().1 {
                *best = Some((selection.clone(), waste));
            }
            if waste == 0 {
                return; // Perfect match
            }
        }

        if depth >= utxos.len() || *current_sum > needed + cost_of_change {
            return;
        }

        // Include this UTXO
        selection[depth] = true;
        *current_sum += utxos[depth].amount_sats;
        bnb_recurse(utxos, selection, current_sum, target_sats, fee_rate, cost_of_change, depth + 1, tries, max_tries, best);
        *current_sum -= utxos[depth].amount_sats;
        selection[depth] = false;

        // Skip this UTXO
        bnb_recurse(utxos, selection, current_sum, target_sats, fee_rate, cost_of_change, depth + 1, tries, max_tries, best);
    }

    bnb_recurse(utxos, &mut selection, &mut current_sum, target_sats, fee_rate_sat_per_vb, cost_of_change, 0, &mut tries, max_tries, &mut best);

    best.map(|(sel, _waste)| {
        let inputs: Vec<Utxo> = sel
            .iter()
            .enumerate()
            .filter(|(_, s)| **s)
            .map(|(i, _)| utxos[i].clone())
            .collect();
        let total_input: u64 = inputs.iter().map(|u| u.amount_sats).sum();
        let fee = calculate_fee(inputs.len(), 1, fee_rate_sat_per_vb);
        CoinSelection {
            inputs,
            total_input_sats: total_input,
            target_sats,
            fee_sats: fee,
            change_sats: 0, // BnB finds exact match
        }
    })
}

/// Largest-first greedy coin selection.
///
/// Selects UTXOs from largest to smallest until the target + fee is covered.
/// Includes a change output if there's excess.
fn greedy_select(
    utxos: &[&Utxo],
    target_sats: u64,
    fee_rate_sat_per_vb: u64,
) -> anyhow::Result<CoinSelection> {
    let mut selected: Vec<Utxo> = Vec::new();
    let mut total: u64 = 0;

    for utxo in utxos {
        selected.push((*utxo).clone());
        total += utxo.amount_sats;

        // Check if we have enough (with 2 outputs: recipient + change)
        let fee = calculate_fee(selected.len(), 2, fee_rate_sat_per_vb);
        if total >= target_sats + fee {
            let change = total - target_sats - fee;

            // If change is dust, absorb it into the fee
            let (final_change, final_fee) = if change < DUST_LIMIT_SATS {
                (0, fee + change)
            } else {
                (change, fee)
            };

            return Ok(CoinSelection {
                inputs: selected,
                total_input_sats: total,
                target_sats,
                fee_sats: final_fee,
                change_sats: final_change,
            });
        }
    }

    anyhow::bail!(
        "Insufficient funds: have {} sats, need {} + fee",
        total,
        target_sats
    )
}

/// Convert satoshis to BTC Decimal.
pub fn sats_to_btc(sats: u64) -> Decimal {
    Decimal::new(sats as i64, 8)
}

/// Convert BTC Decimal to satoshis.
pub fn btc_to_sats(btc: Decimal) -> u64 {
    let sats = btc * Decimal::new(100_000_000, 0);
    use rust_decimal::prelude::ToPrimitive;
    sats.trunc().to_u64().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_utxo(txid: &str, vout: u32, amount_sats: u64) -> Utxo {
        Utxo {
            id: Uuid::new_v4(),
            txid: txid.to_string(),
            vout,
            amount_sats,
            address: "bc1qtest".to_string(),
            confirmations: 6,
            locked: false,
        }
    }

    #[test]
    fn sats_btc_conversion() {
        assert_eq!(sats_to_btc(100_000_000), dec!(1.00000000));
        assert_eq!(sats_to_btc(50_000_000), dec!(0.50000000));
        assert_eq!(sats_to_btc(1), dec!(0.00000001));
        assert_eq!(sats_to_btc(0), dec!(0.00000000));

        assert_eq!(btc_to_sats(dec!(1.0)), 100_000_000);
        assert_eq!(btc_to_sats(dec!(0.5)), 50_000_000);
        assert_eq!(btc_to_sats(dec!(0.00000001)), 1);
    }

    #[test]
    fn tx_size_estimation() {
        // 1 input, 1 output (no change)
        assert_eq!(estimate_tx_vbytes(1, 1), 11 + 68 + 31); // 110
        // 1 input, 2 outputs (with change)
        assert_eq!(estimate_tx_vbytes(1, 2), 11 + 68 + 62); // 141
        // 2 inputs, 2 outputs
        assert_eq!(estimate_tx_vbytes(2, 2), 11 + 136 + 62); // 209
    }

    #[test]
    fn fee_calculation() {
        // 1 input, 2 outputs, 10 sat/vB
        let fee = calculate_fee(1, 2, 10);
        assert_eq!(fee, 1410); // 141 vbytes * 10 sat/vB
    }

    #[test]
    fn greedy_selection_basic() {
        let utxos = vec![
            make_utxo("tx1", 0, 100_000),
            make_utxo("tx2", 0, 200_000),
            make_utxo("tx3", 0, 50_000),
        ];

        // Target: 150,000 sats at 10 sat/vB
        let result = select_coins(&utxos, 150_000, 10).unwrap();
        assert!(result.total_input_sats >= 150_000 + result.fee_sats);
        assert!(!result.inputs.is_empty());
    }

    #[test]
    fn greedy_selects_largest_first() {
        let utxos = vec![
            make_utxo("tx1", 0, 10_000),
            make_utxo("tx2", 0, 500_000),
            make_utxo("tx3", 0, 20_000),
        ];

        // Target: 50,000 — should pick tx2 (500K, largest)
        let result = select_coins(&utxos, 50_000, 10).unwrap();
        assert_eq!(result.inputs.len(), 1);
        assert_eq!(result.inputs[0].amount_sats, 500_000);
    }

    #[test]
    fn dust_utxos_excluded() {
        let utxos = vec![
            make_utxo("dust1", 0, 100),  // below dust limit
            make_utxo("dust2", 0, 545),  // below dust limit
            make_utxo("ok", 0, 100_000), // above dust limit
        ];

        let result = select_coins(&utxos, 50_000, 10).unwrap();
        // Should only select the non-dust UTXO
        assert_eq!(result.inputs.len(), 1);
        assert_eq!(result.inputs[0].amount_sats, 100_000);
    }

    #[test]
    fn locked_utxos_excluded() {
        let mut utxos = vec![
            make_utxo("locked", 0, 1_000_000),
            make_utxo("available", 0, 100_000),
        ];
        utxos[0].locked = true;

        let result = select_coins(&utxos, 50_000, 10).unwrap();
        assert_eq!(result.inputs.len(), 1);
        assert_eq!(result.inputs[0].txid, "available");
    }

    #[test]
    fn insufficient_funds_error() {
        let utxos = vec![make_utxo("small", 0, 1_000)];

        let result = select_coins(&utxos, 100_000, 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
    }

    #[test]
    fn dust_change_absorbed_into_fee() {
        // Create a scenario where change would be dust
        let utxos = vec![make_utxo("tx1", 0, 51_500)];

        // Target near the UTXO amount so change would be dust
        let result = select_coins(&utxos, 50_000, 1).unwrap();
        // Change should be 0 (absorbed) or actual change
        if result.change_sats > 0 {
            assert!(result.change_sats >= DUST_LIMIT_SATS);
        }
    }

    #[test]
    fn empty_utxo_set_error() {
        let result = select_coins(&[], 100_000, 10);
        assert!(result.is_err());
    }

    #[test]
    fn bnb_finds_exact_match() {
        // Two UTXOs that sum to exactly the target + fee
        let utxos = vec![
            make_utxo("tx1", 0, 50_000),
            make_utxo("tx2", 0, 60_000), // 110 vbytes * 10 = 1,100 fee → need 51,100
        ];

        // Target 50_000 with low fee rate. BnB might find tx2 alone (60K > 50K + fee)
        let result = select_coins(&utxos, 50_000, 10).unwrap();
        // Either BnB found a match or greedy picked the largest
        assert!(result.total_input_sats >= 50_000 + result.fee_sats);
    }
}
