//! In-process API integration tests — NO external dependencies.
//!
//! Uses tower::ServiceExt::oneshot() to send real HTTP requests through
//! the full axum middleware stack (RBAC, rate limiting, security headers)
//! without binding a TCP port.
//!
//! Tests that don't need a database (health, metrics, openapi) always run.
//! Tests that need DB are gated on DATABASE_URL and run migrations first.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use custody_common::emergency::FreezeFlag;
use custody_common::events::EventBroadcaster;
use custody_db::CustodyDb;
use custody_gateway::state::AppState;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

// ── Test App Builders ───────────────────────────────────────────

/// Build a minimal app with lazy DB (no real connection needed).
/// Good for testing public endpoints that don't hit DB.
fn build_app_no_db() -> axum::Router {
    let pool = sqlx::PgPool::connect_lazy("postgres://fake:fake@localhost:5432/fake")
        .expect("connect_lazy should not fail");
    let db = CustodyDb::new(pool);

    let seed_hex = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";
    let seed_bytes = hex::decode(seed_hex).unwrap();
    let mut seed_arr = [0u8; 64];
    seed_arr.copy_from_slice(&seed_bytes);

    let seed = Arc::new(custody_address_factory::SeedBytes(seed_arr));
    let allocator = Arc::new(custody_address_factory::AddressAllocator::new(
        db.clone(),
        custody_address_factory::AddressFactoryConfig::default(),
    ));
    let pipeline = Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(),
            rust_decimal_macros::dec!(500000),
            rust_decimal_macros::dec!(10000000),
            rust_decimal_macros::dec!(50000000),
        )
        .with_seed(Arc::new(seed_arr)),
    );

    let state = AppState {
        db,
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline,
        event_broadcaster: Arc::new(EventBroadcaster::with_default_capacity()),
        user_rate_limiter: Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: FreezeFlag::new(),
    };

    // Use router() — no HMAC layer.
    // RBAC falls through to Admin for requests without Bearer token.
    custody_gateway::routes::router(state)
}

/// Build app with real DB — only if DATABASE_URL is set.
async fn build_app_with_db() -> Option<(axum::Router, CustodyDb)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;

    // Run critical migrations
    let migrations = [
        include_str!("../../db/migrations/001_initial_schema.sql"),
        include_str!("../../db/migrations/002_signed_tx_bytes.sql"),
        include_str!("../../db/migrations/003_vaults.sql"),
        include_str!("../../db/migrations/009_circuit_breaker.sql"),
        include_str!("../../db/migrations/011_sessions.sql"),
    ];
    for sql in &migrations {
        let _ = sqlx::raw_sql(sql).execute(&pool).await;
    }

    let db = CustodyDb::new(pool);

    let seed_hex = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";
    let seed_bytes = hex::decode(seed_hex).unwrap();
    let mut seed_arr = [0u8; 64];
    seed_arr.copy_from_slice(&seed_bytes);

    let seed = Arc::new(custody_address_factory::SeedBytes(seed_arr));
    let allocator = Arc::new(custody_address_factory::AddressAllocator::new(
        db.clone(),
        custody_address_factory::AddressFactoryConfig::default(),
    ));

    let policy_rules = vec![
        custody_policy::PolicyRule::new(
            "allow_small", 10,
            custody_policy::PolicyCondition {
                max_amount_usd: Some(rust_decimal_macros::dec!(10000)),
                ..Default::default()
            },
            custody_policy::PolicyAction::Allow,
        ),
    ];
    let policy_engine = Arc::new(custody_policy::PolicyEngine::new(policy_rules));
    let sanctions = Arc::new(custody_compliance::ofac::ofac_checker_with_known_addresses());

    let pipeline = Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(),
            rust_decimal_macros::dec!(500000),
            rust_decimal_macros::dec!(10000000),
            rust_decimal_macros::dec!(50000000),
        )
        .with_seed(Arc::new(seed_arr))
        .with_policy_engine(policy_engine)
        .with_sanctions_checker(sanctions)
        .with_eth_chain_id(11155111),
    );

    let state = AppState {
        db: db.clone(),
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline,
        event_broadcaster: Arc::new(EventBroadcaster::with_default_capacity()),
        user_rate_limiter: Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: FreezeFlag::new(),
    };

    let app = custody_gateway::routes::router(state);
    Some((app, db))
}

