//! Database integration tests.
//!
//! Requires a running PostgreSQL instance.
//! Run: DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody cargo test -p custody-db --test integration
//!
//! Setup: docker compose up -d postgres
//!        psql $DATABASE_URL -f crates/db/migrations/001_initial_schema.sql

use custody_common::*;
use custody_db::CustodyDb;
use rust_decimal_macros::dec;
use uuid::Uuid;

/// Get DB connection or skip test.
async fn get_db() -> Option<CustodyDb> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;

    // Run migrations
    let migration = include_str!("../migrations/001_initial_schema.sql");
    let _ = sqlx::raw_sql(migration).execute(&pool).await;

    Some(CustodyDb::new(pool))
}

/// Get DB connection with circuit breaker migration applied.
async fn get_db_with_circuit_breaker() -> Option<(CustodyDb, String)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;

    // Run required migrations
    let migration = include_str!("../migrations/001_initial_schema.sql");
    let _ = sqlx::raw_sql(migration).execute(&pool).await;
    let migration_cb = include_str!("../migrations/009_circuit_breaker.sql");
    let _ = sqlx::raw_sql(migration_cb).execute(&pool).await;

    Some((CustodyDb::new(pool), url))
}

#[tokio::test]
async fn address_crud() {
    let Some(db) = get_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let user_id = Uuid::new_v4();
    let asset = Asset::new("ETH");
    let chain = Chain::Ethereum;

    // Should not exist initially
    let found = db.find_address(user_id, &asset, chain).await.unwrap();
    assert!(found.is_none());

    // Insert
    let addr = DepositAddress {
        id: Uuid::new_v4(),
        user_id,
        asset: asset.clone(),
        chain,
        address: format!("0x{}", hex::encode(&[0xAB; 20])),
        derivation_path: "m/44'/60'/0'/0/0".to_string(),
        created_at: chrono::Utc::now(),
    };
    db.insert_address(&addr).await.unwrap();

    // Should find it now
    let found = db.find_address(user_id, &asset, chain).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.address, addr.address);
    assert_eq!(found.derivation_path, "m/44'/60'/0'/0/0");

    // Idempotent: inserting same (user, asset, chain) again should not fail
    db.insert_address(&addr).await.unwrap();

    // All addresses for chain
    let all = db.all_addresses_for_chain(Chain::Ethereum).await.unwrap();
    assert!(all.contains(&addr.address));
}

#[tokio::test]
async fn address_counter_atomic() {
    let Some(db) = get_db().await else { return; };

    // Get index (should increment)
    let idx1 = db.next_address_index(Chain::Ethereum).await.unwrap();
    let idx2 = db.next_address_index(Chain::Ethereum).await.unwrap();
    assert_eq!(idx2, idx1 + 1, "Index should increment atomically");
}

#[tokio::test]
async fn deposit_lifecycle() {
    let Some(db) = get_db().await else { return; };

    let deposit = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: "0xtest123".to_string(),
        amount: dec!(1.5),
        tx_hash: format!("0x{}", Uuid::new_v4().to_string().replace("-", "")),
        log_index: None,
        block_height: 20_000_000,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
        memo: None,
        source_memo: None,
    };

    // Insert
    db.insert_deposit(&deposit).await.unwrap();

    // Find by tx_hash
    let found = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.amount, dec!(1.5));
    assert_eq!(found.status, DepositStatus::Detected);

    // Query by status
    let detected = db.deposits_by_status(DepositStatus::Detected).await.unwrap();
    assert!(detected.iter().any(|d| d.id == deposit.id));

    // Update status
    db.update_deposit_status(deposit.id, DepositStatus::Confirmed, 12).await.unwrap();
    let updated = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(updated.status, DepositStatus::Confirmed);
    assert_eq!(updated.confirmations, 12);

    // Dedup: inserting same tx_hash again with different id should be ignored
    // (ON CONFLICT on tx_hash + log_index; NULL log_index = unique in SQL)
    // In practice dedup is done by checking find_deposit_by_tx_hash first
}

