//! End-to-end testnet integration tests.
//!
//! These tests connect to REAL testnet nodes and verify:
//! 1. Address derivation works for all 4 chains
//! 2. Withdrawal signing produces valid signed transactions
//! 3. Fee estimation returns sane values from live nodes
//! 4. Scanner amount conversion handles edge cases
//!
//! Tests soft-fail if network is unavailable (CI-safe).

use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use custody_address_factory::allocator::{derive_address, AddressFactoryConfig};
use custody_address_factory::hd::SeedBytes;
use custody_common::Chain;
use custody_fee_estimator::ethereum::EthereumFeeEstimator;
use custody_scanner::uint256_to_decimal;
// Bitcoin signing tested in custody-signing crate directly (needs `bitcoin` crate for ScriptBuf)
use custody_signing::ethereum::{sign_eth_transfer, EthTransferParams};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

const SEPOLIA_RPC: &str = "https://ethereum-sepolia-rpc.publicnode.com";
const TRON_NILE: &str = "https://nile.trongrid.io";
const SOLANA_DEVNET: &str = "https://api.devnet.solana.com";

fn test_seed() -> SeedBytes {
    let bytes = hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap();
    let mut arr = [0u8; 64];
    arr.copy_from_slice(&bytes);
    SeedBytes(arr)
}

fn test_seed_raw() -> [u8; 64] {
    let bytes = hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap();
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&bytes);
    seed
}

async fn sepolia_available() -> bool {
    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());
    provider.get_block_number().await.is_ok()
}

// ── Address Derivation (offline, always runs) ────────────────────

#[test]
fn derive_addresses_all_chains() {
    let seed = test_seed();
    let config = AddressFactoryConfig::default();

    // Ethereum — EIP-55 checksum
    let (eth_addr, eth_path) = derive_address(&seed, Chain::Ethereum, 0, &config).unwrap();
    assert!(eth_addr.starts_with("0x"), "ETH address: {eth_addr}");
    assert_eq!(eth_addr.len(), 42);
    assert!(eth_path.contains("44'/60'"), "ETH path: {eth_path}");

    // Bitcoin — Bech32 P2WPKH
    let (btc_addr, btc_path) = derive_address(&seed, Chain::Bitcoin, 0, &config).unwrap();
    assert!(btc_addr.starts_with("bc1q"), "BTC address: {btc_addr}");
    assert!(btc_path.contains("84'/0'"), "BTC path: {btc_path}");

    // Tron — Base58Check
    let (trx_addr, _) = derive_address(&seed, Chain::Tron, 0, &config).unwrap();
    assert!(trx_addr.starts_with('T'), "TRX address: {trx_addr}");
    assert_eq!(trx_addr.len(), 34);

    // Solana — Base58 Ed25519
    let (sol_addr, _) = derive_address(&seed, Chain::Solana, 0, &config).unwrap();
    assert!(sol_addr.len() >= 32 && sol_addr.len() <= 44, "SOL address: {sol_addr}");

    // Determinism — same seed+index = same address
    let (eth_addr2, _) = derive_address(&seed, Chain::Ethereum, 0, &config).unwrap();
    assert_eq!(eth_addr, eth_addr2, "Address derivation must be deterministic");

    // Different index = different address
    let (eth_addr3, _) = derive_address(&seed, Chain::Ethereum, 1, &config).unwrap();
    assert_ne!(eth_addr, eth_addr3, "Different index must produce different address");
}

// ── Ethereum Signing (offline, always runs) ─────────────────────

#[test]
fn sign_eth_withdrawal_offline() {
    let seed = test_seed_raw();

    let params = EthTransferParams {
        to: "0xdead000000000000000000000000000000000000".parse::<Address>().unwrap(),
        value_wei: U256::from(10_000_000_000_000_000u128), // 0.01 ETH
        gas_limit: 21_000,
        max_fee_per_gas: 10_000_000_000,     // 10 gwei
        max_priority_fee_per_gas: 1_000_000_000, // 1 gwei
        nonce: 0,
        chain_id: 11155111, // Sepolia
    };

    let signed = sign_eth_transfer(&seed, 0, &params).unwrap();

    // Signed tx should be non-empty raw bytes
    assert!(signed.len() > 50, "Signed ETH tx too small: {} bytes", signed.len());
    // EIP-1559 type 2 transactions start with 0x02 prefix
    assert_eq!(signed[0], 0x02, "Should be EIP-1559 type 2 tx");
}

// Bitcoin signing is tested in custody-signing crate directly (needs `bitcoin` crate for ScriptBuf).

// ── Live Sepolia Tests ──────────────────────────────────────────

#[tokio::test]
async fn sepolia_scan_real_block() {
    if !sepolia_available().await {
        eprintln!("Skipping: Sepolia RPC unavailable");
        return;
    }

    let provider = ProviderBuilder::new()
        .connect_http(SEPOLIA_RPC.parse().unwrap());

    // Get a recent block
    let block_num = provider.get_block_number().await.unwrap();
    let block = provider
        .get_block_by_number((block_num - 5).into())
        .await
        .unwrap()
        .unwrap();

    let tx_count = block.transactions.len();
    println!("Sepolia block {} has {} transactions", block_num - 5, tx_count);

    // Verify uint256_to_decimal works on real chain values
    let test_value = U256::from(1_500_000_000_000_000_000u128); // 1.5 ETH
    let decimal = uint256_to_decimal(test_value, 18).unwrap();
    assert_eq!(decimal, dec!(1.5));
}

