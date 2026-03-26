//! Address poisoning detection — protect against zero-value transfer attacks.
//!
//! Attackers send zero-value token transfers from addresses that look similar
//! to the user's real address (same first/last 4 chars). Users copy the wrong
//! address from transaction history → funds lost.

use crate::Chain;

/// Check if two addresses are suspiciously similar (poisoning attempt).
pub fn is_poisoning_attempt(real_address: &str, suspicious_address: &str) -> bool {
    if real_address == suspicious_address { return false; } // same address is fine

    let real = normalize(real_address);
    let sus = normalize(suspicious_address);

    if real.len() < 8 || sus.len() < 8 { return false; }

    // Check if first 4 and last 4 chars match (common poisoning pattern)
    let prefix_match = real[..4] == sus[..4];
    let suffix_match = real[real.len()-4..] == sus[sus.len()-4..];

    prefix_match && suffix_match
}

/// Check if a transfer looks like a poisoning attack.
pub fn is_suspicious_transfer(
    _chain: Chain,
    from: &str,
    to: &str,
    amount: &str,
    known_addresses: &[String],
) -> PoisoningCheck {
    // Zero-value transfers are the primary vector
    let is_zero = amount == "0" || amount == "0x0" || amount.is_empty();

    // Check if `from` mimics any known address
    let mimics_known = known_addresses.iter().any(|known| {
        is_poisoning_attempt(known, from)
    });

    // Check if `to` mimics any known address
    let to_mimics = known_addresses.iter().any(|known| {
        is_poisoning_attempt(known, to)
    });

    let risk = if is_zero && (mimics_known || to_mimics) {
        PoisoningRisk::High
    } else if is_zero {
        PoisoningRisk::Medium
    } else if mimics_known || to_mimics {
        PoisoningRisk::Low
    } else {
        PoisoningRisk::None
    };

    PoisoningCheck { is_zero_value: is_zero, mimics_known_address: mimics_known || to_mimics, risk }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoisoningRisk { None, Low, Medium, High }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoisoningCheck {
    pub is_zero_value: bool,
    pub mimics_known_address: bool,
    pub risk: PoisoningRisk,
}

fn normalize(addr: &str) -> String {
    addr.trim_start_matches("0x").to_lowercase()
}

/// Generate a warning message for UI display.
pub fn poisoning_warning(check: &PoisoningCheck) -> Option<String> {
    match check.risk {
        PoisoningRisk::High => Some(
            "WARNING: This address closely matches a known address and sent a zero-value transfer. \
             This is likely an address poisoning attack. Verify the FULL address before sending funds.".into()
        ),
        PoisoningRisk::Medium => Some(
            "CAUTION: Zero-value transfer detected. Always verify the full recipient address.".into()
        ),
        PoisoningRisk::Low => Some(
            "Note: This address is similar to a known address. Double-check before proceeding.".into()
        ),
        PoisoningRisk::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_poisoning_same_prefix_suffix() {
        // Real: 0xAbcD...1234
        // Fake: 0xAbcE...1234 (different middle, same prefix/suffix)
        assert!(is_poisoning_attempt(
            "0xAbcD1111111111111111111111111111111111234",
            "0xAbcD2222222222222222222222222222222221234",
        ));
    }

    #[test]
    fn no_poisoning_different_prefix() {
        assert!(!is_poisoning_attempt(
            "0xAbcD1111111111111111111111111111111111234",
            "0xXYZW2222222222222222222222222222222221234",
        ));
    }

    #[test]
    fn no_poisoning_same_address() {
        let addr = "0xAbcD1111111111111111111111111111111111234";
        assert!(!is_poisoning_attempt(addr, addr));
    }

    #[test]
    fn suspicious_zero_value_transfer() {
        let known = vec!["0xAbcD1111111111111111111111111111111111234".into()];
        let check = is_suspicious_transfer(
            Chain::Ethereum,
            "0xAbcD2222222222222222222222222222222221234",
            "0xvictim",
            "0",
            &known,
        );
        assert_eq!(check.risk, PoisoningRisk::High);
        assert!(check.is_zero_value);
        assert!(check.mimics_known_address);
    }

    #[test]
    fn normal_transfer_no_risk() {
        let known = vec!["0xAbcD1111111111111111111111111111111111234".into()];
        let check = is_suspicious_transfer(
            Chain::Ethereum,
            "0xCompletely_Different_Address_999999999999",
            "0xRecipient",
            "1000000",
            &known,
        );
        assert_eq!(check.risk, PoisoningRisk::None);
    }

    #[test]
    fn zero_value_medium_risk() {
        let check = is_suspicious_transfer(
            Chain::Ethereum,
            "0xUnknownSender",
            "0xRecipient",
            "0",
            &[],
        );
        assert_eq!(check.risk, PoisoningRisk::Medium);
    }

    #[test]
    fn warning_messages() {
        let high = PoisoningCheck { is_zero_value: true, mimics_known_address: true, risk: PoisoningRisk::High };
        assert!(poisoning_warning(&high).unwrap().contains("WARNING"));

        let none = PoisoningCheck { is_zero_value: false, mimics_known_address: false, risk: PoisoningRisk::None };
        assert!(poisoning_warning(&none).is_none());
    }

    #[test]
    fn risk_serialization() {
        assert_eq!(serde_json::to_string(&PoisoningRisk::High).unwrap(), r#""high""#);
    }

    #[test]
    fn case_insensitive_comparison() {
        assert!(is_poisoning_attempt(
            "0xABCD1111111111111111111111111111111111234",
            "0xabcd2222222222222222222222222222222221234",
        ));
    }
}