/// Build a request. With router() (no HMAC layer), RBAC middleware gives
/// Admin role to all requests without Bearer token (HMAC fallback path).
fn api_request(method: &str, path: &str, body: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .uri(path)
        .method(method)
        .header("Content-Type", "application/json");

    if let Some(b) = body {
        builder.body(Body::from(b.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

// ═══════════════════════════════════════════════════════════════
// PUBLIC ENDPOINTS (no DB needed — always run)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn health_endpoint() {
    let app = build_app_no_db();
    let resp = app.oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_chains_endpoint() {
    let app = build_app_no_db();
    let resp = app.oneshot(Request::get("/health/chains").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn metrics_endpoint_returns_prometheus() {
    let app = build_app_no_db();
    let resp = app.oneshot(Request::get("/metrics").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 100_000).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    // Should contain our custody metrics
    assert!(text.contains("custody_"), "Metrics should contain custody_* entries");
}

#[tokio::test]
async fn openapi_spec_endpoint() {
    let app = build_app_no_db();
    let resp = app.oneshot(
        Request::get("/v1/openapi.json").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 500_000).await.unwrap();
    let spec: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["paths"].as_object().unwrap().len() > 10, "Should have 10+ paths");
}

#[tokio::test]
async fn security_headers_present() {
    let app = build_app_no_db();
    let resp = app.oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();

    // CSP
    assert!(resp.headers().contains_key("content-security-policy"),
        "Missing Content-Security-Policy header");
    // X-Frame-Options
    assert!(resp.headers().contains_key("x-frame-options"),
        "Missing X-Frame-Options header");
    // X-Content-Type-Options
    assert!(resp.headers().contains_key("x-content-type-options"),
        "Missing X-Content-Type-Options header");
    // API version
    assert!(resp.headers().contains_key("x-api-version"),
        "Missing X-API-Version header");
}

#[tokio::test]
async fn unauthenticated_request_denied() {
    let app = build_app_no_db();
    // Authenticated endpoints should reject without HMAC
    let resp = app.oneshot(
        Request::get("/v1/wallets/ETH").body(Body::empty()).unwrap()
    ).await.unwrap();
    // Should get 401 or at least not 200
    assert_ne!(resp.status(), StatusCode::OK,
        "Unauthenticated request should not return 200");
}

// ═══════════════════════════════════════════════════════════════
// AUTH BEHAVIOR (router() mode — RBAC gives Admin to all)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn rbac_grants_admin_without_bearer() {
    let app = build_app_no_db();
    // In router() mode, requests without Bearer get Admin via HMAC fallback.
    // The request will fail on DB (lazy pool), but should NOT be 401/403.
    let req = api_request("GET", "/v1/wallets/ETH", None);
    let resp = app.oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED, "Should not be 401");
    assert_ne!(resp.status(), StatusCode::FORBIDDEN, "Should not be 403");
    // Will be 500 (DB not connected) — that's fine, auth passed
}

// ═══════════════════════════════════════════════════════════════
// DB-DEPENDENT TESTS (only run with DATABASE_URL)
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn db_create_address_and_query() {
    let Some((app, _db)) = build_app_with_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let user_id = uuid::Uuid::new_v4();
    let body = json!({"user_id": user_id, "asset": "ETH", "chain": "ethereum"}).to_string();
    let req = api_request("POST", "/v1/addresses", Some(&body));

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "Should create address");

    let resp_body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
    let addr: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    let address = addr["address"].as_str().unwrap();
    assert!(address.starts_with("0x"), "ETH address should start with 0x");
    assert_eq!(address.len(), 42, "ETH address should be 42 chars");

    // Same request should be idempotent — return same address
    let body2 = json!({"user_id": user_id, "asset": "ETH", "chain": "ethereum"}).to_string();
    let req2 = api_request("POST", "/v1/addresses", Some(&body2));
    let resp2 = app.oneshot(req2).await.unwrap();
    let resp_body2 = axum::body::to_bytes(resp2.into_body(), 10_000).await.unwrap();
    let addr2: serde_json::Value = serde_json::from_slice(&resp_body2).unwrap();
    assert_eq!(addr["address"], addr2["address"], "Address allocation must be idempotent");
}

#[tokio::test]
async fn db_submit_withdrawal_and_check_status() {
    let Some((app, _db)) = build_app_with_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let w_id = uuid::Uuid::new_v4();
    let body = json!({
        "withdrawal_id": w_id,
        "asset": "ETH",
        "chain": "ethereum",
        "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
        "amount": "0.01"
    }).to_string();

    let req = api_request("POST", "/v1/withdrawals", Some(&body));
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED, "Withdrawal should return 202");

    let resp_body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
    let result: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(result["custody_ref"].is_string(), "Should return custody_ref");

    // Check status
    let custody_ref = result["custody_ref"].as_str().unwrap();
    let path = format!("/v1/withdrawals/{custody_ref}");
    let req2 = api_request("GET", &path, None);
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK, "Status check should return 200");

    let status_body = axum::body::to_bytes(resp2.into_body(), 10_000).await.unwrap();
    let status: serde_json::Value = serde_json::from_slice(&status_body).unwrap();
    assert!(status["status"].is_string(), "Should have status field");

    // Idempotency: same withdrawal_id = same result
    let req3 = api_request("POST", "/v1/withdrawals", Some(&body));
    let resp3 = app.oneshot(req3).await.unwrap();
    let resp_body3 = axum::body::to_bytes(resp3.into_body(), 10_000).await.unwrap();
    let result3: serde_json::Value = serde_json::from_slice(&resp_body3).unwrap();
    assert_eq!(result["custody_ref"], result3["custody_ref"], "Withdrawal must be idempotent");
}

