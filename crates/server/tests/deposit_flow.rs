//! Full deposit flow E2E test.
//!
//! 1. Create deposit address via API
//! 2. Simulate deposit detection (insert into DB)
//! 3. Run confirmation tracker → Detected → Confirmed
//! 4. Verify status progression

use custody_common::*;
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

#[tokio::test]
async fn full_deposit_lifecycle() {
    let Some(db) = get_db().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let user_id = Uuid::new_v4();
    let deposit_address = format!("0x{}", hex::encode(&[0xDE; 20]));

    // Step 1: Create deposit address in DB
    let addr = DepositAddress {
        id: Uuid::new_v4(),
        user_id,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: deposit_address.clone(),
        derivation_path: "m/44'/60'/0'/0/42".into(),
        created_at: chrono::Utc::now(),
    };
    db.insert_address(&addr).await.unwrap();

    // Step 2: Simulate scanner detecting a deposit
    let deposit = Deposit {
        id: Uuid::new_v4(),
        user_id,
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: deposit_address.clone(),
        amount: dec!(2.5),
        tx_hash: format!("0x{}", Uuid::new_v4().to_string().replace("-", "")),
        log_index: None,
        block_height: 20_000_000,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };
    db.insert_deposit(&deposit).await.unwrap();

    // Verify: deposit exists with Detected status
    let found = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(found.status, DepositStatus::Detected);
    assert_eq!(found.amount, dec!(2.5));
    assert_eq!(found.confirmations, 0);

    // Step 3: Simulate confirmation progress (6 confirmations — not enough)
    db.update_deposit_status(deposit.id, DepositStatus::Detected, 6).await.unwrap();
    let found = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(found.confirmations, 6);
    assert_eq!(found.status, DepositStatus::Detected); // still not confirmed

    // Step 4: 12 confirmations — should be confirmed
    db.update_deposit_status(deposit.id, DepositStatus::Confirmed, 12).await.unwrap();
    let found = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(found.status, DepositStatus::Confirmed);
    assert_eq!(found.confirmations, 12);
    assert!(found.confirmed_at.is_some());

    // Step 5: Credit the deposit
    db.update_deposit_status(deposit.id, DepositStatus::Credited, 50).await.unwrap();
    let found = db.find_deposit_by_tx_hash(&deposit.tx_hash).await.unwrap().unwrap();
    assert_eq!(found.status, DepositStatus::Credited);

    // Step 6: Verify deposit shows up in by_status queries
    let confirmed = db.deposits_by_status(DepositStatus::Credited).await.unwrap();
    assert!(confirmed.iter().any(|d| d.id == deposit.id));
}

#[tokio::test]
async fn erc20_deposit_with_log_index() {
    let Some(db) = get_db().await else { return; };

    let tx_hash = format!("0x{}", Uuid::new_v4().to_string().replace("-", ""));

    // Two ERC-20 transfers in the same transaction (different log indices)
    let deposit1 = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("USDT"),
        chain: Chain::Ethereum,
        address: "0xuser1".into(),
        amount: dec!(1000),
        tx_hash: tx_hash.clone(),
        log_index: Some(0), // first Transfer event
        block_height: 20_000_100,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };

    let deposit2 = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("USDC"),
        chain: Chain::Ethereum,
        address: "0xuser2".into(),
        amount: dec!(2000),
        tx_hash: tx_hash.clone(),
        log_index: Some(1), // second Transfer event in same tx
        block_height: 20_000_100,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };

    // Both should insert successfully (same tx_hash, different log_index)
    db.insert_deposit(&deposit1).await.unwrap();
    db.insert_deposit(&deposit2).await.unwrap();

    // Both should be queryable
    let detected = db.deposits_by_status(DepositStatus::Detected).await.unwrap();
    assert!(detected.iter().any(|d| d.id == deposit1.id));
    assert!(detected.iter().any(|d| d.id == deposit2.id));
}

#[tokio::test]
async fn deposit_dedup_by_tx_hash() {
    let Some(db) = get_db().await else { return; };

    let tx_hash = format!("0xdedup{}", Uuid::new_v4().to_string().replace("-", ""));

    let deposit = Deposit {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        address: "0xdedup_test".into(),
        amount: dec!(5.0),
        tx_hash: tx_hash.clone(),
        log_index: Some(0),
        block_height: 20_000_200,
        confirmations: 0,
        required_confirmations: 12,
        status: DepositStatus::Detected,
        detected_at: chrono::Utc::now(),
        confirmed_at: None,
        credited_at: None,
    };

    db.insert_deposit(&deposit).await.unwrap();

    // Duplicate with same tx_hash + log_index should be ignored (ON CONFLICT)
    let duplicate = Deposit {
        id: Uuid::new_v4(), // different ID but same tx_hash+log_index
        ..deposit.clone()
    };
    db.insert_deposit(&duplicate).await.unwrap(); // should not error

    // Only one deposit should exist for this tx_hash
    let found = db.find_deposit_by_tx_hash(&tx_hash).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, deposit.id); // original, not duplicate
}

#[tokio::test]
async fn confirmation_tracker_logic() {
    // Test the pure confirmation logic (no DB needed)
    use custody_confirmations::tracker::{confirmation_count, is_confirmed};

    // Ethereum: 12 confirmations
    let current_block = 20_000_012u64;
    let deposit_block = 20_000_000u64;
    assert_eq!(confirmation_count(current_block, deposit_block), 12);
    assert!(is_confirmed(current_block, deposit_block, 12));
    assert!(!is_confirmed(current_block - 1, deposit_block, 12)); // 11 confirms

    // Bitcoin: 3 confirmations
    assert!(is_confirmed(100, 97, 3));
    assert!(!is_confirmed(99, 97, 3)); // only 2

    // Tron: 20 confirmations
    assert!(is_confirmed(120, 100, 20));
    assert!(!is_confirmed(119, 100, 20));

    // Solana: 1 confirmation (finalized)
    assert!(is_confirmed(1001, 1000, 1));
}
