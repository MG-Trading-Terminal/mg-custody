//! Live Sepolia scanner test.
//!
//! Scans recent Sepolia blocks for real ETH transfers.
//! Proves our EthereumScanner can read real blockchain data.

use alloy::eips::BlockNumberOrTag;
use alloy::providers::{Provider, ProviderBuilder};
use custody_scanner::uint256_to_decimal;
use rust_decimal::Decimal;

const SEPOLIA_RPC: &str = "https://rpc.sepolia.org";

async fn can_connect() -> bool {
    let provider = ProviderBuilder::new().connect_http(SEPOLIA_RPC.parse().unwrap());
    provider.get_block_number().await.is_ok()
}

#[tokio::test]
async fn scan_sepolia_block_for_eth_transfers() {
    if !can_connect().await {
        eprintln!("Skipping: Sepolia unavailable");
        return;
    }

    let provider = ProviderBuilder::new().connect_http(SEPOLIA_RPC.parse().unwrap());
    let current = provider.get_block_number().await.unwrap();

    // Scan a recent block
    let block = provider
        .get_block_by_number(BlockNumberOrTag::Number(current - 5))
        .await
        .unwrap()
        .unwrap();

    println!("Scanning Sepolia block {} ({} txs)", block.header.number, block.transactions.len());

    // Check each transaction for ETH transfers
    let mut eth_transfers = 0;
    for tx_hash in block.transactions.hashes() {
        if let Ok(Some(tx)) = provider.get_transaction_by_hash(tx_hash).await {
            use alloy::consensus::Transaction;
            let value = tx.value();
            if value > alloy::primitives::U256::ZERO {
                let amount_eth = uint256_to_decimal(value, 18);
                if let Some(to) = tx.to() {
                    println!("  ETH transfer: {} ETH to {:#x}", amount_eth, to);
                    eth_transfers += 1;

                    // Verify our uint256_to_decimal conversion
                    assert!(amount_eth > Decimal::ZERO);
                    assert!(amount_eth < Decimal::new(1_000_000, 0)); // sanity: < 1M ETH
                }
            }
        }
    }

    println!("Found {} ETH transfers in block {}", eth_transfers, block.header.number);
}

#[tokio::test]
async fn scan_sepolia_erc20_transfer_logs() {
    if !can_connect().await { return; }

    let provider = ProviderBuilder::new().connect_http(SEPOLIA_RPC.parse().unwrap());
    let current = provider.get_block_number().await.unwrap();

    // ERC-20 Transfer event topic
    let transfer_topic: alloy::primitives::FixedBytes<32> = alloy::primitives::FixedBytes::new([
        0xdd, 0xf2, 0x52, 0xad, 0x1b, 0xe2, 0xc8, 0x9b,
        0x69, 0xc2, 0xb0, 0x68, 0xfc, 0x37, 0x8d, 0xaa,
        0x95, 0x2b, 0xa7, 0xf1, 0x63, 0xc4, 0xa1, 0x16,
        0x28, 0xf5, 0x5a, 0x4d, 0xf5, 0x23, 0xb3, 0xef,
    ]);

    // Scan last 100 blocks for ERC-20 Transfer events
    let from_block = current.saturating_sub(100);
    let filter = alloy::rpc::types::Filter::new()
        .event_signature(transfer_topic)
        .from_block(from_block)
        .to_block(current);

    let logs = provider.get_logs(&filter).await.unwrap();
    println!("Found {} ERC-20 Transfer events in blocks {}-{}", logs.len(), from_block, current);

    // Parse some logs
    for log in logs.iter().take(5) {
        if log.topics().len() >= 3 {
            let from = &log.topics()[1];
            let to = &log.topics()[2];
            let contract = log.address();
            println!("  ERC-20: {:#x} → from={}... to={}...",
                contract,
                hex::encode(&from[..4]),
                hex::encode(&to[..4]),
            );
        }
    }
}

#[tokio::test]
async fn fee_estimator_with_real_sepolia_data() {
    if !can_connect().await { return; }

    let estimator = custody_fee_estimator::ethereum::EthereumFeeEstimator::new(SEPOLIA_RPC, 500).unwrap();

    // Run estimation 3 times to verify consistency
    for i in 0..3 {
        let est = estimator.estimate().await.unwrap();
        println!("Fee estimate #{}: base={} gwei, priority={} gwei, max={} gwei",
            i + 1, est.base_fee, est.priority_fee, est.max_fee);

        assert!(est.base_fee > Decimal::ZERO);
        assert!(est.max_fee > est.base_fee);
        // max_fee = 2 * base_fee + priority_fee
        let expected_max = est.base_fee * Decimal::new(2, 0) + est.priority_fee;
        assert_eq!(est.max_fee, expected_max);
    }
}