#[tokio::test]
async fn withdrawal_lifecycle() {
    let Some(db) = get_db().await else { return; };

    let exchange_id = Uuid::new_v4();
    let withdrawal = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: exchange_id,
        user_id: uuid::Uuid::nil(),
        vault_id: None,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: "0xdest456".to_string(),
        amount: dec!(2.0),
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None,
        block_height: None,
        confirmations: 0,
        error_message: None,
        created_at: chrono::Utc::now(),
        signed_at: None,
        broadcast_at: None,
        confirmed_at: None,
        delay_release_at: None,
        memo: None,
        destination_tag: None,
    };

    // Insert
    db.insert_withdrawal(&withdrawal).await.unwrap();

    // Find by exchange ID (idempotency)
    let found = db.find_withdrawal_by_exchange_id(exchange_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().amount, dec!(2.0));

    // Find by custody ID
    let found = db.find_withdrawal(withdrawal.id).await.unwrap();
    assert!(found.is_some());

    // Query by status
    let received = db.withdrawals_by_status(WithdrawalStatus::Received).await.unwrap();
    assert!(received.iter().any(|w| w.id == withdrawal.id));

    // Update status: Received → Validated
    db.update_withdrawal_status(withdrawal.id, WithdrawalStatus::Validated).await.unwrap();
    let updated = db.find_withdrawal(withdrawal.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Validated);

    // Set tx_hash after broadcast
    db.set_withdrawal_tx_hash(withdrawal.id, "0xbroadcasthash").await.unwrap();
    let updated = db.find_withdrawal(withdrawal.id).await.unwrap().unwrap();
    assert_eq!(updated.tx_hash.as_deref(), Some("0xbroadcasthash"));

    // Store signed tx bytes
    let signed_bytes = vec![0x02, 0xDE, 0xAD, 0xBE, 0xEF];
    db.set_signed_tx(withdrawal.id, &signed_bytes, 42, dec!(50.5)).await.unwrap();

    // Retrieve signed tx bytes
    let loaded = db.get_signed_tx_bytes(withdrawal.id).await.unwrap();
    assert_eq!(loaded, Some(signed_bytes));

    // Idempotent: inserting same exchange_id should not fail
    db.insert_withdrawal(&withdrawal).await.unwrap();
}

#[tokio::test]
async fn scan_progress() {
    let Some(db) = get_db().await else { return; };

    // Set progress
    db.set_scan_progress(Chain::Ethereum, 20_000_000).await.unwrap();

    // Get progress
    let height = db.get_scan_progress(Chain::Ethereum).await.unwrap();
    assert_eq!(height, Some(20_000_000));

    // Update
    db.set_scan_progress(Chain::Ethereum, 20_000_100).await.unwrap();
    let height = db.get_scan_progress(Chain::Ethereum).await.unwrap();
    assert_eq!(height, Some(20_000_100));
}

#[tokio::test]
async fn nonce_management() {
    let Some(db) = get_db().await else { return; };

    let address = format!("0xtest_{}", Uuid::new_v4());

    // Initially no nonce
    let nonce = db.get_nonce(&address).await.unwrap();
    assert!(nonce.is_none());

    // Set initial nonce
    db.set_nonce(&address, 10).await.unwrap();
    assert_eq!(db.get_nonce(&address).await.unwrap(), Some(10));

    // Increment
    let prev = db.increment_nonce(&address).await.unwrap();
    assert_eq!(prev, 10); // returns previous value
    assert_eq!(db.get_nonce(&address).await.unwrap(), Some(11));
}

