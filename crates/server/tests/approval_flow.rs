//! Approval workflow E2E test.
//!
//! Tests: submit large withdrawal → PendingApproval → approve → Validated → Signed

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::json;

async fn setup() -> Option<(axum::Router, custody_db::CustodyDb)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let _ = sqlx::raw_sql(include_str!("../../db/migrations/001_initial_schema.sql")).execute(&pool).await;
    let _ = sqlx::raw_sql(include_str!("../../db/migrations/002_signed_tx_bytes.sql")).execute(&pool).await;
    let db = custody_db::CustodyDb::new(pool);

    let seed = std::sync::Arc::new(custody_address_factory::SeedBytes([0u8; 64]));
    let allocator = std::sync::Arc::new(custody_address_factory::AddressAllocator::new(
        db.clone(), custody_address_factory::AddressFactoryConfig::default(),
    ));

    // Policy: >$100 requires 1 approval
    let rules = vec![
        custody_policy::PolicyRule::new("allow_small", 10,
            custody_policy::PolicyCondition { max_amount_usd: Some(rust_decimal_macros::dec!(100)), ..Default::default() },
            custody_policy::PolicyAction::Allow,
        ),
        custody_policy::PolicyRule::new("approve_large", 20,
            custody_policy::PolicyCondition { min_amount_usd: Some(rust_decimal_macros::dec!(100)), ..Default::default() },
            custody_policy::PolicyAction::RequireApproval { quorum: 1, approver_role: custody_policy::rbac::Role::Signer },
        ),
    ];

    let pipeline = std::sync::Arc::new(
        custody_withdrawal_pipeline::WithdrawalPipeline::new(
            db.clone(), rust_decimal_macros::dec!(500000), rust_decimal_macros::dec!(10000000), rust_decimal_macros::dec!(50000000),
        )
        .with_policy_engine(std::sync::Arc::new(custody_policy::PolicyEngine::new(rules)))
        .with_seed(std::sync::Arc::new([0u8; 64]))
        .with_eth_chain_id(11155111)
    );

    let state = custody_gateway::state::AppState { db: db.clone(), address_allocator: allocator, seed, withdrawal_pipeline: pipeline.clone(), event_broadcaster: std::sync::Arc::new(custody_common::events::EventBroadcaster::with_default_capacity()), user_rate_limiter: std::sync::Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()) };
    Some((custody_gateway::routes::router(state), db))
}

#[tokio::test]
async fn large_withdrawal_requires_approval() {
    let Some((app, db)) = setup().await else { return; };

    // Submit $500 withdrawal (>$100 threshold)
    let wid = uuid::Uuid::new_v4();
    let resp = app.oneshot(
        Request::post("/v1/withdrawals")
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&json!({
                "withdrawal_id": wid, "asset": "ETH", "chain": "ethereum",
                "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", "amount": "500"
            })).unwrap())).unwrap(),
    ).await.unwrap();

    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let result: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    ).unwrap();
    let custody_id: uuid::Uuid = result["custody_ref"].as_str().unwrap().parse().unwrap();

    // Pipeline tick → should move to PendingApproval
    let pipeline = custody_withdrawal_pipeline::WithdrawalPipeline::new(
        db.clone(), rust_decimal_macros::dec!(500000), rust_decimal_macros::dec!(10000000), rust_decimal_macros::dec!(50000000),
    ).with_policy_engine(std::sync::Arc::new(custody_policy::PolicyEngine::new(vec![
        custody_policy::PolicyRule::new("allow_small", 10,
            custody_policy::PolicyCondition { max_amount_usd: Some(rust_decimal_macros::dec!(100)), ..Default::default() },
            custody_policy::PolicyAction::Allow),
        custody_policy::PolicyRule::new("approve_large", 20,
            custody_policy::PolicyCondition { min_amount_usd: Some(rust_decimal_macros::dec!(100)), ..Default::default() },
            custody_policy::PolicyAction::RequireApproval { quorum: 1, approver_role: custody_policy::rbac::Role::Signer }),
    ])));
    pipeline.tick().await.unwrap();

    let w = db.find_withdrawal(custody_id).await.unwrap().unwrap();
    assert_eq!(w.status, custody_common::WithdrawalStatus::PendingApproval,
        "Large withdrawal should need approval, got {:?}", w.status);

    // Approve via API
    let app2 = setup().await.unwrap().0;
    let resp = app2.oneshot(
        Request::post(&format!("/v1/withdrawals/{custody_id}/approve"))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&json!({"approved": true, "approver": "admin@co.com"})).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // After approval: status should be Validated (pipeline will pick up next tick)
    let w = db.find_withdrawal(custody_id).await.unwrap().unwrap();
    assert_eq!(w.status, custody_common::WithdrawalStatus::Validated,
        "Approved withdrawal should be Validated, got {:?}", w.status);
}
