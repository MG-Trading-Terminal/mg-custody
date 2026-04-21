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

/// Build a provider-shaped `TravelRuleData` from the persistence-layer
/// record the gateway writes (`custody_common::platform::TravelRuleData`).
/// Centralises the field-name mapping so handlers don't do it inline.
pub fn data_from_persisted(
    persisted: &custody_common::platform::TravelRuleData,
) -> TravelRuleData {
    TravelRuleData {
        transfer_id: persisted.id,
        originator: PersonData {
            name: persisted.originator.name.clone(),
            account_number: persisted.originator.account_number.clone(),
            address: persisted.originator.address.as_ref().map(|a| PhysicalAddress {
                street: a.street.clone(),
                city: a.city.clone(),
                country: a.country.clone(),
                postal_code: Some(a.postal_code.clone()),
            }),
            national_id: None,
            date_of_birth: None,
        },
        beneficiary: PersonData {
            name: persisted.beneficiary.name.clone(),
            account_number: persisted.beneficiary.account_number.clone(),
            address: persisted.beneficiary.address.as_ref().map(|a| PhysicalAddress {
                street: a.street.clone(),
                city: a.city.clone(),
                country: a.country.clone(),
                postal_code: Some(a.postal_code.clone()),
            }),
            national_id: None,
            date_of_birth: None,
        },
        amount_usd: persisted.amount,
        asset: persisted.asset.as_str().to_string(),
        originator_vasp: persisted.originator.vasp_id.clone(),
        beneficiary_vasp: persisted.beneficiary.vasp_id.clone(),
        created_at: persisted.created_at,
    }
}

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

