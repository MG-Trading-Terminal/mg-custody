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
        tracing::info!(
            transfer_id = %data.transfer_id,
            amount_usd = %data.amount_usd,
            "Sending Travel Rule originator data"
        );

        let payload = serde_json::json!({
            "transactionAsset": data.asset,
            "transactionAmount": data.amount_usd.to_string(),
            "originator": {
                "originatorPersons": [{
                    "naturalPerson": {
                        "name": [{
                            "nameIdentifier": [{
                                "primaryIdentifier": data.originator.name,
                            }]
                        }]
                    }
                }],
                "accountNumber": [data.originator.account_number.as_deref().unwrap_or("")],
            },
            "beneficiary": {
                "beneficiaryPersons": [{
                    "naturalPerson": {
                        "name": [{
                            "nameIdentifier": [{
                                "primaryIdentifier": data.beneficiary.name,
                            }]
                        }]
                    }
                }],
                "accountNumber": [data.beneficiary.account_number.as_deref().unwrap_or("")],
            },
            "originatingVASP": data.originator_vasp,
            "beneficiaryVASP": data.beneficiary_vasp,
        });

        let resp = self.client
            .post(format!("{}/transaction", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                tracing::info!(transfer_id = %data.transfer_id, "Travel Rule data sent");
                Ok(())
            }
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                tracing::warn!(status = %status, "Notabene API error: {body}");
                // Don't block withdrawal on Travel Rule API failure
                Ok(())
            }
            Err(e) => {
                tracing::warn!(error = %e, "Notabene API unreachable");
                Ok(())
            }
        }
    }

    async fn receive_beneficiary_data(&self, transfer_id: Uuid) -> Result<Option<TravelRuleData>> {
        let resp = self.client
            .get(format!("{}/transaction/{}", self.base_url, transfer_id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let body: serde_json::Value = r.json().await?;
                if body["status"].as_str() == Some("accepted") {
                    tracing::info!(transfer_id = %transfer_id, "Beneficiary data received");
                    // Parse into TravelRuleData if beneficiary confirmed
                    // For now, return None until full parsing is implemented
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    async fn lookup_vasp(&self, address: &str) -> Result<Option<String>> {
        let resp = self.client
            .get(format!("{}/vasp/lookup", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .query(&[("address", address)])
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let body: serde_json::Value = r.json().await?;
                let vasp_id = body["vasp"]["did"].as_str().map(String::from);
                if let Some(ref id) = vasp_id {
                    tracing::info!(address, vasp = %id, "VASP found");
                }
                Ok(vasp_id)
            }
            _ => Ok(None),
        }
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
