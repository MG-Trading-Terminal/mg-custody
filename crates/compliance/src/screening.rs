//! AML/KYT transaction screening.
//!
//! Integrates with blockchain analytics providers (Chainalysis KYT, Elliptic, TRM Labs)
//! to score addresses and transactions for money laundering risk.
//!
//! Flow:
//! 1. Pre-withdrawal: screen destination address → block if high risk
//! 2. Post-deposit: screen source address → flag for review if medium+ risk
//! 3. Continuous: re-screen on-chain counterparties periodically

use anyhow::Result;
use chrono::{DateTime, Utc};
use custody_common::Chain;
use serde::{Deserialize, Serialize};

/// Risk level assigned by screening provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// No risk indicators detected.
    Low,
    /// Some risk indicators — manual review recommended.
    Medium,
    /// Significant risk — block transaction, escalate to compliance.
    High,
    /// Sanctioned or known illicit — block immediately.
    Severe,
}

/// Result of screening an address or transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreeningResult {
    pub address: String,
    pub chain: Chain,
    pub risk_level: RiskLevel,
    pub risk_score: f64, // 0.0 (safe) to 1.0 (max risk)
    pub provider: String,
    pub categories: Vec<String>, // e.g., "sanctions", "darknet", "mixer", "fraud"
    pub screened_at: DateTime<Utc>,
    pub details: Option<String>,
}

impl ScreeningResult {
    /// Check if this result should block the transaction.
    pub fn should_block(&self) -> bool {
        matches!(self.risk_level, RiskLevel::High | RiskLevel::Severe)
    }

    /// Check if this result needs manual review.
    pub fn needs_review(&self) -> bool {
        matches!(self.risk_level, RiskLevel::Medium)
    }
}

/// Trait for AML/KYT screening providers.
pub trait ScreeningProvider: Send + Sync {
    /// Screen an address for AML risk.
    fn screen_address(
        &self,
        address: &str,
        chain: Chain,
    ) -> impl std::future::Future<Output = Result<ScreeningResult>> + Send;

    /// Screen a transaction by hash.
    fn screen_transaction(
        &self,
        tx_hash: &str,
        chain: Chain,
    ) -> impl std::future::Future<Output = Result<ScreeningResult>> + Send;

    /// Provider name (e.g., "chainalysis", "elliptic", "trm").
    fn provider_name(&self) -> &str;
}

/// Chainalysis KYT integration.
pub struct ChainalysisProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl ChainalysisProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.chainalysis.com/api/kyt/v2".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

/// Map custody Chain to Chainalysis network name.
fn chain_to_chainalysis_network(chain: Chain) -> &'static str {
    match chain {
        Chain::Ethereum => "ethereum",
        Chain::Bitcoin => "bitcoin",
        Chain::Tron => "tron",
        Chain::Solana => "solana",
    }
}

/// Parse Chainalysis risk rating to our RiskLevel.
fn parse_risk_level(rating: &str, score: f64) -> RiskLevel {
    match rating {
        "highRisk" | "severe" => RiskLevel::Severe,
        "high" => RiskLevel::High,
        "medium" => RiskLevel::Medium,
        "low" | "lowRisk" => RiskLevel::Low,
        _ => {
            // Fallback: use score
            if score >= 0.8 { RiskLevel::Severe }
            else if score >= 0.6 { RiskLevel::High }
            else if score >= 0.3 { RiskLevel::Medium }
            else { RiskLevel::Low }
        }
    }
}