/// Parse a Notabene GET /transaction/{id} response into our
/// neutral `TravelRuleData` shape. Mirrors the IVMS-101-style
/// structure we POST on send_originator_data. Fields that are
/// genuinely missing collapse to empty strings / None rather than
/// returning an error — a real counterparty VASP might submit a
/// subset of optional fields, and rejecting that would stall the
/// workflow on every partial entry.
///
/// Returns Err only when the top-level shape is unrecognisable
/// (no originator/beneficiary objects at all) — that IS a reason
/// to escalate to human review rather than silently accept an
/// empty record as valid TR data.
fn parse_notabene_transfer(
    body: &serde_json::Value,
    transfer_id: Uuid,
) -> Result<TravelRuleData> {
    // Notabene wraps the parties under `originator` / `beneficiary`
    // at the top level. We accept either their full IVMS shape or
    // the flatter echoes some sandbox environments return.
    let originator_obj = body
        .get("originator")
        .ok_or_else(|| anyhow::anyhow!("Notabene response missing originator"))?;
    let beneficiary_obj = body
        .get("beneficiary")
        .ok_or_else(|| anyhow::anyhow!("Notabene response missing beneficiary"))?;

    fn extract_person(v: &serde_json::Value, kind: &str) -> PersonData {
        // IVMS path: {"originatorPersons":[{"naturalPerson":
        //   {"name":[{"nameIdentifier":[{"primaryIdentifier": "..."}]}]}}]}
        let ivms_name = v
            .get(format!("{kind}Persons"))
            .and_then(|a| a.get(0))
            .and_then(|p| p.get("naturalPerson"))
            .and_then(|np| np.get("name"))
            .and_then(|a| a.get(0))
            .and_then(|n| n.get("nameIdentifier"))
            .and_then(|a| a.get(0))
            .and_then(|id| id.get("primaryIdentifier"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        // Flat fallback: {"name": "..."}
        let flat_name = v.get("name").and_then(|s| s.as_str()).map(|s| s.to_string());
        let account = v
            .get("accountNumber")
            .and_then(|a| a.get(0).or(Some(a)))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        PersonData {
            name: ivms_name.or(flat_name).unwrap_or_default(),
            account_number: account,
            address: None,
            national_id: None,
            date_of_birth: None,
        }
    }

    let originator = extract_person(originator_obj, "originator");
    let beneficiary = extract_person(beneficiary_obj, "beneficiary");
    let amount_usd = body
        .get("transactionAmount")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Decimal>().ok())
        .unwrap_or_default();
    let asset = body
        .get("transactionAsset")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let originator_vasp = body
        .get("originatingVASP")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let beneficiary_vasp = body
        .get("beneficiaryVASP")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(TravelRuleData {
        transfer_id,
        originator,
        beneficiary,
        amount_usd,
        asset,
        originator_vasp,
        beneficiary_vasp,
        created_at: Utc::now(),
    })
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

        // Fail-closed on dispatch errors. Earlier versions returned
        // Ok(()) on any outcome with a warning log — the pipeline
        // called this, saw success, and broadcast without the
        // required data exchange. That was the bug the readiness
        // gate on screening.aml was also designed to prevent:
        // configured-vendor-unreachable is not "OK, proceed".
        match resp {
            Ok(r) if r.status().is_success() => {
                tracing::info!(transfer_id = %data.transfer_id, "Travel Rule data sent");
                Ok(())
            }
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                tracing::warn!(status = %status, "Notabene API error: {body}");
                anyhow::bail!("Notabene API returned {status}: {body}")
            }
            Err(e) => {
                tracing::warn!(error = %e, "Notabene API unreachable");
                Err(anyhow::anyhow!("Notabene API unreachable: {e}"))
            }
        }
    }

    async fn receive_beneficiary_data(&self, transfer_id: Uuid) -> Result<Option<TravelRuleData>> {
        let resp = self.client
            .get(format!("{}/transaction/{}", self.base_url, transfer_id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        // Status semantics for the caller:
        //   Ok(Some(_))   — beneficiary data retrieved and parsed.
        //   Ok(None)      — counterparty has not posted data yet
        //                   (status != "accepted", or transport error).
        //   Err(_)        — Notabene acknowledged the transfer but we
        //                   cannot parse the response. This is NOT the
        //                   same as "no data"; surfacing it as `Err`
        //                   forces the caller to record the transfer
        //                   as "review manually" instead of silently
        //                   treating it as "still waiting".
        match resp {
            Ok(r) if r.status().is_success() => {
                let body: serde_json::Value = r.json().await?;
                match body["status"].as_str() {
                    Some("accepted") => Ok(Some(parse_notabene_transfer(&body, transfer_id)?)),
                    Some(other) => {
                        // Counterparty hasn't responded yet or rejected —
                        // "pending", "sent", "rejected" all map to "no
                        // beneficiary data yet". Caller polls again.
                        tracing::debug!(
                            transfer_id = %transfer_id, status = %other,
                            "Notabene transfer not yet accepted"
                        );
                        Ok(None)
                    }
                    None => {
                        tracing::warn!(
                            transfer_id = %transfer_id,
                            "Notabene GET returned no status field"
                        );
                        Ok(None)
                    }
                }
            }
            Ok(r) => {
                tracing::debug!(transfer_id = %transfer_id, status = %r.status(), "Notabene returned non-success");
                Ok(None)
            }
            Err(e) => {
                tracing::debug!(transfer_id = %transfer_id, error = %e, "Notabene transport error");
                Ok(None)
            }
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
    fn parse_notabene_ivms_shape_round_trip() {
        // Full IVMS-101-style response — what Notabene returns in
        // production once the counterparty VASP accepts.
        let body = serde_json::json!({
            "status": "accepted",
            "transactionAsset": "BTC",
            "transactionAmount": "1500",
            "originator": {
                "originatorPersons": [{
                    "naturalPerson": {
                        "name": [{
                            "nameIdentifier": [{"primaryIdentifier": "Alice Smith"}]
                        }]
                    }
                }],
                "accountNumber": ["bc1qorig..."],
            },
            "beneficiary": {
                "beneficiaryPersons": [{
                    "naturalPerson": {
                        "name": [{
                            "nameIdentifier": [{"primaryIdentifier": "Bob Jones"}]
                        }]
                    }
                }],
                "accountNumber": ["bc1qbeneficiary..."],
            },
            "originatingVASP": "vasp-us-123",
            "beneficiaryVASP": "vasp-eu-456",
        });
        let id = Uuid::new_v4();
        let parsed = parse_notabene_transfer(&body, id).unwrap();
        assert_eq!(parsed.transfer_id, id);
        assert_eq!(parsed.originator.name, "Alice Smith");
        assert_eq!(parsed.originator.account_number.as_deref(), Some("bc1qorig..."));
        assert_eq!(parsed.beneficiary.name, "Bob Jones");
        assert_eq!(parsed.amount_usd, dec!(1500));
        assert_eq!(parsed.asset, "BTC");
        assert_eq!(parsed.originator_vasp.as_deref(), Some("vasp-us-123"));
        assert_eq!(parsed.beneficiary_vasp.as_deref(), Some("vasp-eu-456"));
    }

    #[test]
    fn parse_notabene_flat_fallback() {
        // Some sandbox environments return a flatter echo shape —
        // the parser must tolerate "name" at the top of each party.
        let body = serde_json::json!({
            "originator":   {"name": "Alice", "accountNumber": "0xabc"},
            "beneficiary":  {"name": "Bob",   "accountNumber": "0xdef"},
            "transactionAmount": "250",
            "transactionAsset": "USDC",
        });
        let parsed = parse_notabene_transfer(&body, Uuid::new_v4()).unwrap();
        assert_eq!(parsed.originator.name, "Alice");
        assert_eq!(parsed.beneficiary.name, "Bob");
        assert_eq!(parsed.amount_usd, dec!(250));
    }

    #[test]
    fn parse_notabene_missing_parties_errors() {
        // If both party objects are missing we escalate to "review
        // manually" rather than return a bogus empty TravelRuleData.
        let body = serde_json::json!({"status": "accepted"});
        let res = parse_notabene_transfer(&body, Uuid::new_v4());
        assert!(res.is_err());
    }

    #[test]
    fn parse_notabene_tolerates_missing_optional_fields() {
        // Realistic partial case: counterparty omitted VASP ids.
        // Parser must NOT reject — it's still valid TR data.
        let body = serde_json::json!({
            "originator":  {"name": "A"},
            "beneficiary": {"name": "B"},
        });
        let parsed = parse_notabene_transfer(&body, Uuid::new_v4()).unwrap();
        assert!(parsed.originator_vasp.is_none());
        assert!(parsed.beneficiary_vasp.is_none());
        assert_eq!(parsed.amount_usd, Decimal::default());
    }

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
