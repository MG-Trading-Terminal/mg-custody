//! Multi-chain withdrawal tests — BTC, TRX, SOL through the pipeline.

use custody_common::*;
use custody_db::CustodyDb;
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

fn test_seed() -> [u8; 64] {
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap());
    seed
}

fn make_pipeline(db: CustodyDb) -> WithdrawalPipeline {
    WithdrawalPipeline::new(db, dec!(500000), dec!(10000000), dec!(50000000))
        .with_seed(Arc::new(test_seed()))
        .with_eth_chain_id(11155111)
}

fn make_withdrawal(asset: &str, chain: Chain, address: &str, amount: rust_decimal::Decimal) -> Withdrawal {
    Withdrawal {
        id: uuid::Uuid::new_v4(),
        exchange_withdrawal_id: uuid::Uuid::new_v4(),
        user_id: uuid::Uuid::nil(),
        vault_id: None,
        asset: Asset::new(asset), chain,
        destination_address: address.into(),
        amount, fee: None,
        status: WithdrawalStatus::Received,
        wallet_tier: WalletTier::Hot,
        tx_hash: None, block_height: None, confirmations: 0,
        error_message: None, created_at: chrono::Utc::now(),
        signed_at: None, broadcast_at: None, confirmed_at: None, delay_release_at: None,
                memo: None, destination_tag: None,
    }
}

#[tokio::test]
async fn btc_withdrawal_signed() {
    let Some(db) = get_db().await else { return; };
    let w = make_withdrawal("BTC", Chain::Bitcoin, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu", dec!(0.001));
    db.insert_withdrawal(&w).await.unwrap();

    let pipeline = make_pipeline(db.clone());
    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Signed, "BTC should be Signed, got {:?}", updated.status);

    let tx = db.get_signed_tx_bytes(w.id).await.unwrap();
    assert!(tx.is_some(), "BTC signed tx should be stored");
    let tx = tx.unwrap();
    assert!(tx.len() > 100, "BTC tx should be substantial: {} bytes", tx.len());
}

#[tokio::test]
async fn trx_withdrawal_signed() {
    let Some(db) = get_db().await else { return; };
    let w = make_withdrawal("TRX", Chain::Tron, "TLyqzVGLV1srkB7dToTAEqgDSfPtXRJZYH", dec!(100));
    db.insert_withdrawal(&w).await.unwrap();

    let pipeline = make_pipeline(db.clone());
    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Signed, "TRX should be Signed, got {:?}", updated.status);
}

#[tokio::test]
async fn sol_withdrawal_signed() {
    let Some(db) = get_db().await else { return; };
    // Valid Solana address (base58, 32 bytes)
    let w = make_withdrawal("SOL", Chain::Solana, "11111111111111111111111111111111", dec!(1.0));
    db.insert_withdrawal(&w).await.unwrap();

    let pipeline = make_pipeline(db.clone());
    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
    assert_eq!(updated.status, WithdrawalStatus::Signed, "SOL should be Signed, got {:?}", updated.status);
}

#[tokio::test]
async fn all_four_chains_sign() {
    let Some(db) = get_db().await else { return; };

    let withdrawals = vec![
        make_withdrawal("ETH", Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(0.1)),
        make_withdrawal("BTC", Chain::Bitcoin, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu", dec!(0.001)),
        make_withdrawal("TRX", Chain::Tron, "TLyqzVGLV1srkB7dToTAEqgDSfPtXRJZYH", dec!(100)),
        make_withdrawal("SOL", Chain::Solana, "11111111111111111111111111111111", dec!(1.0)),
    ];

    for w in &withdrawals {
        db.insert_withdrawal(w).await.unwrap();
    }

    let pipeline = make_pipeline(db.clone());
    pipeline.tick().await.unwrap();
    pipeline.tick().await.unwrap();

    let mut signed = 0;
    for w in &withdrawals {
        let updated = db.find_withdrawal(w.id).await.unwrap().unwrap();
        if updated.status == WithdrawalStatus::Signed {
            signed += 1;
            println!("{:?} {} → Signed ✅", w.chain, w.asset.as_str());
        } else {
            println!("{:?} {} → {:?}", w.chain, w.asset.as_str(), updated.status);
        }
    }

    assert_eq!(signed, 4, "All 4 chains should sign, got {signed}/4");
}
