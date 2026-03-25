//! On-chain AML analysis (self-hosted alternative to Chainalysis).
//!
//! Basic heuristics for detecting high-risk addresses:
//! - Known mixer/tumbler addresses
//! - High-volume dust patterns
//! - Rapid fan-out transactions
//! - Darknet marketplace indicators
//!
//! This is NOT a replacement for professional AML services but provides
//! basic screening when Chainalysis/Elliptic API keys are not available.

use custody_common::Chain;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Risk indicators from on-chain analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnChainRiskIndicators {
    pub address: String,
    pub chain: Chain,
    /// Known mixer/tumbler interaction.
    pub mixer_interaction: bool,
    /// Known darknet marketplace interaction.
    pub darknet_interaction: bool,
    /// High-volume small transactions (dust attack pattern).
    pub dust_pattern: bool,
    /// Rapid fan-out (one input → many outputs in short time).
    pub fan_out_pattern: bool,
    /// Address age (recently created addresses are higher risk).
    pub is_new_address: bool,
    /// Number of risk indicators triggered.
    pub risk_score: u8,
}

impl OnChainRiskIndicators {
    pub fn is_high_risk(&self) -> bool {
        self.risk_score >= 3 || self.mixer_interaction || self.darknet_interaction
    }
}

/// Known mixer/tumbler contract addresses.
pub fn known_mixer_addresses() -> HashSet<String> {
    let mut mixers = HashSet::new();

    // Tornado Cash (Ethereum) — OFAC sanctioned
    mixers.insert("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b".into());
    mixers.insert("0xd4b88df4d29f5cedd6857912842cff3b20c8cfa3".into());
    mixers.insert("0x910cbd523d972eb0a6f4cae4618ad62622b39dbf".into());
    mixers.insert("0xa160cdab225685da1d56aa342ad8841c3b53f291".into());
    mixers.insert("0xfd8610d20aa15b7b2e3be39b396a1bc3516c7144".into());
    mixers.insert("0xf60dd140cff0706bae9cd734ac3683550e9b0e97".into());
    mixers.insert("0x22aaa7720ddd5388a3c0a3333430953c68f1849b".into());
    mixers.insert("0xba214c1c1928a32bffe790263e38b4af9bfcd659".into());
    mixers.insert("0xb1c8094b234dce6e03f10a5b673c1d8c69739a00".into());
    mixers.insert("0x527653ea119f3e6a1f5bd18fbf4714081d7b31ce".into());

    // Tornado Cash (BSC)
    mixers.insert("0x84443cfd09a48af6ef360c6976c5392ac5023a1f".into());

    // Railgun (privacy protocol)
    mixers.insert("0xfa7093cdd9ee6932b4eb2c9e1cde7ce00b1fa4b9".into());

    // ChipMixer (Bitcoin — base58)
    // Not including BTC addresses here as they change frequently

    mixers
}

/// Check if an address has interacted with known mixers.
pub fn check_mixer_interaction(address: &str) -> bool {
    let mixers = known_mixer_addresses();
    mixers.contains(&address.to_lowercase())
}

/// Basic risk scoring for an address (0-5 scale).
pub fn basic_risk_score(
    is_mixer: bool,
    is_darknet: bool,
    is_dust: bool,
    is_fan_out: bool,
    is_new: bool,
) -> u8 {
    let mut score = 0u8;
    if is_mixer { score += 3; }    // Mixer = high risk
    if is_darknet { score += 3; }  // Darknet = high risk
    if is_dust { score += 1; }     // Dust pattern = moderate
    if is_fan_out { score += 1; }  // Fan-out = moderate
    if is_new { score += 1; }      // New address = slight risk
    score.min(5) // cap at 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_mixer_check() {
        assert!(check_mixer_interaction("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b"));
        assert!(!check_mixer_interaction("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18"));
    }

    #[test]
    fn risk_scoring() {
        assert_eq!(basic_risk_score(false, false, false, false, false), 0);
        assert_eq!(basic_risk_score(true, false, false, false, false), 3); // mixer alone = 3
        assert_eq!(basic_risk_score(true, true, true, true, true), 5);    // capped at 5
        assert_eq!(basic_risk_score(false, false, true, true, true), 3);  // moderate indicators
    }

    #[test]
    fn known_mixers_list() {
        let mixers = known_mixer_addresses();
        assert!(mixers.len() >= 10); // at least 10 known addresses
    }

    #[test]
    fn risk_indicators_high_risk() {
        let indicators = OnChainRiskIndicators {
            address: "0xbad".into(),
            chain: Chain::Ethereum,
            mixer_interaction: true,
            darknet_interaction: false,
            dust_pattern: false,
            fan_out_pattern: false,
            is_new_address: false,
            risk_score: 3,
        };
        assert!(indicators.is_high_risk());
    }

    #[test]
    fn risk_indicators_low_risk() {
        let indicators = OnChainRiskIndicators {
            address: "0xgood".into(),
            chain: Chain::Ethereum,
            mixer_interaction: false,
            darknet_interaction: false,
            dust_pattern: false,
            fan_out_pattern: false,
            is_new_address: true,
            risk_score: 1,
        };
        assert!(!indicators.is_high_risk());
    }
}