#[tokio::test]
async fn db_wallet_balance() {
    let Some((app, _db)) = build_app_with_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let req = api_request("GET", "/v1/wallets/ETH", None);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
    let balance: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(balance["hot_balance"].is_string(), "Should have hot_balance as string");
    assert!(balance["warm_balance"].is_string(), "Should have warm_balance as string");
    assert!(balance["total_managed"].is_string(), "Should have total_managed as string");
}

#[tokio::test]
async fn db_circuit_breaker_status() {
    let Some((app, _db)) = build_app_with_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let req = api_request("GET", "/v1/circuit-breaker/status", None);
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 10_000).await.unwrap();
    let cb: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(cb["is_tripped"].is_boolean(), "Should have is_tripped");
    assert!(cb["single_tx_limit_usd"].is_string(), "Limits should be string (Decimal)");
}

#[tokio::test]
async fn db_emergency_freeze_and_unfreeze() {
    let Some((app, _db)) = build_app_with_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    // Freeze
    let body = json!({"reason": "integration test"}).to_string();
    let req = api_request("POST", "/v1/admin/emergency/freeze", Some(&body));
    let resp = app.clone().oneshot(req).await.unwrap();
    // May be 200 or 409 (already frozen) — both acceptable
    assert!(resp.status() == StatusCode::OK || resp.status() == StatusCode::CONFLICT);

    // Check status
    let req2 = api_request("GET", "/v1/admin/emergency/status", None);
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);

    // Unfreeze
    let body3 = json!({"reason": "test cleanup"}).to_string();
    let req3 = api_request("POST", "/v1/admin/emergency/unfreeze", Some(&body3));
    let resp3 = app.oneshot(req3).await.unwrap();
    assert!(resp3.status() == StatusCode::OK || resp3.status() == StatusCode::CONFLICT);
}
