//! Travel Rule compliance (FATF Recommendation 16).
//!
//! For transfers above threshold ($1K EU / $3K US), originator and beneficiary
//! information must be exchanged between VASPs.
//!
//! Supports Notabene dispatch and 21 Travel Rule TRUST notifications.
//!
//! Reference:
//! - FATF: <https://www.fatf-gafi.org/en/topics/virtual-assets.html>
//! - TFR 2023/1113 (EU): <https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32023R1113>

use anyhow::{Context, Result, ensure};
use reqwest::{Client, Url};
use chrono::{DateTime, Utc};
use custody_common::{Chain, platform::TrustParties};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

const TRUST_BITCOIN: &str = "DTI4H95J0R2X";
const LOOKUP: &str = "query Lookup($address: String!, $asset: Asset!) {
    trustd { lookupBulletinBoard(address: $address, asset: $asset) { id name lei status } }
}";
const SEND: &str = "mutation Send($address: String!, $ivms101: TrustdTxDetailsInput!) {
    autod { sendTxToAddrOwner(address: $address, ivms101: $ivms101) {
        __typename
        ... on TrustdTx { trustd { id } }
    } }
}";
const RECORDS: &str = "query Records($filter: TravelTxFilterInput!) {
    autod { txDetails(limit: 2, filter: $filter) {
        ... on TrustdTx { trustd { id txId asset amount type beneficiary {
            walletAddress name { ... on TravelNaturalName { firstName lastName }
                                 ... on TrpdLegalName { name } }
        } } }
    } }
}";

/// The API did not confirm the outcome of a mutation that may have reached the recipient.
#[derive(Debug)]
pub struct TrustSendUnconfirmed;

impl std::fmt::Display for TrustSendUnconfirmed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("21 Travel Rule did not confirm a TRUST send. Check delivery before retrying.")
    }
}

impl std::error::Error for TrustSendUnconfirmed {}

/// Runtime settings shared by the withdrawal dispatcher and admin CLI.
pub struct TwentyOneTravelConfig {
    base_url: String,
    client_id: String,
    secret: String,
}

impl TwentyOneTravelConfig {
    /// Load the HTTPS deployment and service-account credentials from the environment.
    pub fn from_env() -> Result<Self> {
        fn required(name: &str) -> Result<String> {
            let value = std::env::var(name).with_context(|| format!("Set {name}"))?;
            ensure!(!value.trim().is_empty(), "Set {name}");
            Ok(value)
        }

        Ok(Self {
            base_url: required("CUSTODY_21_TRAVEL__BASE_URL")?,
            client_id: required("CUSTODY_21_TRAVEL__CLIENT_ID")?,
            secret: required("CUSTODY_21_TRAVEL__CLIENT_SECRET")?,
        })
    }

    /// Open an authenticated API session.
    pub async fn connect(&self) -> Result<TwentyOneTravelProvider> {
        TwentyOneTravelProvider::authenticate(&self.base_url, &self.client_id, &self.secret).await
    }
}

/// The 8.4.0 DTI for a native asset or a token supported by the custody signing pipeline.
pub fn trust_asset(chain: Chain, symbol: &str) -> Result<&'static str> {
    Ok(match (chain, symbol) {
        (Chain::Bitcoin, "BTC") => TRUST_BITCOIN,
        (Chain::Ethereum, "ETH") => "DTIX9J9K872S",
        (Chain::Ethereum, "USDT") => "DTI2QWSBDMNC",
        (Chain::Ethereum, "USDC") => "DTICK9PW1MFH",
        (Chain::Tron, "TRX") => "DTI993D8X1FB",
        (Chain::Tron, "USDT") => "DTIC9N6ZVN7S",
        (Chain::Tron, "USDC") => "DTIT6TQ8GN7L",
        (Chain::Solana, "SOL") => "DTI20J63Z4N3",
        _ => anyhow::bail!("Asset is unsupported for TRUST on this chain"),
    })
}

