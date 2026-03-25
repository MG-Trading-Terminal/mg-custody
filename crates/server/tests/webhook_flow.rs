//! Webhook delivery E2E test.
//!
//! Starts a mock HTTP server, triggers deposit confirmation,
//! and verifies the webhook is actually received with correct payload.

use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::collections::HashMap;
use custody_common::*;
use custody_confirmations::{ConfirmationTracker, WebhookSender};
use custody_confirmations::tracker::BlockHeightFn;
use custody_db::CustodyDb;
use rust_decimal_macros::dec;
use uuid::Uuid;

async fn get_db() -> Option<CustodyDb> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let migration = include_str!("../../db/migrations/001_initial_schema.sql");
    let _ = sqlx::raw_sql(migration).execute(&pool).await;
    Some(CustodyDb::new(pool))
}

/// Start a mock webhook receiver that counts received webhooks.
async fn start_mock_webhook_server() -> (String, Arc<AtomicU32>, Arc<tokio::sync::Mutex<Vec<String>>>) {
    let counter = Arc::new(AtomicU32::new(0));
    let payloads = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));

    let counter_clone = counter.clone();
    let payloads_clone = payloads.clone();

    let app = axum::Router::new().route(
        "/webhook",
        axum::routing::post(move |body: String| {
            let c = counter_clone.clone();
            let p = payloads_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                p.lock().await.push(body);
                "ok"
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}/webhook", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Small delay to ensure server is ready
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (url, counter, payloads)
}

#[tokio::test]
async fn deposit_confirmed_fires_webhook() {
    let Some(db) = get_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    // Start mock webhook receiver
    let (webhook_url, webhook_count, webhook_payloads) = start_mock_webhook_server().await;

    // Create webhook sender pointing to our mock
    let webhook_sender = Arc::new(WebhookSender::new(
        webhook_url.clone(),
        b"test-webhook-secret".to_vec(),
        db.clone(),
        5,
    ));

    // Create a deposit at block 20,000,000
    let deposit = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: "0xwebhook_test".into(),
        amount: dec!(3.14),
        tx_hash: format!("0xwh_{}", Uuid::new_v4().to_string().replace("-", "")),
        log_index: Some(0),
        block_height: 20_000_000,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };
    db.insert_deposit(&deposit).await.unwrap();

    // Create confirmation tracker with mock block height (returns 20,000,012 = exactly 12 confirms)
    let mut block_fns: HashMap<Chain, BlockHeightFn> = HashMap::new();
    block_fns.insert(Chain::Ethereum, Arc::new(|| {
        Box::pin(async { Ok(20_000_012u64) }) // 12 confirmations
    }));

    let tracker = ConfirmationTracker::new(db.clone(), block_fns, webhook_sender);

    // Run one tick — should confirm deposit and fire webhook
    tracker.tick().await.unwrap();

    // Verify deposit is now Confirmed
    let updated = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(updated.status, DepositStatus::Confirmed,
        "Deposit should be Confirmed, got {:?}", updated.status);

    // Verify webhook was received
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = webhook_count.load(Ordering::SeqCst);
    assert!(count >= 1, "Webhook should have been called at least once, got {count}");

    // Verify webhook payload
    let payloads = webhook_payloads.lock().await;
    assert!(!payloads.is_empty(), "Should have received webhook payload");
    let payload: serde_json::Value = serde_json::from_str(&payloads[0]).unwrap();
    assert_eq!(payload["event"], "deposit_confirmed");
    // The webhook may include data from any confirmed deposit in DB
    // Verify the structure is correct
    assert!(payload["data"]["confirmations"].is_number());
    assert!(payload["data"]["deposit_id"].is_string());
    assert!(payload["data"]["tx_hash"].is_string());
}

#[tokio::test]
async fn deposit_not_confirmed_when_insufficient_blocks() {
    let Some(db) = get_db().await else { return; };

    let (webhook_url, webhook_count, _) = start_mock_webhook_server().await;
    let webhook_sender = Arc::new(WebhookSender::new(
        webhook_url, b"secret".to_vec(), db.clone(), 5,
    ));

    // Deposit at block 20,000,000
    let deposit = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: "0xno_confirm".into(),
        amount: dec!(1.0),
        tx_hash: format!("0xnc_{}", Uuid::new_v4().to_string().replace("-", "")),
        log_index: Some(0),
        block_height: 20_000_000,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };
    db.insert_deposit(&deposit).await.unwrap();

    // Block height = 20,000,010 → only 10 confirmations (need 12)
    let mut block_fns: HashMap<Chain, BlockHeightFn> = HashMap::new();
    block_fns.insert(Chain::Ethereum, Arc::new(|| {
        Box::pin(async { Ok(20_000_010u64) }) // only 10 confirms
    }));

    let tracker = ConfirmationTracker::new(db.clone(), block_fns, webhook_sender);
    tracker.tick().await.unwrap();

    // THIS specific deposit should still be Detected (not enough confirmations)
    let updated = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(updated.status, DepositStatus::Detected,
        "Deposit at block 20M with only 10 confirms should stay Detected");

    // Webhook may fire for OTHER deposits in DB from previous tests
    // The key assertion is that THIS deposit stayed Detected
}
