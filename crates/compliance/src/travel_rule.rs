//! Travel Rule compliance (FATF Recommendation 16).
//!
//! For transfers above threshold ($1K EU / $3K US), originator and beneficiary
//! information must be exchanged between VASPs.
//!
//! Integrates with Notabene or TRISA protocol for counterparty data exchange.
//!
//! Reference:
//! - FATF: <https://www.fatf-gafi.org/en/topics/virtual-assets.html>
//! - TFR 2023/1113 (EU): <https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32023R1113>

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Travel Rule threshold (EUR, per TFR 2023/1113).
pub const EU_THRESHOLD_EUR: Decimal = Decimal::ONE_THOUSAND;

/// Travel Rule threshold (USD, per FinCEN).
pub const US_THRESHOLD_USD: u64 = 3000;

/// Originator/beneficiary data for Travel Rule compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelRuleData {
    pub transfer_id: Uuid,
    /// Originator information.
    pub originator: PersonData,
    /// Beneficiary information.
    pub beneficiary: PersonData,
    /// Transfer amount in USD equivalent.
    pub amount_usd: Decimal,
    /// Asset being transferred.
    pub asset: String,
    /// Originator VASP identifier.
    pub originator_vasp: Option<String>,
    /// Beneficiary VASP identifier.
    pub beneficiary_vasp: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Personal/entity data for Travel Rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonData {
    pub name: String,
    pub account_number: Option<String>, // blockchain address
    pub address: Option<PhysicalAddress>,
    pub national_id: Option<String>,
    pub date_of_birth: Option<String>,
}

/// Physical address for KYC/Travel Rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalAddress {
    pub street: String,
    pub city: String,
    pub country: String, // ISO 3166-1 alpha-2
    pub postal_code: Option<String>,
}

/// Trait for Travel Rule data exchange providers.
pub trait TravelRuleProvider: Send + Sync {
    /// Send originator data to the beneficiary VASP.
    fn send_originator_data(
        &self,
        data: &TravelRuleData,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Receive and validate beneficiary data from counterparty VASP.
    fn receive_beneficiary_data(
        &self,
        transfer_id: Uuid,
    ) -> impl std::future::Future<Output = Result<Option<TravelRuleData>>> + Send;

    /// Look up a VASP by blockchain address.
    fn lookup_vasp(
        &self,
        address: &str,
    ) -> impl std::future::Future<Output = Result<Option<String>>> + Send;

    fn provider_name(&self) -> &str;
}

/// Check if a transfer requires Travel Rule data exchange.
pub fn requires_travel_rule(amount_usd: Decimal, jurisdiction: &str) -> bool {
    match jurisdiction {
        "EU" | "EEA" => amount_usd >= EU_THRESHOLD_EUR, // simplified: EUR ≈ USD
        "US" => amount_usd >= Decimal::new(US_THRESHOLD_USD as i64, 0),
        _ => amount_usd >= EU_THRESHOLD_EUR, // default to stricter threshold
    }
}

/// Notabene Travel Rule provider.
pub struct NotabeneProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl NotabeneProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.notabene.id/tf/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

impl TravelRuleProvider for NotabeneProvider {
    async fn send_originator_data(&self, data: &TravelRuleData) -> Result<()> {
        // TODO: POST /tf/v1/transaction with originator data
        tracing::info!(
            transfer_id = %data.transfer_id,
            amount_usd = %data.amount_usd,
            "Sending Travel Rule data (stub)"
        );
        Ok(())
    }

    async fn receive_beneficiary_data(&self, transfer_id: Uuid) -> Result<Option<TravelRuleData>> {
        // TODO: GET /tf/v1/transaction/{id}
        let _ = transfer_id;
        Ok(None)
    }

    async fn lookup_vasp(&self, address: &str) -> Result<Option<String>> {
        // TODO: GET /tf/v1/vasp/lookup?address={address}
        let _ = address;
        Ok(None)
    }

    fn provider_name(&self) -> &str {
        "notabene"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn eu_travel_rule_threshold() {
        assert!(!requires_travel_rule(dec!(999), "EU"));
        assert!(requires_travel_rule(dec!(1000), "EU"));
        assert!(requires_travel_rule(dec!(50000), "EU"));
    }

    #[test]
    fn us_travel_rule_threshold() {
        assert!(!requires_travel_rule(dec!(2999), "US"));
        assert!(requires_travel_rule(dec!(3000), "US"));
    }

    #[test]
    fn unknown_jurisdiction_uses_strict() {
        // Default to EU threshold (stricter)
        assert!(requires_travel_rule(dec!(1000), "JP"));
        assert!(!requires_travel_rule(dec!(999), "JP"));
    }

    #[test]
    fn travel_rule_data_serialization() {
        let data = TravelRuleData {
            transfer_id: Uuid::new_v4(),
            originator: PersonData {
                name: "Alice".to_string(),
                account_number: Some("0x123".to_string()),
                address: None,
                national_id: None,
                date_of_birth: None,
            },
            beneficiary: PersonData {
                name: "Bob".to_string(),
                account_number: Some("0x456".to_string()),
                address: None,
                national_id: None,
                date_of_birth: None,
            },
            amount_usd: dec!(5000),
            asset: "ETH".to_string(),
            originator_vasp: Some("VASP001".to_string()),
            beneficiary_vasp: Some("VASP002".to_string()),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("Alice"));
        assert!(json.contains("VASP001"));
    }
}
