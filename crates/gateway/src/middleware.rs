//! Axum middleware: HMAC auth, rate limiting, request logging.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::time::Instant;

use crate::auth;
use crate::rate_limit::{RateLimiter, UserRateLimiter, ip_key, user_key};
use crate::rbac_middleware::AuthContext;

/// Shared middleware state.
#[derive(Clone)]
pub struct MiddlewareState {
    pub hmac_secret: Vec<u8>,
    pub rate_limiter: Arc<RateLimiter>,
    pub user_rate_limiter: Arc<UserRateLimiter>,
    /// Skip auth for these paths (health, metrics).
    pub public_paths: Vec<String>,
}

impl MiddlewareState {
    pub fn new(hmac_secret: &[u8]) -> Self {
        Self {
            hmac_secret: hmac_secret.to_vec(),
            rate_limiter: Arc::new(RateLimiter::default_api()),
            user_rate_limiter: Arc::new(UserRateLimiter::default_user()),
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
    if !timestamp.is_empty()
        && let Ok(ts) = timestamp.parse::<i64>() {
            let now = chrono::Utc::now().timestamp();
            if (now - ts).abs() > 30 {
                tracing::warn!(path = %path, "Stale request timestamp");
                return Err(StatusCode::UNAUTHORIZED);
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

/// Security headers middleware for dashboard pages.
///
/// CSP: default-src 'self' — no external resources (air-gap compatible).
/// X-Frame-Options: DENY — prevent clickjacking of approval flows.
/// HSTS: 1 year with includeSubDomains.
pub async fn security_headers(
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss:; img-src 'self' data:; font-src 'self'"
            .parse().unwrap(),
    );
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains".parse().unwrap(),
    );
    headers.insert("Referrer-Policy", "same-origin".parse().unwrap());
    headers.insert(
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=()".parse().unwrap(),
    );

    response
}

/// Rate limiting middleware.
///
/// For authenticated requests (Bearer token already resolved into `AuthContext`
/// by `rbac_auth`), applies per-user sliding window limits keyed on user_id with
/// per-role caps.  For unauthenticated requests, falls back to the IP-based token
/// bucket.
///
/// Adds `X-RateLimit-Limit` and `X-RateLimit-Reset` headers to every response.
/// Returns 429 with `Retry-After` header when the limit is exceeded.
pub async fn rate_limit(
    State(mw): State<MiddlewareState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    use crate::rate_limit::RoleLimit;

    // Attempt to read the AuthContext injected by rbac_auth (runs before us in the stack).
    let auth_ctx = request.extensions().get::<AuthContext>().cloned();

    if let Some(ctx) = auth_ctx {
        // Authenticated — use per-user sliding window.
        let role_ordinal = ctx.role as u8;
        let limit = RoleLimit::for_role_ordinal(role_ordinal).max_per_minute;
        let key = user_key(&ctx.user_id.to_string());

        // Reset epoch: seconds until the start of the next window (up to 60).
        match mw.user_rate_limiter.check(&key, limit) {
            Ok((remaining, cap)) => {
                let mut response = next.run(request).await;
                let headers = response.headers_mut();
                if let (Ok(rem_hv), Ok(cap_hv), Ok(reset_hv)) = (
                    remaining.to_string().parse(),
                    cap.to_string().parse(),
                    "60".parse(),
                ) {
                    headers.insert("X-RateLimit-Remaining", rem_hv);
                    headers.insert("X-RateLimit-Limit", cap_hv);
                    headers.insert("X-RateLimit-Reset", reset_hv);
                }
                Ok(response)
            }
            Err(retry_secs) => {
                tracing::warn!(
                    user = %ctx.email,
                    role = ?ctx.role,
                    retry_secs,
                    "Per-user rate limit exceeded"
                );
                let mut response = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(Body::empty())
                    .unwrap();
                if let Ok(hv) = retry_secs.to_string().parse() {
                    response.headers_mut().insert("Retry-After", hv);
                }
                if let (Ok(lim_hv), Ok(reset_hv)) =
                    (limit.to_string().parse(), retry_secs.to_string().parse())
                {
                    response.headers_mut().insert("X-RateLimit-Limit", lim_hv);
                    response.headers_mut().insert("X-RateLimit-Reset", reset_hv);
                }
                Ok(response)
            }
        }
    } else {
        // Unauthenticated — fall back to IP-based token bucket.
        let key = request
            .headers()
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .map(|s| ip_key(s.split(',').next().unwrap_or("unknown").trim()))
            .unwrap_or_else(|| ip_key("unknown"));

        match mw.rate_limiter.check(&key) {
            Ok(remaining) => {
                let mut response = next.run(request).await;
                let headers = response.headers_mut();
                if let (Ok(rem_hv), Ok(cap_hv), Ok(reset_hv)) = (
                    remaining.to_string().parse(),
                    "100".parse(),
                    "60".parse(),
                ) {
                    headers.insert("X-RateLimit-Remaining", rem_hv);
                    headers.insert("X-RateLimit-Limit", cap_hv);
                    headers.insert("X-RateLimit-Reset", reset_hv);
                }
                Ok(response)
            }
            Err(retry_after) => {
                tracing::warn!(key = %key, retry_secs = retry_after.as_secs(), "IP rate limit exceeded");
                let mut response = Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(Body::empty())
                    .unwrap();
                if let Ok(hv) = retry_after.as_secs().to_string().parse() {
                    response.headers_mut().insert("Retry-After", hv);
                }
                Ok(response)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Request logger
// ---------------------------------------------------------------------------

/// Whether debug mode is enabled via `CUSTODY_DEBUG=true`.
fn debug_mode() -> bool {
    std::env::var("CUSTODY_DEBUG").as_deref() == Ok("true")
}

/// SHA-256 of the request body, hex-encoded.  Used in debug mode to
/// correlate logged hashes with raw payloads without ever logging the body.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

/// Structured request/response logging middleware.
///
/// Logs method, path, status code, elapsed time, and request ID at INFO level
/// after every request.  In debug mode (`CUSTODY_DEBUG=true`) also logs the
/// body SHA-256 (not the raw body — sensitive data is never logged) and adds
/// `X-Debug-RequestId` and `X-Debug-Elapsed-Ms` response headers.
///
/// Prometheus counters and latency histograms are updated unconditionally.
pub async fn request_logger(request: Request<Body>, next: Next) -> Response {
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let request_id = request
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let debug = debug_mode();

    // In debug mode: extract body bytes, hash them, rebuild the request.
    // We limit to 1 MB to prevent memory exhaustion on large uploads.
    let (request, body_hash) = if debug {
        let (parts, body) = request.into_parts();
        let bytes = axum::body::to_bytes(body, 1024 * 1024)
            .await
            .unwrap_or_default();
        let hash = sha256_hex(&bytes);
        let rebuilt = Request::from_parts(parts, Body::from(bytes));
        (rebuilt, Some(hash))
    } else {
        (request, None)
    };

    let start = Instant::now();
    let mut response = next.run(request).await;
    let elapsed = start.elapsed();

    let status = response.status().as_u16();
    let elapsed_ms = elapsed.as_millis() as u64;

    tracing::info!(
        method = %method,
        path = %path,
        status,
        request_id = %request_id,
        elapsed_ms,
        event = "api_request",
        "API request completed"
    );

    if debug {
        if let Some(hash) = &body_hash {
            tracing::debug!(
                method = %method,
                path = %path,
                body_sha256 = %hash,
                response_status = status,
                "API request debug"
            );
        }

        let headers = response.headers_mut();
        if let Ok(v) = request_id.parse() {
            headers.insert("X-Debug-RequestId", v);
        }
        if let Ok(v) = elapsed_ms.to_string().parse() {
            headers.insert("X-Debug-Elapsed-Ms", v);
        }
    }

    // Update Prometheus metrics.
    custody_common::prometheus_metrics::API_REQUEST_LATENCY
        .with_label_values(&[&method, &path])
        .observe(elapsed.as_secs_f64());
    custody_common::prometheus_metrics::API_REQUEST_COUNT
        .with_label_values(&[&method, &status.to_string()])
        .inc();

    response
}

/// API versioning headers middleware.
///
/// Adds `X-API-Version` and `X-Powered-By` to every response so that callers
/// can detect the running version without parsing the OpenAPI spec.
///
/// For any path that starts with a deprecated version prefix (currently `"/v0"`),
/// the `Deprecation` and `Sunset` headers are also added per RFC 8594.
pub async fn api_version_headers(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Version identity headers — always present.
    if let Ok(v) = "1.0".parse() {
        headers.insert("X-API-Version", v);
    }
    if let Ok(v) = "MG-Custody/1.0".parse() {
        headers.insert("X-Powered-By", v);
    }

    // Deprecation headers for any legacy `/v0` prefixed routes.
    if path.starts_with("/v0") {
        if let Ok(v) = "true".parse() {
            headers.insert("Deprecation", v);
        }
        if let Ok(v) = "2026-06-01".parse() {
            headers.insert("Sunset", v);
        }
        if let Ok(v) = "</v1/openapi.json>; rel=\"successor-version\"".parse() {
            headers.insert("Link", v);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middleware_state_creation() {
        let mw = MiddlewareState::new(b"test-secret");
        assert_eq!(mw.hmac_secret, b"test-secret");
        assert!(mw.public_paths.contains(&"/health".to_string()));
        // Both rate limiters are present
        drop(mw.rate_limiter);
        drop(mw.user_rate_limiter);
    }

    #[test]
    fn public_paths_skip_auth() {
        let mw = MiddlewareState::new(b"secret");
        assert!(mw.public_paths.iter().any(|p| "/health".starts_with(p)));
        assert!(mw.public_paths.iter().any(|p| "/metrics".starts_with(p)));
        assert!(!mw.public_paths.iter().any(|p| "/v1/addresses".starts_with(p)));
    }

    // --- API versioning ---

    #[test]
    fn deprecation_headers_for_v0_path() {
        let deprecated_headers = custody_common::api_versioning::deprecation_headers("v0");
        assert!(!deprecated_headers.is_empty(), "v0 must have deprecation headers");
        assert!(
            deprecated_headers.iter().any(|(k, _)| k == "Deprecation"),
            "missing Deprecation header for v0"
        );
        assert!(
            deprecated_headers.iter().any(|(k, _)| k == "Sunset"),
            "missing Sunset header for v0"
        );
    }

    #[test]
    fn deprecation_headers_absent_for_v1() {
        let headers = custody_common::api_versioning::deprecation_headers("v1");
        assert!(
            headers.is_empty(),
            "v1 must not have deprecation headers, got: {:?}",
            headers
        );
    }

    #[test]
    fn api_version_constant_is_1_0() {
        let versions = custody_common::api_versioning::supported_versions();
        assert!(
            versions.iter().any(|v| v.version == "v1"),
            "v1 must be in supported versions"
        );
    }

    // --- Request logger helpers ---

    #[test]
    fn sha256_hex_deterministic() {
        let h1 = super::sha256_hex(b"hello");
        let h2 = super::sha256_hex(b"hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // 32 bytes → 64 hex chars
    }

    #[test]
    fn sha256_hex_different_inputs() {
        let h1 = super::sha256_hex(b"hello");
        let h2 = super::sha256_hex(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn debug_mode_default_false() {
        // Without CUSTODY_DEBUG=true set, debug_mode() must return false.
        // (env var might be set in CI — skip if already set.)
        if std::env::var("CUSTODY_DEBUG").is_err() {
            assert!(!super::debug_mode());
        }
    }

    // --- Prometheus metric names (verify they compile) ---

    #[test]
    fn api_request_latency_metric_exists() {
        // Force initialisation of the lazy static.
        custody_common::prometheus_metrics::API_REQUEST_LATENCY
            .with_label_values(&["GET", "/health"])
            .observe(0.001);
        let output = custody_common::prometheus_metrics::render_metrics();
        assert!(output.contains("custody_api_request_duration_seconds"));
    }

    #[test]
    fn api_request_count_metric_exists() {
        custody_common::prometheus_metrics::API_REQUEST_COUNT
            .with_label_values(&["GET", "200"])
            .inc();
        let output = custody_common::prometheus_metrics::render_metrics();
        assert!(output.contains("custody_api_requests_total"));
    }
}