/// Validate the withdrawal asset and scale and serialise the TRUST mutation input.
pub fn trust_withdrawal_details(
    chain: Chain,
    symbol: &str,
    amount: Decimal,
    parties: &TrustParties,
) -> Result<Value> {
    let asset = trust_asset(chain, symbol)?;
    let originator = &parties.originator;
    let address = &originator.address;
    let required = [&originator.id, &originator.customer_identification, &address.address_line,
        &address.postal_code, &address.town];
    ensure!(required.iter().all(|value| !value.trim().is_empty())
            && address.country.len() == 2 && address.country.bytes().all(|b| b.is_ascii_uppercase()),
            "TRUST originator requires identity and a complete postal address with a two-letter country code");
    use custody_common::platform::TrustBeneficiaryName;
    let valid_name = match &parties.beneficiary_name {
        TrustBeneficiaryName::Natural { first_name, last_name } =>
            !first_name.trim().is_empty() && !last_name.trim().is_empty(),
        TrustBeneficiaryName::Legal { name } => !name.trim().is_empty(),
    };
    ensure!(valid_name, "TRUST requires the beneficiary name");
    let registry = custody_common::token_registry::TokenRegistry::with_defaults();
    let decimals = registry.decimals(symbol, chain).context("Missing asset decimals")?;
    let amount = amount.checked_mul(Decimal::from(10u64.pow(decimals.into())))
        .filter(|scaled| scaled.fract().is_zero())
        .and_then(|scaled| scaled.to_u128())
        .filter(|scaled| *scaled > 0)
        .context("TRUST amount must be a positive whole number of base units")?;
    let mut details = serde_json::to_value(parties)?;
    details["asset"] = json!(asset);
    details["amount"] = json!(amount.to_string());
    Ok(details)
}

/// Authenticated client for one 21 Travel Rule operator command.
pub struct TwentyOneTravelProvider {
    http: Client,
    graphql_url: Url,
    token: String,
}

