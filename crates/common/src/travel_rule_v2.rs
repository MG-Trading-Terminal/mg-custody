//! Travel Rule V2 — VASP directory, proof of address, enhanced compliance.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// VASP (Virtual Asset Service Provider) in the directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaspEntry {
    pub did: String,
    pub name: String,
    pub country: String,
    pub jurisdiction_type: JurisdictionType,
    pub protocols: Vec<TravelRuleProtocol>,
    pub is_verified: bool,
    pub last_verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JurisdictionType { Eu, Us, Uk, Switzerland, Singapore, Japan, Other }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelRuleProtocol { Notabene, Trisa, OpenVasp, Sygna, TrustVasp }

/// Travel Rule transaction data (enhanced from V1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelRuleTransaction {
    pub id: Uuid,
    pub withdrawal_id: Uuid,
    pub originator_vasp: String,
    pub beneficiary_vasp: Option<String>,
    pub amount: Decimal,
    pub asset: String,
    pub originator_info: PartyInfo,
    pub beneficiary_info: PartyInfo,
    pub threshold: TravelRuleThreshold,
    pub proof_of_address: Option<ProofOfAddress>,
    pub status: TrStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyInfo {
    pub name: String,
    pub account_number: Option<String>,
    pub address: Option<String>,
    pub country: Option<String>,
    pub national_id: Option<String>,
    pub date_of_birth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelRuleThreshold {
    pub currency: String,
    pub amount: Decimal,
    pub jurisdiction: JurisdictionType,
}

/// Default thresholds by jurisdiction.
pub fn default_thresholds() -> Vec<TravelRuleThreshold> {
    use rust_decimal_macros::dec;
    vec![
        TravelRuleThreshold { currency: "EUR".into(), amount: dec!(1000), jurisdiction: JurisdictionType::Eu },
        TravelRuleThreshold { currency: "USD".into(), amount: dec!(3000), jurisdiction: JurisdictionType::Us },
        TravelRuleThreshold { currency: "GBP".into(), amount: dec!(1000), jurisdiction: JurisdictionType::Uk },
        TravelRuleThreshold { currency: "CHF".into(), amount: dec!(1000), jurisdiction: JurisdictionType::Switzerland },
        TravelRuleThreshold { currency: "SGD".into(), amount: dec!(1500), jurisdiction: JurisdictionType::Singapore },
        TravelRuleThreshold { currency: "JPY".into(), amount: dec!(100000), jurisdiction: JurisdictionType::Japan },
    ]
}

/// Proof of Address — cryptographic proof that an address belongs to a VASP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfAddress {
    pub address: String,
    pub chain: String,
    pub vasp_did: String,
    pub signature: String,
    pub message: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrStatus { Required, Pending, Sent, Received, Verified, Rejected, Exempted }

/// Staking provider with terms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingProvider {
    pub id: String,
    pub name: String,
    pub chains: Vec<String>,
    pub commission_percent: Decimal,
    pub min_stake: Decimal,
    pub unbonding_days: u32,
    pub apy_range: (Decimal, Decimal),
    pub is_liquid: bool,
    pub audited: bool,
}

/// Known staking providers.
pub fn staking_providers() -> Vec<StakingProvider> {
    use rust_decimal_macros::dec;
    vec![
        StakingProvider { id: "lido".into(), name: "Lido".into(), chains: vec!["ethereum".into(), "solana".into()], commission_percent: dec!(10), min_stake: dec!(0.01), unbonding_days: 1, apy_range: (dec!(3.0), dec!(4.0)), is_liquid: true, audited: true },
        StakingProvider { id: "rocketpool".into(), name: "Rocket Pool".into(), chains: vec!["ethereum".into()], commission_percent: dec!(15), min_stake: dec!(0.01), unbonding_days: 1, apy_range: (dec!(2.5), dec!(3.5)), is_liquid: true, audited: true },
        StakingProvider { id: "figment".into(), name: "Figment".into(), chains: vec!["ethereum".into(), "solana".into(), "cosmos".into(), "polkadot".into()], commission_percent: dec!(8), min_stake: dec!(32), unbonding_days: 27, apy_range: (dec!(3.0), dec!(7.0)), is_liquid: false, audited: true },
        StakingProvider { id: "coinbase".into(), name: "Coinbase Cloud".into(), chains: vec!["ethereum".into()], commission_percent: dec!(25), min_stake: dec!(0.01), unbonding_days: 1, apy_range: (dec!(2.5), dec!(3.5)), is_liquid: true, audited: true },
        StakingProvider { id: "kiln".into(), name: "Kiln".into(), chains: vec!["ethereum".into(), "solana".into(), "cosmos".into()], commission_percent: dec!(8), min_stake: dec!(32), unbonding_days: 27, apy_range: (dec!(3.0), dec!(7.0)), is_liquid: false, audited: true },
        StakingProvider { id: "marinade".into(), name: "Marinade".into(), chains: vec!["solana".into()], commission_percent: dec!(6), min_stake: dec!(0.01), unbonding_days: 2, apy_range: (dec!(6.0), dec!(8.0)), is_liquid: true, audited: true },
    ]
}

/// Asset conversion (exchange trading pairs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetConversion {
    pub id: Uuid,
    pub from_asset: String,
    pub to_asset: String,
    pub from_amount: Decimal,
    pub to_amount: Decimal,
    pub rate: Decimal,
    pub fee: Decimal,
    pub exchange: String,
    pub status: ConversionStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStatus { Quoted, Executing, Completed, Failed, Expired }

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn default_thresholds_all_jurisdictions() {
        let thresholds = default_thresholds();
        assert!(thresholds.len() >= 6);
        let eu = thresholds.iter().find(|t| t.jurisdiction == JurisdictionType::Eu).unwrap();
        assert_eq!(eu.amount, dec!(1000));
        let us = thresholds.iter().find(|t| t.jurisdiction == JurisdictionType::Us).unwrap();
        assert_eq!(us.amount, dec!(3000));
    }

    #[test]
    fn vasp_entry_structure() {
        let vasp = VaspEntry {
            did: "did:web:custody.example.com".into(), name: "Example Custody".into(),
            country: "CH".into(), jurisdiction_type: JurisdictionType::Switzerland,
            protocols: vec![TravelRuleProtocol::Notabene], is_verified: true,
            last_verified_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&vasp).unwrap();
        assert!(json.contains("did:web"));
    }

    #[test]
    fn staking_providers_count() {
        assert!(staking_providers().len() >= 6);
    }

    #[test]
    fn lido_multi_chain() {
        let providers = staking_providers();
        let lido = providers.iter().find(|p| p.id == "lido").unwrap();
        assert!(lido.chains.len() >= 2);
        assert!(lido.is_liquid);
    }

    #[test]
    fn travel_rule_protocol_serialization() {
        assert_eq!(serde_json::to_string(&TravelRuleProtocol::Notabene).unwrap(), r#""notabene""#);
        assert_eq!(serde_json::to_string(&TravelRuleProtocol::Trisa).unwrap(), r#""trisa""#);
    }

    #[test]
    fn conversion_status() {
        assert_eq!(serde_json::to_string(&ConversionStatus::Executing).unwrap(), r#""executing""#);
    }

    #[test]
    fn tr_status_flow() {
        let statuses = vec![TrStatus::Required, TrStatus::Pending, TrStatus::Sent, TrStatus::Verified];
        for s in statuses {
            let json = serde_json::to_string(&s).unwrap();
            let parsed: TrStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, parsed);
        }
    }
}
