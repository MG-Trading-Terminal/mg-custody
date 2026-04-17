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

        let response = self.client
            .post(&url)
            .header("Token", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "network": network,
                "asset": network,
                "address": address,
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Chainalysis unreachable: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %text, "Chainalysis API error");
            // Propagate the error so ScreeningService applies its
            // configured fallback (Block in production). The previous
            // "graceful degradation to Low" was a hidden safe-mode
            // bypass: every withdrawal passed screening when the
            // vendor was unavailable.
            anyhow::bail!("Chainalysis API returned HTTP {status}: {text}");
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Chainalysis JSON parse failed: {e}"))?;

        let risk_rating = body["rating"].as_str().unwrap_or("low");
        let risk_score = body["riskScore"].as_f64().unwrap_or(0.0);
        let exposures = body["exposures"].as_array()
            .map(|arr| arr.iter()
                .filter_map(|e| e["category"].as_str().map(String::from))
                .collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(ScreeningResult {
            address: address.to_string(),
            chain,
            risk_level: parse_risk_level(risk_rating, risk_score),
            risk_score,
            provider: "chainalysis".to_string(),
            categories: exposures,
            screened_at: Utc::now(),
            details: Some(format!("rating={risk_rating}")),
        })
    }

    async fn screen_transaction(&self, tx_hash: &str, chain: Chain) -> Result<ScreeningResult> {
        let network = chain_to_chainalysis_network(chain);
        tracing::info!(tx_hash, chain = ?chain, provider = "chainalysis", "Screening transaction");

        // POST /api/kyt/v2/users/{userId}/transfers
        let user_id = format!("custody_tx_{}", &tx_hash[..8.min(tx_hash.len())]);
        let url = format!("{}/users/{}/transfers", self.base_url, user_id);

        let response = self.client
            .post(&url)
            .header("Token", &self.api_key)
            .json(&serde_json::json!({
                "network": network,
                "asset": network,
                "transferReference": tx_hash,
                "direction": "received",
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Chainalysis unreachable: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Chainalysis API returned HTTP {status}: {text}");
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Chainalysis JSON parse failed: {e}"))?;
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

    fn provider_name(&self) -> &str {
        "chainalysis"
    }
}

// ---------------------------------------------------------------------------
// Elliptic Lens Provider
// ---------------------------------------------------------------------------

/// Elliptic Lens API integration.
pub struct EllipticProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl EllipticProvider {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: "https://api.elliptic.co/v2".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

/// Map custody Chain to Elliptic blockchain identifier.
fn chain_to_elliptic_blockchain(chain: Chain) -> &'static str {
    match chain {
        Chain::Ethereum => "ethereum",
        Chain::Bitcoin => "bitcoin",
        Chain::Tron => "tron",
        Chain::Solana => "solana",
    }
}

/// Risk-level bucketing helper for Elliptic's numeric risk_score.
fn score_to_risk_level(score: f64) -> RiskLevel {
    if score >= 0.8 { RiskLevel::Severe }
    else if score >= 0.6 { RiskLevel::High }
    else if score >= 0.3 { RiskLevel::Medium }
    else { RiskLevel::Low }
}

impl ScreeningProvider for EllipticProvider {
    async fn screen_address(&self, address: &str, chain: Chain) -> Result<ScreeningResult> {
        let blockchain = chain_to_elliptic_blockchain(chain);
        tracing::info!(address, chain = ?chain, provider = "elliptic", "Screening address");

        // POST /v2/wallet/synchronous
        let url = format!("{}/wallet/synchronous", self.base_url);

        let response = self.client
            .post(&url)
            .header("x-access-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "subject": {
                    "asset": blockchain,
                    "blockchain": blockchain,
                    "type": "address",
                    "hash": address,
                },
                "type": "wallet_exposure",
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Elliptic unreachable: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            // Err, not Ok(Low) — lets the service apply its fallback
            // (Block in production) instead of silently passing every
            // withdrawal through a "safe" Low-risk verdict.
            anyhow::bail!("Elliptic API returned HTTP {status}: {text}");
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Elliptic JSON parse failed: {e}"))?;

        let risk_score = body["risk_score"].as_f64().unwrap_or(0.0);
        let risk_level = score_to_risk_level(risk_score);

        let categories: Vec<String> = body["cluster_entities"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e["category"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ScreeningResult {
            address: address.to_string(),
            chain,
            risk_level,
            risk_score,
            provider: "elliptic".to_string(),
            categories,
            screened_at: Utc::now(),
            details: None,
        })
    }

    async fn screen_transaction(&self, tx_hash: &str, chain: Chain) -> Result<ScreeningResult> {
        let blockchain = chain_to_elliptic_blockchain(chain);
        tracing::info!(tx_hash, chain = ?chain, provider = "elliptic", "Screening transaction");

        let url = format!("{}/txn/synchronous", self.base_url);

        let response = self.client
            .post(&url)
            .header("x-access-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "subject": {
                    "asset": blockchain,
                    "blockchain": blockchain,
                    "type": "transaction",
                    "hash": tx_hash,
                },
                "type": "source_of_funds",
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Elliptic unreachable: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Elliptic API returned HTTP {status}: {text}");
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Elliptic JSON parse failed: {e}"))?;
        let risk_score = body["risk_score"].as_f64().unwrap_or(0.0);

        Ok(ScreeningResult {
            address: tx_hash.to_string(),
            chain,
            risk_level: score_to_risk_level(risk_score),
            risk_score,
            provider: "elliptic".to_string(),
            categories: vec![],
            screened_at: Utc::now(),
            details: None,
        })
    }

    fn provider_name(&self) -> &str {
        "elliptic"
    }
}

// ---------------------------------------------------------------------------
// Screening Cache + Fallback
// ---------------------------------------------------------------------------

/// Fallback behavior when screening provider is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreeningFallback {
    /// Block all transactions when provider is down (safe default).
    Block,
    /// Allow with flag for manual review.
    AllowAndFlag,
}

/// Cached screening with TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedScreening {
    pub result: ScreeningResult,
    pub expires_at: DateTime<Utc>,
}

impl CachedScreening {
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        now < self.expires_at
    }
}

/// Provider enum for dispatch (avoids dyn-incompatible async traits).
pub enum AnyScreeningProvider {
    Chainalysis(ChainalysisProvider),
    Elliptic(EllipticProvider),
}

impl AnyScreeningProvider {
    pub async fn screen_address(&self, address: &str, chain: Chain) -> Result<ScreeningResult> {
        match self {
            Self::Chainalysis(p) => p.screen_address(address, chain).await,
            Self::Elliptic(p) => p.screen_address(address, chain).await,
        }
    }

    pub fn provider_name(&self) -> &str {
        match self {
            Self::Chainalysis(p) => p.provider_name(),
            Self::Elliptic(p) => p.provider_name(),
        }
    }
}

/// Multi-provider screening service with caching and fallback.
pub struct ScreeningService {
    providers: Vec<AnyScreeningProvider>,
    fallback: ScreeningFallback,
    cache_ttl_hours: i64,
}

impl ScreeningService {
    pub fn new(fallback: ScreeningFallback, cache_ttl_hours: i64) -> Self {
        Self {
            providers: vec![],
            fallback,
            cache_ttl_hours,
        }
    }

    pub fn add_chainalysis(mut self, api_key: &str) -> Self {
        self.providers.push(AnyScreeningProvider::Chainalysis(ChainalysisProvider::new(api_key)));
        self
    }

    pub fn add_elliptic(mut self, api_key: &str) -> Self {
        self.providers.push(AnyScreeningProvider::Elliptic(EllipticProvider::new(api_key)));
        self
    }

    /// Screen an address using all configured providers.
    ///
    /// Returns the WORST (highest risk) result across providers. When
    /// no provider yields an `Ok` result (all errored, or none are
    /// configured) the service applies its `fallback` decision:
    ///
    ///   - `Block` (production default) — mark the result `High` with
    ///     a `provider_error` / `no_providers` category so the
    ///     withdrawal pipeline refuses it.
    ///   - `AllowAndFlag` — mark the result `Medium` so downstream
    ///     manual-review gates still trigger, but the withdrawal is
    ///     not blocked outright.
    ///
    /// The previous implementation silently returned `Low` for both
    /// "all providers unreachable" and "no providers configured".
    /// Either scenario is a hidden safe-mode bypass: AML screening
    /// would appear to have passed when it never ran.
    pub async fn screen_address(&self, address: &str, chain: Chain) -> ScreeningResult {
        if self.providers.is_empty() {
            return self.fallback_result(
                address,
                chain,
                "no_providers",
                "No screening providers configured — screening did not run",
            );
        }

        let mut worst: Option<ScreeningResult> = None;
        let mut last_error: Option<String> = None;

        for provider in &self.providers {
            match provider.screen_address(address, chain).await {
                Ok(result) => match &worst {
                    Some(w) if w.risk_level >= result.risk_level => {}
                    _ => worst = Some(result),
                },
                Err(e) => {
                    tracing::error!(
                        provider = provider.provider_name(),
                        error = %e,
                        "Screening provider error"
                    );
                    last_error = Some(format!("{}: {e}", provider.provider_name()));
                    if matches!(self.fallback, ScreeningFallback::Block) {
                        // Short-circuit: with Block policy, one failed
                        // provider is enough to deny the screening.
                        return self.fallback_result(
                            address,
                            chain,
                            "provider_error",
                            &format!("Provider unavailable, fallback=block: {e}"),
                        );
                    }
                }
            }
        }

        match worst {
            Some(w) => w,
            None => self.fallback_result(
                address,
                chain,
                "provider_error",
                last_error
                    .as_deref()
                    .unwrap_or("All screening providers returned errors"),
            ),
        }
    }

    fn fallback_result(
        &self,
        address: &str,
        chain: Chain,
        category: &str,
        detail: &str,
    ) -> ScreeningResult {
        let (risk_level, risk_score) = match self.fallback {
            ScreeningFallback::Block => (RiskLevel::High, 1.0),
            // AllowAndFlag → Medium triggers manual review but doesn't
            // block. This is only safe when operators actively monitor
            // the review queue.
            ScreeningFallback::AllowAndFlag => (RiskLevel::Medium, 0.5),
        };
        ScreeningResult {
            address: address.to_string(),
            chain,
            risk_level,
            risk_score,
            provider: "fallback".to_string(),
            categories: vec![category.to_string()],
            screened_at: Utc::now(),
            details: Some(detail.to_string()),
        }
    }

    /// Cache TTL for this service.
    pub fn cache_ttl_hours(&self) -> i64 {
        self.cache_ttl_hours
    }

    /// Fallback mode.
    pub fn fallback(&self) -> ScreeningFallback {
        self.fallback
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

    #[test]
    fn elliptic_provider_creation() {
        let provider = EllipticProvider::new("test-key");
        assert_eq!(provider.provider_name(), "elliptic");
        assert!(provider.base_url.contains("elliptic.co"));
    }

    #[test]
    fn screening_fallback_serialization() {
        assert_eq!(
            serde_json::to_string(&ScreeningFallback::Block).unwrap(),
            "\"block\""
        );
        assert_eq!(
            serde_json::to_string(&ScreeningFallback::AllowAndFlag).unwrap(),
            "\"allow_and_flag\""
        );
    }

    #[test]
    fn cached_screening_validity() {
        let now = Utc::now();
        let cached = CachedScreening {
            result: ScreeningResult {
                address: "0x123".to_string(),
                chain: Chain::Ethereum,
                risk_level: RiskLevel::Low,
                risk_score: 0.1,
                provider: "test".to_string(),
                categories: vec![],
                screened_at: now,
                details: None,
            },
            expires_at: now + chrono::Duration::hours(24),
        };
        assert!(cached.is_valid(now));
        assert!(cached.is_valid(now + chrono::Duration::hours(23)));
        assert!(!cached.is_valid(now + chrono::Duration::hours(25)));
    }

    #[test]
    fn screening_service_no_providers() {
        let service = ScreeningService::new(ScreeningFallback::Block, 24);
        assert_eq!(service.fallback(), ScreeningFallback::Block);
        assert_eq!(service.cache_ttl_hours(), 24);
    }

    #[test]
    fn screening_service_config() {
        let service = ScreeningService::new(ScreeningFallback::AllowAndFlag, 12);
        assert_eq!(service.fallback(), ScreeningFallback::AllowAndFlag);
        assert_eq!(service.cache_ttl_hours(), 12);
    }

    #[tokio::test]
    async fn no_providers_block_policy_returns_high() {
        // Production default: when no AML providers are configured,
        // the screening service MUST NOT return Low. The previous
        // implementation did exactly that and it's why "silent safe
        // mode" is in REALITY.md as a hidden bypass.
        let service = ScreeningService::new(ScreeningFallback::Block, 24);
        let result = service.screen_address("0xdead", Chain::Ethereum).await;
        assert_eq!(result.risk_level, RiskLevel::High);
        assert!(result.should_block());
        assert!(result.categories.contains(&"no_providers".to_string()));
    }

    #[tokio::test]
    async fn no_providers_allow_and_flag_returns_medium() {
        let service = ScreeningService::new(ScreeningFallback::AllowAndFlag, 24);
        let result = service.screen_address("0xdead", Chain::Ethereum).await;
        assert_eq!(result.risk_level, RiskLevel::Medium);
        assert!(!result.should_block());
        assert!(result.needs_review());
    }

    #[tokio::test]
    async fn provider_error_block_policy_returns_high() {
        // Point Chainalysis at an unresolvable host so the provider
        // fails fast. The service must respond with the Block fallback
        // (High risk), not silently pass the screening.
        let mut service = ScreeningService::new(ScreeningFallback::Block, 24);
        service.providers.push(AnyScreeningProvider::Chainalysis(
            ChainalysisProvider {
                api_key: "key".into(),
                base_url: "http://127.0.0.1:1".into(),
                client: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_millis(400))
                    .build()
                    .unwrap(),
            },
        ));
        let result = service.screen_address("0xdead", Chain::Ethereum).await;
        assert_eq!(result.risk_level, RiskLevel::High);
        assert!(result.categories.contains(&"provider_error".to_string()));
    }

    #[test]
    fn parse_risk_levels_from_ratings() {
        assert_eq!(parse_risk_level("highRisk", 0.9), RiskLevel::Severe);
        assert_eq!(parse_risk_level("severe", 0.95), RiskLevel::Severe);
        assert_eq!(parse_risk_level("high", 0.7), RiskLevel::High);
        assert_eq!(parse_risk_level("medium", 0.4), RiskLevel::Medium);
        assert_eq!(parse_risk_level("low", 0.1), RiskLevel::Low);
        assert_eq!(parse_risk_level("lowRisk", 0.05), RiskLevel::Low);
        // Fallback to score
        assert_eq!(parse_risk_level("unknown", 0.85), RiskLevel::Severe);
        assert_eq!(parse_risk_level("unknown", 0.65), RiskLevel::High);
        assert_eq!(parse_risk_level("unknown", 0.35), RiskLevel::Medium);
        assert_eq!(parse_risk_level("unknown", 0.1), RiskLevel::Low);
    }
}