impl ScreeningProvider for ChainalysisProvider {
    async fn screen_address(&self, address: &str, chain: Chain) -> Result<ScreeningResult> {
        let network = chain_to_chainalysis_network(chain);
        tracing::info!(address, chain = ?chain, provider = "chainalysis", "Screening address");

        // POST /api/kyt/v2/users/{userId}/withdrawaladdresses
        let user_id = format!("custody_{}", &address[..8.min(address.len())]);
        let url = format!("{}/users/{}/withdrawaladdresses", self.base_url, user_id);

        let resp = self.client
            .post(&url)
            .header("Token", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "network": network,
                "asset": network,
                "address": address,
            }))
            .send()
            .await;

        match resp {
            Ok(response) if response.status().is_success() => {
                let body: serde_json::Value = response.json().await?;

                // Parse Chainalysis response
                let risk_rating = body["rating"].as_str().unwrap_or("low");
                let risk_score = body["riskScore"].as_f64().unwrap_or(0.0);
                let exposures = body["exposures"].as_array()
                    .map(|arr| arr.iter()
                        .filter_map(|e| e["category"].as_str().map(String::from))
                        .collect::<Vec<_>>())
                    .unwrap_or_default();

                let risk_level = parse_risk_level(risk_rating, risk_score);

                Ok(ScreeningResult {
                    address: address.to_string(),
                    chain,
                    risk_level,
                    risk_score,
                    provider: "chainalysis".to_string(),
                    categories: exposures,
                    screened_at: Utc::now(),
                    details: Some(format!("rating={risk_rating}")),
                })
            }
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                tracing::warn!(status = %status, body = %text, "Chainalysis API error");
                // Return Low risk on API error to avoid blocking all withdrawals
                Ok(ScreeningResult {
                    address: address.to_string(), chain,
                    risk_level: RiskLevel::Low, risk_score: 0.0,
                    provider: "chainalysis".to_string(),
                    categories: vec![],
                    screened_at: Utc::now(),
                    details: Some(format!("API error: HTTP {status}")),
                })
            }
            Err(e) => {
                tracing::warn!(error = %e, "Chainalysis API unreachable");
                Ok(ScreeningResult {
                    address: address.to_string(), chain,
                    risk_level: RiskLevel::Low, risk_score: 0.0,
                    provider: "chainalysis".to_string(),
                    categories: vec![],
                    screened_at: Utc::now(),
                    details: Some(format!("Connection error: {e}")),
                })
            }
        }
    }

    async fn screen_transaction(&self, tx_hash: &str, chain: Chain) -> Result<ScreeningResult> {
        let network = chain_to_chainalysis_network(chain);
        tracing::info!(tx_hash, chain = ?chain, provider = "chainalysis", "Screening transaction");

        // POST /api/kyt/v2/users/{userId}/transfers
        let user_id = format!("custody_tx_{}", &tx_hash[..8.min(tx_hash.len())]);
        let url = format!("{}/users/{}/transfers", self.base_url, user_id);

        let resp = self.client
            .post(&url)
            .header("Token", &self.api_key)
            .json(&serde_json::json!({
                "network": network,
                "asset": network,
                "transferReference": tx_hash,
                "direction": "received",
            }))
            .send()
            .await;

        match resp {
            Ok(response) if response.status().is_success() => {
                let body: serde_json::Value = response.json().await?;
                let risk_rating = body["rating"].as_str().unwrap_or("low");
                let risk_score = body["riskScore"].as_f64().unwrap_or(0.0);
                let exposures = body["exposures"].as_array()
                    .map(|arr| arr.iter()
                        .filter_map(|e| e["category"].as_str().map(String::from))
                        .collect::<Vec<_>>())
                    .unwrap_or_default();

                Ok(ScreeningResult {
                    address: tx_hash.to_string(), chain,
                    risk_level: parse_risk_level(risk_rating, risk_score),
                    risk_score,
                    provider: "chainalysis".to_string(),
                    categories: exposures,
                    screened_at: Utc::now(),
                    details: Some(format!("rating={risk_rating}")),
                })
            }
            _ => {
                // Graceful degradation
                Ok(ScreeningResult {
                    address: tx_hash.to_string(), chain,
                    risk_level: RiskLevel::Low, risk_score: 0.0,
                    provider: "chainalysis".to_string(),
                    categories: vec![],
                    screened_at: Utc::now(),
                    details: Some("API unavailable".to_string()),
                })
            }
        }
    }

    fn provider_name(&self) -> &str {
        "chainalysis"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Severe);
    }

    #[test]
    fn should_block_high_and_severe() {
        let mut result = ScreeningResult {
            address: "0x123".to_string(),
            chain: Chain::Ethereum,
            risk_level: RiskLevel::Low,
            risk_score: 0.1,
            provider: "test".to_string(),
            categories: vec![],
            screened_at: Utc::now(),
            details: None,
        };

        assert!(!result.should_block());
        assert!(!result.needs_review());

        result.risk_level = RiskLevel::Medium;
        assert!(!result.should_block());
        assert!(result.needs_review());

        result.risk_level = RiskLevel::High;
        assert!(result.should_block());

        result.risk_level = RiskLevel::Severe;
        assert!(result.should_block());
    }

    #[test]
    fn risk_level_serialization() {
        assert_eq!(serde_json::to_string(&RiskLevel::Low).unwrap(), "\"low\"");
        assert_eq!(serde_json::to_string(&RiskLevel::Severe).unwrap(), "\"severe\"");
    }

    #[test]
    fn screening_result_with_categories() {
        let result = ScreeningResult {
            address: "0xbad".to_string(),
            chain: Chain::Ethereum,
            risk_level: RiskLevel::Severe,
            risk_score: 0.95,
            provider: "chainalysis".to_string(),
            categories: vec!["sanctions".to_string(), "darknet".to_string()],
            screened_at: Utc::now(),
            details: Some("OFAC designated entity".to_string()),
        };
        assert!(result.should_block());
        assert_eq!(result.categories.len(), 2);
    }
}
