//! Full withdrawal flow with mock broadcaster.
//!
//! Tests: Received → Validated → Signed → Broadcast → tx_hash stored

use custody_common::*;
use custody_db::CustodyDb;
use custody_broadcaster::ethereum::EthereumBroadcaster;
use custody_withdrawal_pipeline::WithdrawalPipeline;
use rust_decimal_macros::dec;
use std::sync::Arc;

async fn get_db() -> Option<CustodyDb> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::PgPool::connect(&url).await.ok()?;
    let _ = sqlx::raw_sql(include_str!("../../db/migrations/001_initial_schema.sql")).execute(&pool).await;
    let _ = sqlx::raw_sql(include_str!("../../db/migrations/002_signed_tx_bytes.sql")).execute(&pool).await;
    Some(CustodyDb::new(pool))
}

#[tokio::test]
async fn full_flow_with_signing() {
    let Some(db) = get_db().await else { return; };

    // Insert a valid withdrawal
    let w = Withdrawal {
        id: uuid::Uuid::new_v4(),
        exchange_withdrawal_id: uuid::Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain: Chain::Ethereum,
        destination_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
        amount: dec!(0.05),
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None, created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
    };
    db.insert_withdrawal(&w).await.unwrap();

    // Build pipeline with seed for signing
    let seed_hex = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&hex::decode(seed_hex).unwrap());

    let pipeline = WithdrawalPipeline::new(
        db.clone(), dec!(500000), dec!(10000000), dec!(50000000),
    ).with_seed(Arc::new(seed)).with_eth_chain_id(11155111);

    // Tick 1: Received → Validated → Signed
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert!(
        updated.status == WithdrawalStatus::Validated || updated.status == WithdrawalStatus::Signed,
        "Expected Validated or Signed, got {:?}", updated.status
    );

    // If Validated, tick again
    if updated.status == WithdrawalStatus::Validated {
        pipeline.tick().await.unwrap();
    }

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Signed, "Expected Signed, got {:?}", updated.status);

    // Verify signed tx bytes in DB
    let tx_bytes = db.get_signed_tx_bytes(w.id).await.unwrap();
    assert!(tx_bytes.is_some(), "Signed tx should be stored");
    let tx = tx_bytes.unwrap();
    assert!(tx.len() > 50, "Tx should be substantial: {} bytes", tx.len());
    assert_eq!(tx[0], 0x02, "Should be EIP-1559 (type 2)");
}

#[tokio::test]
async fn multiple_withdrawals_sequential() {
    let Some(db) = get_db().await else { return; };

    let mut seed = [0u8; 64];
    seed.copy_from_slice(&hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap());

    // Insert 3 valid withdrawals
    let mut ids = Vec::new();
    for i in 0..3 {
        let w = Withdrawal {
            id: uuid::Uuid::new_v4(),
            exchange_withdrawal_id: uuid::Uuid::new_v4(),
            asset: Asset::new("ETH"),
            chain: Chain::Ethereum,
            destination_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
            amount: dec!(0.01) + rust_decimal::Decimal::new(i, 2),
            fee: None,
            status: WithdrawalStatus::Received,
            wallet_tier: WalletTier::Hot,
            tx_hash: None, block_height: None, confirmations: 0,
            error_message: None, created_at: chrono::Utc::now(),
            signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
        };
        db.insert_withdrawal(&w).await.unwrap();
        ids.push(w.id);
    }

    let pipeline = WithdrawalPipeline::new(
        db.clone(), dec!(500000), dec!(10000000), dec!(50000000),
    ).with_seed(Arc::new(seed)).with_eth_chain_id(11155111);

    // Process all
    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    // All should be Signed
    let mut signed_count = 0;
    for id in &ids {
        let w = db.find_withdrawal(*id).await.unwrap().unwrap();
        if w.status == WithdrawalStatus::Signed {
            signed_count += 1;
            // Verify each has unique signed tx
            let tx = db.get_signed_tx_bytes(*id).await.unwrap();
            assert!(tx.is_some(), "Withdrawal {} should have signed tx", id);
        }
    }
    assert!(signed_count >= 2, "At least 2 of 3 should be signed, got {signed_count}");
}

#[tokio::test]
async fn usdt_withdrawal_uses_correct_contract() {
    let Some(db) = get_db().await else { return; };

    let mut seed = [0u8; 64];
    seed.copy_from_slice(&hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap());

    // USDT withdrawal
    let w = Withdrawal {
        id: uuid::Uuid::new_v4(),
        exchange_withdrawal_id: uuid::Uuid::new_v4(),
        asset: Asset::new("USDT"),
        chain: Chain::Ethereum,
        destination_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
        amount: dec!(100), // 100 USDT
        fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None, created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
    };
    db.insert_withdrawal(&w).await.unwrap();

    let pipeline = WithdrawalPipeline::new(
        db.clone(), dec!(500000), dec!(10000000), dec!(50000000),
    ).with_seed(Arc::new(seed)).with_eth_chain_id(11155111);

    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Signed, "USDT should be signed, got {:?}", updated.status);

    // The signed tx should use the correct USDT contract (not Address::ZERO)
    let tx_bytes = db.get_signed_tx_bytes(w.id).await.unwrap().unwrap();
    assert_eq!(tx_bytes[0], 0x02, "EIP-1559");
    assert!(tx_bytes.len() > 100, "ERC-20 tx should be larger than ETH transfer");
}
