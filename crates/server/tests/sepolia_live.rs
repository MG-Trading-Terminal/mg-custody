//! Live Sepolia testnet integration test.
//!
//! Connects to real Sepolia Ethereum testnet and:
//! 1. Gets current block number
//! 2. Reads a block
//! 3. Gets gas price
//! 4. Verifies our fee estimator works with real data
//!
//! Skip if no network or RPC is down.

use alloy::providers::{Provider, ProviderBuilder};
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use rust_decimal::Decimal;

const SEPOLIA_RPC: &str = "https://rpc.sepolia.org";

async fn can_connect() -> bool {
    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());
    provider.get_block_number().await.is_ok()
}

#[tokio::test]
async fn sepolia_get_block_number() {
    if !can_connect().await {
        eprintln!("Skipping: cannot connect to Sepolia RPC");
        return;
    }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    let block_number = provider.get_block_number().await.unwrap();

    // Sepolia should be well past 5 million blocks by 2026
    assert!(block_number > 5_000_000,
        "Sepolia block number should be > 5M, got {block_number}");

    println!("Sepolia block number: {block_number}");
}

#[tokio::test]
async fn sepolia_get_latest_block() {
    if !can_connect().await { return; }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    let block = provider
        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
        .await
        .unwrap()
        .unwrap();

    // Block should have valid data
    assert!(block.header.number > 0);
    assert!(block.header.base_fee_per_gas.is_some(),
        "Sepolia should have EIP-1559 base fee");

    let base_fee = block.header.base_fee_per_gas.unwrap();
    println!("Sepolia latest block: {} (base_fee: {} wei)", block.header.number, base_fee);
}

#[tokio::test]
async fn sepolia_fee_estimator() {
    if !can_connect().await { return; }

    let estimator = EthereumFeeEstimator::new(SEPOLIA_RPC, 200).unwrap();
    let estimate = estimator.estimate().await.unwrap();

    // Fee should be positive
    assert!(estimate.base_fee > Decimal::ZERO,
        "Base fee should be > 0, got {}", estimate.base_fee);
    assert!(estimate.max_fee > Decimal::ZERO);
    assert!(estimate.max_fee >= estimate.base_fee,
        "Max fee should be >= base fee");

    // Gas should not exceed our cap (200 gwei) on testnet
    assert!(!estimator.gas_exceeds_cap().await.unwrap(),
        "Sepolia gas should not exceed 200 gwei cap");

    println!("Sepolia fees: base={} gwei, priority={} gwei, max={} gwei",
        estimate.base_fee, estimate.priority_fee, estimate.max_fee);
}

#[tokio::test]
async fn sepolia_chain_id() {
    if !can_connect().await { return; }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    let chain_id = provider.get_chain_id().await.unwrap();
    assert_eq!(chain_id, 11155111, "Sepolia chain ID should be 11155111");
}

#[tokio::test]
async fn sepolia_get_balance() {
    if !can_connect().await { return; }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    // Check balance of Sepolia faucet address (should have some ETH)
    let vitalik: alloy::primitives::Address =
        "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".parse().unwrap();

    let balance = provider.get_balance(vitalik).await.unwrap();
    println!("Vitalik's Sepolia balance: {} wei", balance);
    // Just verify the call works — balance might be 0 on testnet
}

#[tokio::test]
async fn sepolia_scan_recent_blocks() {
    if !can_connect().await { return; }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    let current = provider.get_block_number().await.unwrap();

    // Scan last 3 blocks for any transactions
    let mut total_txs = 0;
    for block_num in (current - 2)..=current {
        let block = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Number(block_num))
            .await
            .unwrap();

        if let Some(block) = block {
            let tx_count = block.transactions.len();
            total_txs += tx_count;
            println!("Block {}: {} transactions", block_num, tx_count);
        }
    }

    println!("Total transactions in last 3 blocks: {total_txs}");
    // Sepolia should have at least some transactions
}
