//! Withdrawal security tests — full pipeline logic without DB.
//!
//! Tests cover:
//! 1. Address validation (valid/invalid for all chains, cross-chain rejection)
//! 2. Circuit breaker enforcement (single, hourly, daily limits)
//! 3. ERC-20 USDT/USDC signing (calldata encoding + signing)
//! 4. Webhook HMAC signature integrity (sign + verify + tamper detection)
//! 5. Full pipeline: validate → circuit breaker → sign → webhook

use alloy::primitives::{Address, U256};
use custody_common::{Asset, Chain, CustodyError, WalletTier, Withdrawal, WithdrawalStatus};
use custody_confirmations::webhook::compute_hmac;
use custody_signing::ethereum::{
    encode_erc20_transfer_calldata, sign_erc20_transfer, sign_eth_transfer,
    Erc20TransferParams, EthTransferParams,
};
use custody_withdrawal_pipeline::validation::{
    check_circuit_breaker, validate_withdrawal,
};
use rust_decimal_macros::dec;
use uuid::Uuid;

fn test_seed() -> [u8; 64] {
    let bytes = hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap();
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&bytes);
    seed
}

fn test_withdrawal(chain: Chain, address: &str, amount: rust_decimal::Decimal) -> Withdrawal {
    Withdrawal {
        id: Uuid::new_v4(),
        exchange_withdrawal_id: Uuid::new_v4(),
        asset: Asset::new("ETH"),
        chain,
        destination_address: address.into(),
        amount,
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
    }
}

// ── Address Validation ──────────────────────────────────────────