impl TwentyOneTravelProvider {
    /// Obtain a bearer token using the deployment's OAuth client credentials.
    pub async fn authenticate(base_url: &str, client_id: &str, secret: &str) -> Result<Self> {
        let base = Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
            .context("Invalid 21 Travel Rule base URL")?;
        ensure!(
            base.scheme() == "https"
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none(),
            "21 Travel Rule base URL must use HTTPS and contain only a host and optional path"
        );
        let http = Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()?;
        let response: Value = http
            .post(base.join("auth/realms/21travel/protocol/openid-connect/token")?)
            .form(&[
                ("grant_type", "client_credentials"),
                ("scope", "openid"),
                ("client_id", client_id),
                ("client_secret", secret),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let token = response["access_token"]
            .as_str()
            .filter(|token| !token.is_empty())
            .context("Missing 21 Travel Rule access token")?;
        Ok(Self {
            http,
            graphql_url: base.join("graphql")?,
            token: token.into(),
        })
    }

    /// Return counterparty details, or JSON null when TRUST has no match.
    pub async fn lookup(&self, address: &str) -> Result<Value> {
        self.lookup_asset(address, Chain::Bitcoin, TRUST_BITCOIN).await
    }

    /// Look up the destination using the chain and DTI of the custody withdrawal.
    pub async fn lookup_asset(&self, address: &str, chain: Chain, asset: &str) -> Result<Value> {
        if chain == Chain::Bitcoin {
            address.parse::<bitcoin::Address<_>>().context("Expected a Bitcoin wallet address")?;
        } else {
            ensure!(address.is_ascii(), "Expected an ASCII wallet address");
            custody_common::validation::validate_address(chain, address).map_err(anyhow::Error::msg)?;
        }
        let mut data = self
            .graphql(LOOKUP, json!({ "address": address, "asset": asset }))
            .await?;
        data["trustd"]
            .as_object_mut()
            .and_then(|trustd| trustd.remove("lookupBulletinBoard"))
            .context("Missing TRUST lookup result")
    }

    /// Send Travel Rule details for a broadcast Bitcoin withdrawal and return the record ID.
    pub async fn send(&self, address: &str, mut details: Value) -> Result<String> {
        let amount = details["amount"].as_str().unwrap_or_default();
        let txid = details["txId"].as_str().unwrap_or_default();
        ensure!(
            amount.bytes().all(|b| b.is_ascii_digit())
                && amount.parse::<std::num::NonZeroU128>().is_ok(),
            "Expected a positive string amount in satoshis"
        );
        ensure!(
            txid.len() == 64 && txid.bytes().all(|b| b.is_ascii_hexdigit()),
            "Expected an on-chain Bitcoin txId"
        );
        ensure!(
            details["asset"].is_null() || details["asset"] == TRUST_BITCOIN,
            "Only Bitcoin is supported"
        );
        details["asset"] = json!(TRUST_BITCOIN);
        self.send_withdrawal(address, Chain::Bitcoin, details).await
    }

    /// Check routing and counterparty before a withdrawal or notification proceeds.
    pub async fn preflight(&self, address: &str, chain: Chain, asset: &str) -> Result<()> {
        let vasp = self.lookup_asset(address, chain, asset).await?;
        ensure!(
            vasp["id"].is_string() && vasp["lei"].is_string() && vasp["status"] == "TRUSTED",
            "No trusted TRUST counterparty with an LEI found"
        );
        self.check_routing().await
    }

    /// Check that the connected deployment restricts outgoing messages to TRUST.
    pub async fn check_routing(&self) -> Result<()> {
        // In 8.4.0 the send mutation checks the directory first. The server's
        // HTTP client rejects file URLs, leaving only the TRUST route.
        let data = self
            .graphql(
                "{ trpd { configuration { directoryUrl syncAddressDirectory } } }",
                json!({}),
            )
            .await?;
        let config = &data["trpd"]["configuration"];
        ensure!(
            config["directoryUrl"] == "file:///" && config["syncAddressDirectory"] == false,
            "Sending requires a 21 Travel Rule instance with the TRP directory disabled (see README)"
        );
        Ok(())
    }

    /// Notify the counterparty after custody has recorded the broadcast transaction ID.
    pub async fn send_withdrawal(&self, address: &str, chain: Chain, details: Value) -> Result<String> {
        let asset = details["asset"].as_str().context("Missing TRUST asset")?;
        self.preflight(address, chain, asset).await?;
        let data = self
            .graphql(SEND, json!({ "address": address, "ivms101": details }))
            .await
            .context(TrustSendUnconfirmed)?;
        let result = &data["autod"]["sendTxToAddrOwner"];
        ensure!(result["__typename"] == "TrustdTx", TrustSendUnconfirmed);
        result["trustd"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .context(TrustSendUnconfirmed)
    }

    /// Fetch a TRUST transaction and its beneficiary data by 21 Travel Rule record ID.
    pub async fn receive(&self, record_id: &str) -> Result<Option<Value>> {
        let data = self.records(json!({"id": {"like": exact_pattern(record_id)}})).await?;
        Ok(data.into_iter().map(|mut row| row["trustd"].take()).find(|row| row["id"] == record_id))
    }

    /// Find the receipt for the exact on-chain transfer without sending any data.
    pub async fn find_sent(&self, address: &str, chain: Chain, details: &Value) -> Result<Option<String>> {
        let tx_id = details["txId"].as_str().context("Missing TRUST txId")?;
        let address_match = if chain == Chain::Ethereum { "ilike" } else { "like" };
        let records = self.records(json!({
            "txid": {"like": exact_pattern(tx_id)},
            "type": ["SENT"],
            "beneficiary": {"walletAddress": {(address_match): exact_pattern(address)}}
        })).await?;
        let matching = records.iter().map(|row| &row["trustd"]).find(|row| {
            let found_address = row["beneficiary"]["walletAddress"].as_str().unwrap_or_default();
            let same_address = if chain == Chain::Ethereum { found_address.eq_ignore_ascii_case(address) }
                else { found_address == address };
            same_address && row["txId"] == tx_id && row["type"] == "SENT"
                && row["asset"] == details["asset"] && row["amount"] == details["amount"]
        });
        ensure!(matching.is_some() || records.len() < 2, "Could not disambiguate existing TRUST receipts");
        Ok(matching.and_then(|row| row["id"].as_str()).map(str::to_owned))
    }

    async fn records(&self, condition: Value) -> Result<Vec<Value>> {
        let mut data = self.graphql(RECORDS, json!({"filter": {"is": {"trustd": condition}}})).await?;
        serde_json::from_value(data["autod"]["txDetails"].take()).context("Missing TRUST transactions")
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let mut response: Value = self
            .http
            .post(self.graphql_url.clone())
            .bearer_auth(&self.token)
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        ensure!(
            (response["errors"].is_null()
                || response["errors"].as_array().is_some_and(Vec::is_empty))
                && response["data"].is_object(),
            "21 Travel Rule GraphQL request failed"
        );
        Ok(response["data"].take())
    }
}

fn exact_pattern(value: &str) -> String {
    value.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn trust_uses_withdrawal_chain_and_exact_base_units() {
        let parties = serde_json::from_value(json!({
            "originator": {"id": "originator-test", "customerIdentification": "customer-test",
                "address": {"addressLine": "1 Example St", "postalCode": "8001", "town": "Zurich", "country": "CH"}},
            "beneficiaryName": {"natural": {"firstName": "Jane", "lastName": "Example"}}
        })).unwrap();
        let eth = trust_withdrawal_details(Chain::Ethereum, "USDC", dec!(1.000001), &parties).unwrap();
        let tron = trust_withdrawal_details(Chain::Tron, "USDC", dec!(1.000001), &parties).unwrap();
        assert_eq!(eth["asset"], "DTICK9PW1MFH");
        assert_eq!(tron["asset"], "DTIT6TQ8GN7L");
        assert_eq!(eth["amount"], "1000001");
        assert!(trust_withdrawal_details(Chain::Ethereum, "USDC", dec!(0.0000001), &parties).is_err());
        assert!(trust_withdrawal_details(Chain::Bitcoin, "USDC", dec!(1), &parties).is_err());
    }

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
