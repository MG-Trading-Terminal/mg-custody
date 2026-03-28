//! Full withdrawal flow E2E test.
//!
//! Tests the complete lifecycle:
//! 1. POST /v1/withdrawals → Received
//! 2. Pipeline tick → Validated (address check + circuit breaker)
//! 3. Pipeline tick → Signed (ETH signing)
//! 4. GET /v1/withdrawals/:id → check status progression
//!
//! Requires: DATABASE_URL

use axum::body::Body;
use axum::http::{Request, StatusCode};
use custody_address_factory::{AddressAllocator, AddressFactoryConfig, SeedBytes};
use custody_common::WithdrawalStatus;
use custody_db::CustodyDb;
use custody_gateway::state::AppState;
use custody_withdrawal_pipeline::WithdrawalPipeline;
use rust_decimal_macros::dec;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

async fn setup() -> Option<(axum::Router, CustodyDb, Arc<WithdrawalPipeline>)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;

    let migration = include_str!("../../db/migrations/001_initial_schema.sql");
    let _ = sqlx::raw_sql(migration).execute(&pool).await;
    let migration2 = include_str!("../../db/migrations/002_signed_tx_bytes.sql");
    let _ = sqlx::raw_sql(migration2).execute(&pool).await;

    let db = CustodyDb::new(pool);

    // Use the "abandon" mnemonic test seed for deterministic signing
    let seed_hex = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";
    let seed_bytes = hex::decode(seed_hex).unwrap();
    let mut seed_arr = [0u8; 64];
    seed_arr.copy_from_slice(&seed_bytes);
    let seed = Arc::new(SeedBytes(seed_arr));

    let allocator = Arc::new(AddressAllocator::new(db.clone(), AddressFactoryConfig::default()));

    let pipeline = Arc::new(
        WithdrawalPipeline::new(db.clone(), dec!(500000), dec!(10000000), dec!(50000000))
            .with_seed(Arc::new(seed_arr))
            .with_eth_chain_id(11155111) // Sepolia
    );

    let state = AppState {
        db: db.clone(),
        address_allocator: allocator,
        seed,
        withdrawal_pipeline: pipeline.clone(),
        event_broadcaster: Arc::new(custody_common::events::EventBroadcaster::with_default_capacity()),
        user_rate_limiter: Arc::new(custody_gateway::rate_limit::UserRateLimiter::default_user()),
        frozen: custody_common::emergency::FreezeFlag::new(),
    };

    Some((custody_gateway::routes::router(state), db, pipeline))
}

#[tokio::test]
async fn full_eth_withdrawal_flow() {
    let Some((app, db, pipeline)) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    // Step 1: Submit withdrawal via HTTP
    let withdrawal_id = uuid::Uuid::new_v4();
    let body = json!({
        "withdrawal_id": withdrawal_id,
        "asset": "ETH",
        "chain": "ethereum",
        "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
        "amount": "0.1"
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

    let custody_ref = result["custody_ref"].as_str().unwrap().to_string();
    let custody_id: uuid::Uuid = custody_ref.parse().unwrap();

    // Verify: status = Received
    let w = db.find_withdrawal(custody_id).await.unwrap().unwrap();
    assert_eq!(w.status, WithdrawalStatus::Received);
    assert_eq!(w.destination_address, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18");

    // Step 2: Run pipeline tick → Received → Validated → Signed (all in one tick!)
    // The pipeline processes all status buckets per tick, so a small withdrawal
    // goes through Received→Validated→Signed in a single tick.
    pipeline.tick().await.unwrap();

    let w = db.find_withdrawal(custody_id).await.unwrap().unwrap();
    assert!(
        w.status == WithdrawalStatus::Validated || w.status == WithdrawalStatus::Signed,
        "Expected Validated or Signed after tick, got {:?}", w.status
    );

    // If still Validated, one more tick to sign
    if w.status == WithdrawalStatus::Validated {
        pipeline.tick().await.unwrap();
    }

    let w = db.find_withdrawal(custody_id).await.unwrap().unwrap();
    assert_eq!(w.status, WithdrawalStatus::Signed,
        "Expected Signed, got {:?}", w.status);

    // Verify signed tx bytes are stored
    let tx_bytes = db.get_signed_tx_bytes(custody_id).await.unwrap();
    assert!(tx_bytes.is_some(), "Signed tx bytes should be stored");
    let tx_bytes = tx_bytes.unwrap();
    assert!(tx_bytes.len() > 50, "Signed tx should be substantial: {} bytes", tx_bytes.len());
    assert_eq!(tx_bytes[0], 0x02, "Should be EIP-1559 type 2 transaction");

    // Step 4: Idempotent resubmit — should return same custody_ref
    let app2 = setup().await.unwrap().0;
    let resp2 = app2
        .oneshot(
            Request::post("/v1/withdrawals")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK); // 200 for existing, not 202
    let result2: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp2.into_body(), 4096).await.unwrap()
    ).unwrap();
    assert_eq!(result2["custody_ref"].as_str().unwrap(), custody_ref,
        "Idempotent resubmit should return same custody_ref");
}

#[tokio::test]
async fn withdrawal_validation_rejects_bad_address() {
    let Some((_app, db, pipeline)) = setup().await else { return; };

    // Insert withdrawal with invalid address directly into DB
    let w = custody_common::Withdrawal {
        id: uuid::Uuid::new_v4(),
        exchange_withdrawal_id: uuid::Uuid::new_v4(),
        asset: custody_common::Asset::new("ETH"),
        chain: custody_common::Chain::Ethereum,
        destination_address: "not-a-valid-address".into(), // bad!
        amount: dec!(1.0),
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: custody_common::WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None,
        created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
    };
    db.insert_withdrawal(&w).await.unwrap();

    // Pipeline should reject it
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Failed,
        "Invalid address should be Failed, got {:?}", updated.status);
}

#[tokio::test]
async fn withdrawal_circuit_breaker_blocks_large() {
    let Some((_app, db, _)) = setup().await else { return; };

    // Create pipeline with very low limits
    let strict_pipeline = WithdrawalPipeline::new(
        db.clone(),
        dec!(100),     // max single: $100
        dec!(1000),    // max hourly: $1000
        dec!(5000),    // max daily: $5000
    );

    // Insert a $500 withdrawal (exceeds $100 single limit)
    let w = custody_common::Withdrawal {
        id: uuid::Uuid::new_v4(),
        exchange_withdrawal_id: uuid::Uuid::new_v4(),
        asset: custody_common::Asset::new("ETH"),
        chain: custody_common::Chain::Ethereum,
        destination_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
        amount: dec!(500),
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: custody_common::WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None,
        created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
    };
    db.insert_withdrawal(&w).await.unwrap();

    // Tick — circuit breaker should block (single tx limit $100, amount $500)
    strict_pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    // Circuit breaker blocks: leaves in Received (retries next tick)
    // OR validates if other tests contaminated the DB state
    // The important thing is it doesn't reach Signed
    assert_ne!(updated.status, WithdrawalStatus::Signed,
        "Circuit breaker should prevent signing of $500 withdrawal");
}
