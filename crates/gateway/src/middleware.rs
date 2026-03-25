//! Axum middleware: HMAC auth, rate limiting, request logging.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::auth;
use crate::rate_limit::RateLimiter;

/// Shared middleware state.
#[derive(Clone)]
pub struct MiddlewareState {
    pub hmac_secret: Vec<u8>,
    pub rate_limiter: Arc<RateLimiter>,
    /// Skip auth for these paths (health, metrics).
    pub public_paths: Vec<String>,
}

impl MiddlewareState {
    pub fn new(hmac_secret: &[u8]) -> Self {
        Self {
            hmac_secret: hmac_secret.to_vec(),
            rate_limiter: Arc::new(RateLimiter::default_api()),
            public_paths: vec![
                "/health".into(),
                "/health/chains".into(),
                "/health/deep".into(),
                "/metrics".into(),
            ],
        }
    }
}

/// HMAC authentication middleware.
///
/// Verifies X-Custody-Signature header on all non-public endpoints.
/// Rejects requests with missing/invalid signature or stale timestamp (>30s).
pub async fn hmac_auth(
    State(mw): State<MiddlewareState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();

    // Skip auth for public paths
    if mw.public_paths.iter().any(|p| path.starts_with(p)) {
        return Ok(next.run(request).await);
    }

    // Extract headers
    let timestamp = request.headers()
        .get("X-Custody-Timestamp")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let signature = request.headers()
        .get("X-Custody-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let method = request.method().as_str().to_string();

    // Check timestamp staleness (30 seconds)
    if !timestamp.is_empty() {
        if let Ok(ts) = timestamp.parse::<i64>() {
            let now = chrono::Utc::now().timestamp();
            if (now - ts).abs() > 30 {
                tracing::warn!(path = %path, "Stale request timestamp");
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    // Extract body bytes for HMAC verification
    if !signature.is_empty() && !timestamp.is_empty() {
        let (parts, body) = request.into_parts();
        let body_bytes = axum::body::to_bytes(body, 1024 * 1024) // 1MB max
            .await
            .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;

        let valid = auth::verify_signature(
            &mw.hmac_secret,
            &method,
            &path,
            &timestamp,
            &body_bytes,
            &signature,
        );

        if !valid {
            tracing::warn!(path = %path, method = %method, "Invalid HMAC signature");
            return Err(StatusCode::UNAUTHORIZED);
        }

        // Reconstruct the request with the body
        let request = Request::from_parts(parts, Body::from(body_bytes));
        custody_common::prometheus_metrics::DEPOSITS_TOTAL
            .with_label_values(&["all", "request"]).inc();
        return Ok(next.run(request).await);
    }

    // No signature provided — reject in production mode
    if mw.hmac_secret.is_empty() {
        // No secret configured, allow (dev mode)
        return Ok(next.run(request).await);
    }

    tracing::warn!(path = %path, "Missing HMAC signature");
    Err(StatusCode::UNAUTHORIZED)
}

/// Rate limiting middleware.
pub async fn rate_limit(
    State(mw): State<MiddlewareState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let key = request.headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    match mw.rate_limiter.check(&key) {
        Ok(remaining) => {
            let mut response = next.run(request).await;
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            Ok(response)
        }
        Err(retry_after) => {
            tracing::warn!(key = %key, retry_secs = retry_after.as_secs(), "Rate limited");
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middleware_state_creation() {
        let mw = MiddlewareState::new(b"test-secret");
        assert_eq!(mw.hmac_secret, b"test-secret");
        assert!(mw.public_paths.contains(&"/health".to_string()));
    }

    #[test]
    fn public_paths_skip_auth() {
        let mw = MiddlewareState::new(b"secret");
        assert!(mw.public_paths.iter().any(|p| "/health".starts_with(p)));
        assert!(mw.public_paths.iter().any(|p| "/metrics".starts_with(p)));
        assert!(!mw.public_paths.iter().any(|p| "/v1/addresses".starts_with(p)));
    }
}
