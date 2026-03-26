//! Advanced Bitcoin — Taproot (P2TR), Schnorr, PSBT, Ordinals awareness.

use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;

/// Bitcoin address types with their properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BtcAddressType {
    /// P2PKH — Legacy (1...)
    P2pkh,
    /// P2SH — Script Hash (3...)
    P2sh,
    /// P2WPKH — Native SegWit (bc1q..., 42 chars)
    P2wpkh,
    /// P2WSH — SegWit Script Hash (bc1q..., 62 chars)
    P2wsh,
    /// P2TR — Taproot (bc1p..., 62 chars)
    P2tr,
}

impl BtcAddressType {
    /// Estimated input size in vBytes for coin selection.
    pub fn input_vbytes(&self) -> u32 {
        match self {
            Self::P2pkh => 148,
            Self::P2sh => 91,
            Self::P2wpkh => 68,
            Self::P2wsh => 104,
            Self::P2tr => 58, // most efficient
        }
    }

    /// Detect address type from address string.
    pub fn detect(address: &str) -> Option<Self> {
        if address.starts_with("bc1p") && address.len() == 62 { return Some(Self::P2tr); }
        if address.starts_with("bc1q") && address.len() == 42 { return Some(Self::P2wpkh); }
        if address.starts_with("bc1q") && address.len() == 62 { return Some(Self::P2wsh); }
        if address.starts_with('1') { return Some(Self::P2pkh); }
        if address.starts_with('3') { return Some(Self::P2sh); }
        None
    }
}

/// Taproot key spend path parameters.
#[derive(Debug, Clone)]
pub struct TaprootKeySpend {
    pub internal_key: [u8; 32],    // x-only pubkey
    pub tweak: Option<[u8; 32]>,   // merkle root of script tree (if any)
}

/// PSBT (Partially Signed Bitcoin Transaction) metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsbtInfo {
    pub version: u32,
    pub inputs_count: u32,
    pub outputs_count: u32,
    pub total_input_value: Decimal,
    pub total_output_value: Decimal,
    pub fee: Decimal,
    pub is_complete: bool,
    pub missing_signatures: Vec<u32>, // input indices
}

/// Estimate transaction fee based on inputs/outputs and fee rate.
pub fn estimate_tx_fee(
    inputs: &[(BtcAddressType, u32)], // (type, count)
    output_count: u32,
    fee_rate_sat_per_vb: Decimal,
) -> Decimal {
    let overhead_vb: u32 = 10; // version + locktime + witness flag
    let output_vb: u32 = output_count * 31; // P2WPKH output

    let input_vb: u32 = inputs.iter()
        .map(|(addr_type, count)| addr_type.input_vbytes() * count)
        .sum();

    let total_vb = overhead_vb + input_vb + output_vb;
    fee_rate_sat_per_vb * Decimal::new(total_vb as i64, 0)
        / Decimal::new(100_000_000, 0) // sat to BTC
}

/// Ordinals/Inscriptions — detect if a UTXO might contain an inscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdinalInfo {
    pub txid: String,
    pub vout: u32,
    pub value_sats: u64,
    pub has_inscription: bool,
    pub content_type: Option<String>,
    pub inscription_number: Option<u64>,
}

/// Check if a UTXO value is likely an inscription (546 sats = dust limit).
pub fn is_likely_inscription(value_sats: u64) -> bool {
    value_sats == 546 || value_sats == 330 || value_sats == 10000
}

/// BIP-21 URI builder.
pub fn build_bip21_uri(address: &str, amount_btc: Option<Decimal>, label: Option<&str>) -> String {
    let mut uri = format!("bitcoin:{address}");
    let mut params = Vec::new();
    if let Some(amount) = amount_btc {
        params.push(format!("amount={amount}"));
    }
    if let Some(label) = label {
        params.push(format!("label={}", urlencoding_simple(label)));
    }
    if !params.is_empty() {
        uri.push('?');
        uri.push_str(&params.join("&"));
    }
    uri
}

fn urlencoding_simple(s: &str) -> String {
    s.replace(' ', "%20").replace('&', "%26")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn detect_p2wpkh() {
        assert_eq!(
            BtcAddressType::detect("bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"),
            Some(BtcAddressType::P2wpkh)
        );
    }

    #[test]
    fn detect_p2tr() {
        assert_eq!(
            BtcAddressType::detect("bc1p5cyxnuxmeuwuvkwfem96lqzszee2457nljwu97q8ppsegjgqmveqmk6vsu"),
            Some(BtcAddressType::P2tr)
        );
    }

    #[test]
    fn detect_legacy() {
        assert_eq!(BtcAddressType::detect("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"), Some(BtcAddressType::P2pkh));
        assert_eq!(BtcAddressType::detect("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"), Some(BtcAddressType::P2sh));
    }

    #[test]
    fn taproot_most_efficient() {
        assert!(BtcAddressType::P2tr.input_vbytes() < BtcAddressType::P2wpkh.input_vbytes());
        assert!(BtcAddressType::P2wpkh.input_vbytes() < BtcAddressType::P2pkh.input_vbytes());
    }

    #[test]
    fn fee_estimation() {
        let fee = estimate_tx_fee(
            &[(BtcAddressType::P2wpkh, 2)],
            2,
            dec!(20), // 20 sat/vB
        );
        // (10 + 2*68 + 2*31) * 20 / 1e8 = 208 * 20 / 1e8 = 0.0000416
        assert!(fee > dec!(0.00004));
        assert!(fee < dec!(0.00005));
    }

    #[test]
    fn fee_estimation_taproot() {
        let fee_segwit = estimate_tx_fee(&[(BtcAddressType::P2wpkh, 1)], 1, dec!(10));
        let fee_taproot = estimate_tx_fee(&[(BtcAddressType::P2tr, 1)], 1, dec!(10));
        assert!(fee_taproot < fee_segwit); // Taproot is cheaper
    }

    #[test]
    fn inscription_detection() {
        assert!(is_likely_inscription(546));
        assert!(is_likely_inscription(330));
        assert!(!is_likely_inscription(100_000));
    }

    #[test]
    fn bip21_uri_basic() {
        let uri = build_bip21_uri("bc1qtest", None, None);
        assert_eq!(uri, "bitcoin:bc1qtest");
    }

    #[test]
    fn bip21_uri_with_amount() {
        let uri = build_bip21_uri("bc1qtest", Some(dec!(0.5)), Some("Payment"));
        assert!(uri.contains("amount=0.5"));
        assert!(uri.contains("label=Payment"));
    }

    #[test]
    fn address_type_serialization() {
        assert_eq!(serde_json::to_string(&BtcAddressType::P2tr).unwrap(), r#""p2tr""#);
        assert_eq!(serde_json::to_string(&BtcAddressType::P2wpkh).unwrap(), r#""p2wpkh""#);
    }

    #[test]
    fn psbt_info_structure() {
        let psbt = PsbtInfo {
            version: 2, inputs_count: 3, outputs_count: 2,
            total_input_value: dec!(1.5), total_output_value: dec!(1.4999),
            fee: dec!(0.0001), is_complete: false, missing_signatures: vec![1, 2],
        };
        let json = serde_json::to_string(&psbt).unwrap();
        assert!(json.contains("missing_signatures"));
    }
}