#[tokio::test]
async fn outflow_tracking() {
    let Some(db) = get_db().await else { return; };

    // Insert a recent withdrawal
    let w = Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: Uuid::new_v4(),
        user_id: uuid::Uuid::nil(),
        vault_id: None,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: "0xout".into(),
        amount: dec!(1000),
        fee: None,
        status: WithdrawalStatus::Validated,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None,
        created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
        memo: None, destination_tag: None,
    };
    db.insert_withdrawal(&w).await.unwrap();

    // Hourly outflow should include this withdrawal
    let hourly = db.hourly_outflow().await.unwrap();
    assert!(hourly >= dec!(1000), "Hourly outflow should be >= 1000, got {hourly}");

    // Daily outflow too
    let daily = db.daily_outflow().await.unwrap();
    assert!(daily >= dec!(1000));
}

/// CRITICAL TEST (Phase 17C): Circuit breaker counters survive process restart.
///
/// Previously counters were in-memory only — restarting the process reset them to zero,
/// allowing another $10M to flow out after $9M had already been withdrawn.
/// This test proves the fix: outflow recorded before "restart" is still there after.
#[tokio::test]
async fn circuit_breaker_survives_restart() {
    let Some((db, url)) = get_db_with_circuit_breaker().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    // 1. Reset to known state
    db.reset_circuit_breaker().await.unwrap();

    // 2. Record $5M outflow (mid-hour, mid-day)
    db.record_outflow(dec!(5_000_000)).await.unwrap();

    // 3. Verify counters before "restart"
    let before = db.get_circuit_breaker_status().await.unwrap();
    assert_eq!(before.hourly_outflow_usd, dec!(5_000_000), "Pre-restart hourly should be $5M");
    assert_eq!(before.daily_outflow_usd, dec!(5_000_000), "Pre-restart daily should be $5M");
    assert!(!before.is_tripped, "Should not be tripped at $5M");

    // 4. Simulate process restart: drop DB, create fresh connection
    drop(db);
    let pool2 = sqlx::PgPool::connect(&url).await.unwrap();
    let db2 = CustodyDb::new(pool2);

    // 5. Load state on "startup" — counters MUST survive
    let after = db2.load_circuit_breaker().await.unwrap();
    assert_eq!(after.hourly_outflow_usd, dec!(5_000_000), "Post-restart hourly must be $5M");
    assert_eq!(after.daily_outflow_usd, dec!(5_000_000), "Post-restart daily must be $5M");

    // 6. The $5M already used means only $5M hourly headroom left (limit=$10M)
    //    A $6M withdrawal should be blocked
    let check = db2.check_circuit_breaker(dec!(6_000_000)).await;
    assert!(check.is_err(), "Should block: $5M used + $6M new > $10M hourly limit");

    // 7. A $4M withdrawal should still pass
    let check = db2.check_circuit_breaker(dec!(4_000_000)).await;
    assert!(check.is_ok(), "Should allow: $5M used + $4M new < $10M hourly limit");

    // 8. Record another $4M, then restart again
    db2.record_outflow(dec!(4_000_000)).await.unwrap();
    drop(db2);
    let pool3 = sqlx::PgPool::connect(&url).await.unwrap();
    let db3 = CustodyDb::new(pool3);

    let final_state = db3.load_circuit_breaker().await.unwrap();
    assert_eq!(final_state.hourly_outflow_usd, dec!(9_000_000), "After 2nd restart: $5M + $4M = $9M");
    assert_eq!(final_state.daily_outflow_usd, dec!(9_000_000), "Daily should also be $9M");

    // 9. Tripped state also survives restart
    db3.trip_circuit_breaker("test trip").await.unwrap();
    drop(db3);
    let pool4 = sqlx::PgPool::connect(&url).await.unwrap();
    let db4 = CustodyDb::new(pool4);

    let tripped = db4.load_circuit_breaker().await.unwrap();
    assert!(tripped.is_tripped, "Tripped state must survive restart");
    assert_eq!(tripped.trip_reason.as_deref(), Some("test trip"));

    // Cleanup
    db4.reset_circuit_breaker().await.unwrap();
}