#[test]
fn valid_addresses_all_chains() {
    let eth = test_withdrawal(
        Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(1.0),
    );
    assert!(validate_withdrawal(&eth).is_ok());

    let btc = test_withdrawal(Chain::Bitcoin, "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq", dec!(0.1));
    assert!(validate_withdrawal(&btc).is_ok());

    let btc_legacy = test_withdrawal(Chain::Bitcoin, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", dec!(0.1));
    assert!(validate_withdrawal(&btc_legacy).is_ok());

    let trx = test_withdrawal(Chain::Tron, "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t", dec!(100));
    assert!(validate_withdrawal(&trx).is_ok());

    let sol = test_withdrawal(Chain::Solana, "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM", dec!(5));
    assert!(validate_withdrawal(&sol).is_ok());
}

#[test]
fn reject_wrong_chain_addresses() {
    // ETH address for BTC withdrawal
    let btc_with_eth = test_withdrawal(
        Chain::Bitcoin, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(0.1),
    );
    assert!(validate_withdrawal(&btc_with_eth).is_err());

    // BTC address for ETH withdrawal
    let eth_with_btc = test_withdrawal(
        Chain::Ethereum, "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq", dec!(1.0),
    );
    assert!(validate_withdrawal(&eth_with_btc).is_err());

    // BTC address for Tron
    let trx_with_btc = test_withdrawal(Chain::Tron, "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq", dec!(1));
    assert!(validate_withdrawal(&trx_with_btc).is_err());
}

#[test]
fn reject_malformed_addresses() {
    let empty = test_withdrawal(Chain::Ethereum, "", dec!(1));
    assert!(validate_withdrawal(&empty).is_err());

    let short = test_withdrawal(Chain::Ethereum, "0x1234", dec!(1));
    assert!(validate_withdrawal(&short).is_err());

    let nonhex = test_withdrawal(
        Chain::Ethereum, "0xZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ", dec!(1),
    );
    assert!(validate_withdrawal(&nonhex).is_err());

    let short_sol = test_withdrawal(Chain::Solana, "abc", dec!(1));
    assert!(validate_withdrawal(&short_sol).is_err());
}

#[test]
fn reject_zero_negative_amount() {
    let zero = test_withdrawal(Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(0));
    assert!(validate_withdrawal(&zero).is_err());

    let negative = test_withdrawal(Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(-1));
    assert!(validate_withdrawal(&negative).is_err());
}

// ── Circuit Breaker ─────────────────────────────────────────────

#[test]
fn circuit_breaker_allows_normal() {
    assert!(check_circuit_breaker(dec!(10_000), dec!(500_000), dec!(0), dec!(10_000_000), dec!(0), dec!(50_000_000)).is_ok());
}

#[test]
fn circuit_breaker_blocks_single_over_limit() {
    let err = check_circuit_breaker(dec!(600_000), dec!(500_000), dec!(0), dec!(10_000_000), dec!(0), dec!(50_000_000)).unwrap_err();
    assert!(matches!(err, CustodyError::CircuitBreakerTripped { .. }));
}

#[test]
fn circuit_breaker_blocks_hourly_accumulation() {
    let err = check_circuit_breaker(dec!(400_000), dec!(500_000), dec!(9_700_000), dec!(10_000_000), dec!(0), dec!(50_000_000)).unwrap_err();
    assert!(matches!(err, CustodyError::CircuitBreakerTripped { .. }));
}

#[test]
fn circuit_breaker_blocks_daily_accumulation() {
    let err = check_circuit_breaker(dec!(400_000), dec!(500_000), dec!(0), dec!(10_000_000), dec!(49_700_000), dec!(50_000_000)).unwrap_err();
    assert!(matches!(err, CustodyError::CircuitBreakerTripped { .. }));
}

// ── ERC-20 Signing ──────────────────────────────────────────────

#[test]
fn erc20_transfer_calldata_encoding() {
    let to: Address = "0xdead000000000000000000000000000000000000".parse().unwrap();
    let amount = U256::from(1_000_000u64); // 1 USDT

    let calldata = encode_erc20_transfer_calldata(to, amount);
    assert_eq!(calldata.len(), 68, "ERC-20 calldata must be 68 bytes");
    assert_eq!(&calldata[..4], &[0xa9, 0x05, 0x9c, 0xbb]); // transfer selector
    assert_eq!(&calldata[4..16], &[0u8; 12]); // zero-padded address
    assert_eq!(calldata[16], 0xde);

    let decoded_amount = U256::from_be_slice(&calldata[36..68]);
    assert_eq!(decoded_amount, U256::from(1_000_000u64));
}

#[test]
fn sign_usdt_withdrawal() {
    let seed = test_seed();
    let usdt_contract: Address = "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse().unwrap();
    let recipient: Address = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".parse().unwrap();

    let params = Erc20TransferParams {
        token_contract: usdt_contract,
        to: recipient,
        amount: U256::from(100_000_000u64), // 100 USDT
        nonce: 42,
        chain_id: 1,
        max_fee_per_gas: 50_000_000_000,
        max_priority_fee_per_gas: 2_000_000_000,
        gas_limit: 65_000,
    };

    let signed = sign_erc20_transfer(&seed, 0, &params).unwrap();
    assert_eq!(signed[0], 0x02); // EIP-1559
    assert!(signed.len() > 100);
}

#[test]
fn usdt_vs_usdc_different_txs() {
    let seed = test_seed();
    let usdt: Address = "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse().unwrap();
    let usdc: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap();
    let to: Address = "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".parse().unwrap();

    let usdt_tx = sign_erc20_transfer(&seed, 0, &Erc20TransferParams {
        token_contract: usdt, to, amount: U256::from(1_000_000u64),
        nonce: 0, chain_id: 1, ..Default::default()
    }).unwrap();

    let usdc_tx = sign_erc20_transfer(&seed, 0, &Erc20TransferParams {
        token_contract: usdc, to, amount: U256::from(1_000_000u64),
        nonce: 0, chain_id: 1, ..Default::default()
    }).unwrap();

    assert_ne!(usdt_tx, usdc_tx, "USDT and USDC txs must differ");
}

// ── Webhook HMAC Signatures ─────────────────────────────────────

#[test]
fn webhook_hmac_deterministic() {
    let sig1 = compute_hmac(b"secret", "POST", "/events", "1700000000", r#"{"event":"ok"}"#);
    let sig2 = compute_hmac(b"secret", "POST", "/events", "1700000000", r#"{"event":"ok"}"#);
    assert_eq!(sig1, sig2, "HMAC must be deterministic");
    assert_eq!(sig1.len(), 64, "SHA-256 hex = 64 chars");
}

#[test]
fn webhook_hmac_tamper_detection() {
    let sig_ok = compute_hmac(b"secret", "POST", "/events", "1700000000", r#"{"amount":"100"}"#);
    let sig_tampered = compute_hmac(b"secret", "POST", "/events", "1700000000", r#"{"amount":"999"}"#);
    assert_ne!(sig_ok, sig_tampered, "Tampered body must produce different HMAC");
}

#[test]
fn webhook_hmac_wrong_secret() {
    let sig1 = compute_hmac(b"secret_1", "POST", "/events", "1700000000", "body");
    let sig2 = compute_hmac(b"secret_2", "POST", "/events", "1700000000", "body");
    assert_ne!(sig1, sig2, "Wrong secret must produce different HMAC");
}

#[test]
fn webhook_hmac_timestamp_bound() {
    let sig_t1 = compute_hmac(b"secret", "POST", "/events", "1700000000", "body");
    let sig_t2 = compute_hmac(b"secret", "POST", "/events", "1700000001", "body");
    assert_ne!(sig_t1, sig_t2, "Different timestamp must produce different HMAC (replay protection)");
}

// ── Full Pipeline (no DB) ───────────────────────────────────────

#[test]
fn full_eth_withdrawal_pipeline() {
    let seed = test_seed();

    // 1. Validate
    let withdrawal = test_withdrawal(Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(2.5));
    validate_withdrawal(&withdrawal).expect("Validation should pass");

    // 2. Circuit breaker ($8,750 at $3,500/ETH)
    check_circuit_breaker(dec!(8750), dec!(500_000), dec!(0), dec!(10_000_000), dec!(0), dec!(50_000_000))
        .expect("Circuit breaker should pass");

    // 3. Sign
    let params = EthTransferParams {
        to: withdrawal.destination_address.parse::<Address>().unwrap(),
        value_wei: U256::from(2_500_000_000_000_000_000u128),
        gas_limit: 21_000,
        max_fee_per_gas: 30_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        nonce: 0,
        chain_id: 1,
    };
    let signed_tx = sign_eth_transfer(&seed, 0, &params).unwrap();
    assert_eq!(signed_tx[0], 0x02);

    // 4. Webhook HMAC
    let body = format!(r#"{{"event":"withdrawal_broadcast","id":"{}"}}"#, withdrawal.id);
    let hmac = compute_hmac(b"webhook_secret", "POST", "/events", "1700000000", &body);
    assert_eq!(hmac.len(), 64);
}

#[test]
fn full_usdt_withdrawal_pipeline() {
    let seed = test_seed();

    // 1. Validate (USDT is on Ethereum)
    let mut withdrawal = test_withdrawal(Chain::Ethereum, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18", dec!(1000));
    withdrawal.asset = Asset::new("USDT");
    validate_withdrawal(&withdrawal).expect("Validation should pass");

    // 2. Circuit breaker ($1,000 USDT = $1,000 USD)
    check_circuit_breaker(dec!(1000), dec!(500_000), dec!(0), dec!(10_000_000), dec!(0), dec!(50_000_000))
        .expect("Circuit breaker should pass");

    // 3. Sign ERC-20 transfer
    let usdt_contract: Address = "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse().unwrap();
    let params = Erc20TransferParams {
        token_contract: usdt_contract,
        to: withdrawal.destination_address.parse::<Address>().unwrap(),
        amount: U256::from(1_000_000_000u64), // 1000 USDT (6 decimals)
        nonce: 0,
        chain_id: 1,
        max_fee_per_gas: 30_000_000_000,
        max_priority_fee_per_gas: 1_000_000_000,
        gas_limit: 65_000,
    };
    let signed_tx = sign_erc20_transfer(&seed, 0, &params).unwrap();
    assert_eq!(signed_tx[0], 0x02);
    assert!(signed_tx.len() > 100);
}
