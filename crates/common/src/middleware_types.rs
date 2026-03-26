//! Middleware and infrastructure types — idempotency, analytics, webhook signing,
//! retry policies, circuit breaker configuration.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Idempotency
// ---------------------------------------------------------------------------

/// An idempotency record — stores the response for a given key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub key: String,
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub response_body: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl IdempotencyRecord {
    /// Create a new record with default 24h expiry.
    pub fn new(key: &str, method: &str, path: &str, status: u16, body: &str) -> Self {
        let now = Utc::now();
        Self {
            key: key.into(),
            method: method.into(),
            path: path.into(),
            status_code: status,
            response_body: body.into(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(24),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

// ---------------------------------------------------------------------------
// Request analytics
// ---------------------------------------------------------------------------

/// API usage metrics per key/endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageRecord {
    pub api_key_id: Option<Uuid>,
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub latency_ms: u32,
    pub request_size_bytes: u32,
    pub response_size_bytes: u32,
    pub timestamp: DateTime<Utc>,
}

/// Aggregated API usage summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUsageSummary {
    pub period: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f32,
    pub p95_latency_ms: f32,
    pub p99_latency_ms: f32,
    pub top_endpoints: Vec<EndpointUsage>,
    pub top_errors: Vec<ErrorUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointUsage {
    pub method: String,
    pub path: String,
    pub count: u64,
    pub avg_latency_ms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorUsage {
    pub status_code: u16,
    pub count: u64,
    pub last_seen: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Webhook signing
// ---------------------------------------------------------------------------

/// Configuration for signing outbound webhook payloads.
#[derive(Debug, Clone)]
pub struct WebhookSigner {
    secret: Vec<u8>,
}

impl WebhookSigner {
    pub fn new(secret: &[u8]) -> Self {
        Self { secret: secret.to_vec() }
    }

    /// Compute HMAC-SHA256 signature for a payload.
    pub fn sign(&self, payload: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<sha2::Sha256>;

        let mut mac = HmacSha256::new_from_slice(&self.secret).unwrap();
        mac.update(payload);
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Verify a signature against a payload.
    pub fn verify(&self, payload: &[u8], signature: &str) -> bool {
        let expected = self.sign(payload);
        // Constant-time comparison
        expected.len() == signature.len()
            && expected.as_bytes().iter().zip(signature.as_bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

/// Signed webhook delivery payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedWebhookPayload {
    pub event: String,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub webhook_id: Uuid,
}

// ---------------------------------------------------------------------------
// Retry policies
// ---------------------------------------------------------------------------

/// Retry policy for chain operations (RPC calls, broadcasts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub chain: Chain,
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f32,
    /// Whether to retry on timeout.
    pub retry_on_timeout: bool,
    /// Whether to retry on rate limit.
    pub retry_on_rate_limit: bool,
}

impl RetryPolicy {
    pub fn default_for_chain(chain: Chain) -> Self {
        match chain {
            Chain::Ethereum => Self {
                chain, max_retries: 3, initial_delay_ms: 1000,
                max_delay_ms: 30_000, backoff_multiplier: 2.0,
                retry_on_timeout: true, retry_on_rate_limit: true,
            },
            Chain::Bitcoin => Self {
                chain, max_retries: 5, initial_delay_ms: 2000,
                max_delay_ms: 60_000, backoff_multiplier: 2.0,
                retry_on_timeout: true, retry_on_rate_limit: true,
            },
            Chain::Tron => Self {
                chain, max_retries: 3, initial_delay_ms: 500,
                max_delay_ms: 15_000, backoff_multiplier: 2.0,
                retry_on_timeout: true, retry_on_rate_limit: true,
            },
            Chain::Solana => Self {
                chain, max_retries: 3, initial_delay_ms: 500,
                max_delay_ms: 10_000, backoff_multiplier: 1.5,
                retry_on_timeout: true, retry_on_rate_limit: true,
            },
        }
    }

    /// Calculate delay for a given retry attempt.
    pub fn delay_for_attempt(&self, attempt: u32) -> u64 {
        let delay = self.initial_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32) as f64;
        (delay as u64).min(self.max_delay_ms)
    }
}

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

/// Circuit breaker state — persisted in DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerState {
    pub single_tx_limit: Decimal,
    pub hourly_limit: Decimal,
    pub daily_limit: Decimal,
    pub current_hourly_outflow: Decimal,
    pub current_daily_outflow: Decimal,
    pub is_tripped: bool,
    pub tripped_at: Option<DateTime<Utc>>,
    pub trip_reason: Option<String>,
    pub last_reset_at: DateTime<Utc>,
}

impl CircuitBreakerState {
    pub fn default_limits() -> Self {
        Self {
            single_tx_limit: Decimal::new(500_000, 0),
            hourly_limit: Decimal::new(10_000_000, 0),
            daily_limit: Decimal::new(50_000_000, 0),
            current_hourly_outflow: Decimal::ZERO,
            current_daily_outflow: Decimal::ZERO,
            is_tripped: false,
            tripped_at: None,
            trip_reason: None,
            last_reset_at: Utc::now(),
        }
    }

    /// Check if a withdrawal amount would trip the breaker.
    pub fn check(&self, amount: Decimal) -> Result<(), CircuitBreakerError> {
        if self.is_tripped {
            return Err(CircuitBreakerError::AlreadyTripped);
        }
        if amount > self.single_tx_limit {
            return Err(CircuitBreakerError::SingleTxLimit {
                amount, limit: self.single_tx_limit,
            });
        }
        if self.current_hourly_outflow + amount > self.hourly_limit {
            return Err(CircuitBreakerError::HourlyLimit {
                current: self.current_hourly_outflow, amount, limit: self.hourly_limit,
            });
        }
        if self.current_daily_outflow + amount > self.daily_limit {
            return Err(CircuitBreakerError::DailyLimit {
                current: self.current_daily_outflow, amount, limit: self.daily_limit,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum CircuitBreakerError {
    AlreadyTripped,
    SingleTxLimit { amount: Decimal, limit: Decimal },
    HourlyLimit { current: Decimal, amount: Decimal, limit: Decimal },
    DailyLimit { current: Decimal, amount: Decimal, limit: Decimal },
}

impl std::fmt::Display for CircuitBreakerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyTripped => write!(f, "Circuit breaker is tripped"),
            Self::SingleTxLimit { amount, limit } => {
                write!(f, "Single tx {amount} exceeds limit {limit}")
            }
            Self::HourlyLimit { current, amount, limit } => {
                write!(f, "Hourly outflow {current} + {amount} exceeds limit {limit}")
            }
            Self::DailyLimit { current, amount, limit } => {
                write!(f, "Daily outflow {current} + {amount} exceeds limit {limit}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- Idempotency ---

    #[test]
    fn idempotency_record_not_expired() {
        let record = IdempotencyRecord::new("key-1", "POST", "/v1/withdrawals", 202, "{}");
        assert!(!record.is_expired());
    }

    #[test]
    fn idempotency_record_expired() {
        let mut record = IdempotencyRecord::new("key-1", "POST", "/v1/withdrawals", 202, "{}");
        record.expires_at = Utc::now() - chrono::Duration::hours(1);
        assert!(record.is_expired());
    }

    // --- Webhook signing ---

    #[test]
    fn webhook_sign_and_verify() {
        let signer = WebhookSigner::new(b"my_secret_key");
        let payload = b"{\"event\":\"deposit_confirmed\"}";
        let sig = signer.sign(payload);
        assert!(signer.verify(payload, &sig));
    }

    #[test]
    fn webhook_verify_wrong_signature() {
        let signer = WebhookSigner::new(b"my_secret_key");
        let payload = b"test";
        assert!(!signer.verify(payload, "wrong_signature"));
    }

    #[test]
    fn webhook_different_secrets_different_signatures() {
        let signer1 = WebhookSigner::new(b"secret1");
        let signer2 = WebhookSigner::new(b"secret2");
        let payload = b"same_payload";
        assert_ne!(signer1.sign(payload), signer2.sign(payload));
    }

    #[test]
    fn webhook_signature_hex_format() {
        let signer = WebhookSigner::new(b"key");
        let sig = signer.sign(b"data");
        assert_eq!(sig.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- Retry policies ---

    #[test]
    fn retry_delay_exponential() {
        let policy = RetryPolicy::default_for_chain(Chain::Ethereum);
        assert_eq!(policy.delay_for_attempt(0), 1000);
        assert_eq!(policy.delay_for_attempt(1), 2000);
        assert_eq!(policy.delay_for_attempt(2), 4000);
    }

    #[test]
    fn retry_delay_capped() {
        let policy = RetryPolicy::default_for_chain(Chain::Ethereum);
        let delay = policy.delay_for_attempt(20);
        assert!(delay <= policy.max_delay_ms);
    }

    #[test]
    fn retry_policy_per_chain() {
        let eth = RetryPolicy::default_for_chain(Chain::Ethereum);
        let btc = RetryPolicy::default_for_chain(Chain::Bitcoin);
        assert_eq!(eth.max_retries, 3);
        assert_eq!(btc.max_retries, 5); // BTC blocks slower, need more retries
    }

    #[test]
    fn retry_solana_faster_backoff() {
        let sol = RetryPolicy::default_for_chain(Chain::Solana);
        assert_eq!(sol.backoff_multiplier, 1.5); // faster recovery for fast chain
        assert_eq!(sol.initial_delay_ms, 500);
    }

    // --- Circuit breaker ---

    #[test]
    fn circuit_breaker_allows_normal_tx() {
        let cb = CircuitBreakerState::default_limits();
        assert!(cb.check(dec!(1000)).is_ok());
    }

    #[test]
    fn circuit_breaker_blocks_over_single_limit() {
        let cb = CircuitBreakerState::default_limits();
        assert!(cb.check(dec!(600000)).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_over_hourly_limit() {
        let mut cb = CircuitBreakerState::default_limits();
        cb.current_hourly_outflow = dec!(9_999_000);
        assert!(cb.check(dec!(2000)).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_over_daily_limit() {
        let mut cb = CircuitBreakerState::default_limits();
        cb.current_daily_outflow = dec!(49_999_000);
        assert!(cb.check(dec!(2000)).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_when_tripped() {
        let mut cb = CircuitBreakerState::default_limits();
        cb.is_tripped = true;
        assert!(cb.check(dec!(1)).is_err());
    }

    #[test]
    fn circuit_breaker_error_display() {
        let err = CircuitBreakerError::SingleTxLimit {
            amount: dec!(600000), limit: dec!(500000),
        };
        let msg = err.to_string();
        assert!(msg.contains("600000"));
        assert!(msg.contains("500000"));
    }

    // --- Analytics ---

    #[test]
    fn api_usage_summary_structure() {
        let summary = ApiUsageSummary {
            period: "last_24h".into(),
            total_requests: 10000,
            successful_requests: 9500,
            failed_requests: 500,
            avg_latency_ms: 45.2,
            p95_latency_ms: 120.0,
            p99_latency_ms: 350.0,
            top_endpoints: vec![EndpointUsage {
                method: "GET".into(), path: "/v1/transactions".into(),
                count: 5000, avg_latency_ms: 30.0,
            }],
            top_errors: vec![ErrorUsage {
                status_code: 429, count: 200, last_seen: Utc::now(),
            }],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("10000"));
        assert!(json.contains("p95_latency_ms"));
    }
}