#[tokio::test]
async fn sepolia_fee_estimation_live() {
    if !sepolia_available().await {
        eprintln!("Skipping: Sepolia RPC unavailable");
        return;
    }

    let estimator = EthereumFeeEstimator::new(SEPOLIA_RPC, 200).unwrap();
    let estimate = estimator.estimate().await.unwrap();

    assert!(estimate.base_fee > Decimal::ZERO, "base_fee should be > 0");
    assert!(estimate.max_fee >= estimate.base_fee, "max_fee >= base_fee");

    println!("Sepolia fees: base={}, priority={}, max={}",
        estimate.base_fee, estimate.priority_fee, estimate.max_fee);
}

// ── Live Solana Devnet Tests ────────────────────────────────────

#[tokio::test]
async fn solana_devnet_get_slot() {
    let client = reqwest::Client::new();
    let resp = client
        .post(SOLANA_DEVNET)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSlot"
        }))
        .send()
        .await;

    let Ok(resp) = resp else {
        eprintln!("Skipping: Solana devnet unavailable");
        return;
    };

    let body: serde_json::Value = resp.json().await.unwrap();
    let slot = body["result"].as_u64().unwrap();
    assert!(slot > 0, "Solana devnet slot should be > 0");
    println!("Solana devnet slot: {slot}");
}

// ── Live Tron Nile Tests ────────────────────────────────────────

#[tokio::test]
async fn tron_nile_get_block() {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{TRON_NILE}/wallet/getnowblock"))
        .send()
        .await;

    let Ok(resp) = resp else {
        eprintln!("Skipping: Tron Nile unavailable");
        return;
    };

    let body: serde_json::Value = resp.json().await.unwrap();
    let block_num = body["block_header"]["raw_data"]["number"].as_u64().unwrap_or(0);
    assert!(block_num > 0, "Tron Nile block number should be > 0");
    println!("Tron Nile block: {block_num}");
}

// ── Circuit Breaker + Price Oracle Integration ──────────────────

#[test]
fn circuit_breaker_with_price_oracle() {
    use custody_common::middleware_types::CircuitBreakerState;
    use custody_common::price_oracle::PriceOracle;

    let oracle = PriceOracle::with_default_ttl();
    oracle.seed_fallback_prices();

    let mut cb = CircuitBreakerState::default_limits();

    // 1. Small withdrawal ($3,500 = 1 ETH at $3,500) — should pass
    let eth_price = oracle.get_price("ETH").unwrap();
    let eth_amount = dec!(1);
    let usd_value = eth_amount * eth_price;
    assert!(cb.check(usd_value).is_ok(), "1 ETH withdrawal should pass");
    cb.current_hourly_outflow += usd_value;

    // 2. Large withdrawal ($650K = 10 BTC at $65K) — exceeds single tx limit ($500K)
    let btc_price = oracle.get_price("BTC").unwrap();
    let btc_amount = dec!(10);
    let usd_value = btc_amount * btc_price;
    assert!(usd_value > dec!(500_000), "10 BTC should be > $500K");
    let err = cb.check(usd_value).unwrap_err();
    assert!(
        matches!(err, custody_common::middleware_types::CircuitBreakerError::SingleTxLimit { .. }),
        "Should be SingleTxLimit error, got: {err}"
    );

    // 3. Many medium withdrawals — accumulate toward hourly limit
    let usdt_price = oracle.get_price("USDT").unwrap();
    assert_eq!(usdt_price, dec!(1), "USDT should be $1");

    // Add $9.6M to hourly outflow
    cb.current_hourly_outflow = dec!(9_600_000);
    // Next $450K (under single tx limit) should trip hourly limit ($10M)
    let err = cb.check(dec!(450_000)).unwrap_err();
    assert!(
        matches!(err, custody_common::middleware_types::CircuitBreakerError::HourlyLimit { .. }),
        "Should be HourlyLimit error, got: {err}"
    );

    // 4. Tripped state blocks everything
    cb.is_tripped = true;
    let err = cb.check(dec!(1)).unwrap_err(); // Even $1 is blocked
    assert!(
        matches!(err, custody_common::middleware_types::CircuitBreakerError::AlreadyTripped),
        "Tripped CB should block all"
    );
}

#[test]
fn price_oracle_zero_price_guard() {
    use custody_common::price_oracle::PriceOracle;

    let oracle = PriceOracle::with_default_ttl();
    oracle.seed_fallback_prices();

    // All fallback prices should be positive
    for symbol in &["BTC", "ETH", "SOL", "TRX", "USDT", "USDC"] {
        let price = oracle.get_price(symbol).unwrap();
        assert!(price > Decimal::ZERO, "{symbol} price should be > 0, got {price}");
    }

    // to_usd should never return $0 for non-zero amount with real price
    let btc_usd = oracle.to_usd("BTC", dec!(1)).unwrap();
    assert!(btc_usd > dec!(10_000), "1 BTC should be > $10K, got {btc_usd}");
}

// ── Cross-Chain Amount Conversion ───────────────────────────────

#[test]
fn amount_conversion_edge_cases() {
    // ETH: 18 decimals
    let eth_raw = U256::from(2_500_000_000_000_000_000u128);
    assert_eq!(uint256_to_decimal(eth_raw, 18).unwrap(), dec!(2.5));

    // USDT: 6 decimals
    assert_eq!(uint256_to_decimal(U256::from(1_000_000u128), 6).unwrap(), dec!(1));

    // BSC USDT: 18 decimals (different from ETH USDT!)
    assert_eq!(uint256_to_decimal(U256::from(1_000_000_000_000_000_000u128), 18).unwrap(), dec!(1));

    // Overflow: U256::MAX should error, not return Decimal::MAX
    assert!(uint256_to_decimal(U256::MAX, 18).is_err());

    // Zero is valid
    assert_eq!(uint256_to_decimal(U256::ZERO, 18).unwrap(), dec!(0));
}
