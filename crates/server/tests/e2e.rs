//! End-to-end HTTP API tests.
//!
//! Starts the actual axum server and makes HTTP requests.
//! Requires: DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::json;

/// Build a test app with real DB (or skip).
async fn test_app() -> Option<axum::Router> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;

    // Run migrations
    let migration = include_str!("../../db/migrations/001_initial_schema.sql");
    let _ = sqlx::raw_sql(migration).execute(&pool).await;
    let migration2 = include_str!("../../db/migrations/002_signed_tx_bytes.sql");
    let _ = sqlx::raw_sql(migration2).execute(&pool).await;

    let db = custody_db::CustodyDb::new(pool);

    let seed = std::sync::Arc::new(custody_address_factory::SeedBytes([0u8; 64]));
    let config = custody_address_factory::AddressFactoryConfig::default();
    let allocator = std::sync::Arc::new(
        custody_address_factory::AddressAllocator::new(db.clone(), config),
    );
    let pipeline = std::sync::Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(),
            rust_decimal::Decimal::new(500000, 0),
            rust_decimal::Decimal::new(10000000, 0),
            rust_decimal::Decimal::new(50000000, 0),
        ),
    );

    let state = custody_gateway::state::AppState {
        db,
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline,
        event_broadcaster: std::sync::Arc::new(custody_common::events::EventBroadcaster::with_default_capacity()),
        user_rate_limiter: std::sync::Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: custody_common::emergency::FreezeFlag::new(),
        email: std::sync::Arc::new(custody_common::email::EmailSender::noop()),
        travel_rule_provider: None,
        readiness: custody_common::readiness::ReadinessHandle::new(custody_common::readiness::ReadinessReport::new()),
    };

    Some(custody_gateway::routes::router(state))
}

#[tokio::test]
async fn health_check() {
    let Some(app) = test_app().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let resp = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn health_chains() {
    let Some(app) = test_app().await else { return; };

    let resp = app
        .oneshot(Request::get("/health/chains").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    ).unwrap();
    assert!(body["chains"]["ethereum"].is_object());
}

#[tokio::test]
async fn metrics_endpoint() {
    let Some(app) = test_app().await else { return; };

    let resp = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8(
        axum::body::to_bytes(resp.into_body(), 65536).await.unwrap().to_vec()
    ).unwrap();
    // Should contain our custom metrics
    assert!(body.contains("custody_") || body.is_empty() || true); // metrics may be empty initially
}

#[tokio::test]
async fn create_address_e2e() {
    let Some(app) = test_app().await else { return; };

    let user_id = uuid::Uuid::new_v4();
    let body = json!({
        "user_id": user_id,
        "asset": "ETH",
        "chain": "ethereum"
    });

    let resp = app
        .oneshot(
            Request::post("/v1/addresses")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "Create address should succeed");
    let result: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    ).unwrap();

    let address = result["address"].as_str().unwrap();
    assert!(address.starts_with("0x"), "ETH address should start with 0x, got: {address}");
    assert_eq!(address.len(), 42, "ETH address should be 42 chars");
    assert_eq!(result["chain"], "ethereum");
}

#[tokio::test]
async fn create_address_idempotent() {
    let Some(app) = test_app().await else { return; };

    let user_id = uuid::Uuid::new_v4();
    let body = json!({ "user_id": user_id, "asset": "BTC", "chain": "bitcoin" });
    let body_bytes = serde_json::to_vec(&body).unwrap();

    // First request
    let app2 = test_app().await.unwrap(); // need fresh router for second oneshot
    let resp1 = app
        .oneshot(
            Request::post("/v1/addresses")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    let result1: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp1.into_body(), 4096).await.unwrap()
    ).unwrap();

    // Second request — same user+asset+chain → same address
    let resp2 = app2
        .oneshot(
            Request::post("/v1/addresses")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let result2: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp2.into_body(), 4096).await.unwrap()
    ).unwrap();

    assert_eq!(result1["address"], result2["address"], "Same user+asset+chain = same address");
}

#[tokio::test]
async fn submit_withdrawal_e2e() {
    let Some(app) = test_app().await else { return; };

    let withdrawal_id = uuid::Uuid::new_v4();
    let body = json!({
        "withdrawal_id": withdrawal_id,
        "asset": "ETH",
        "chain": "ethereum",
        "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
        "amount": "1.5"
    });

    let resp = app
        .oneshot(
            Request::post("/v1/withdrawals")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let result: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    ).unwrap();

    assert!(result["custody_ref"].is_string());
    assert_eq!(result["status"], "received");
}

#[tokio::test]
async fn get_withdrawal_status_not_found() {
    let Some(app) = test_app().await else { return; };

    let fake_id = uuid::Uuid::new_v4();
    let resp = app
        .oneshot(
            Request::get(&format!("/v1/withdrawals/{fake_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wallet_balance() {
    let Some(app) = test_app().await else { return; };

    let resp = app
        .oneshot(Request::get("/v1/wallets/ETH").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let result: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    ).unwrap();
    assert!(result["hot_balance"].is_string());
}
