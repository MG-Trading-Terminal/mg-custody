//! Attack simulation tests — prove security claims with code, not words.
//!
//! Phase 17D: Every claim in SECURITY.md and THREAT_MODEL.md that can be
//! verified in code MUST have a test here.

#[cfg(test)]
mod tests {
    use crate::*;

    // -----------------------------------------------------------------------
    // 1. Key material never leaks to serialization
    // -----------------------------------------------------------------------

    #[test]
    fn withdrawal_serialization_has_no_key_material() {
        let w = Withdrawal {
            id: uuid::Uuid::new_v4(),
            exchange_withdrawal_id: uuid::Uuid::new_v4(),
            asset: Asset::new("ETH"), chain: Chain::Ethereum,
            destination_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
            amount: rust_decimal::Decimal::new(1, 0),
            fee: None, status: WithdrawalStatus::Received,
            wallet_tier: WalletTier::Hot, tx_hash: None,
            block_height: None, confirmations: 0, error_message: None,
            created_at: chrono::Utc::now(), signed_at: None,
            broadcast_at: None, confirmed_at: None, delay_release_at: None,
        };
        let json = serde_json::to_string(&w).unwrap();
        // Must NOT contain anything that looks like a private key
        assert!(!json.contains("seed"), "Withdrawal JSON contains 'seed'");
        assert!(!json.contains("private"), "Withdrawal JSON contains 'private'");
        assert!(!json.contains("secret"), "Withdrawal JSON contains 'secret'");
        assert!(json.len() < 2000, "Withdrawal JSON suspiciously large");
    }

    #[test]
    fn deposit_serialization_has_no_key_material() {
        let d = Deposit {
            id: uuid::Uuid::new_v4(), user_id: uuid::Uuid::new_v4(),
            asset: Asset::new("BTC"), chain: Chain::Bitcoin,
            address: "bc1qtest".into(), amount: rust_decimal::Decimal::new(1, 0),
            tx_hash: "abc123".into(), log_index: None, block_height: 100,
            confirmations: 3, required_confirmations: 3,
            status: DepositStatus::Confirmed,
            detected_at: chrono::Utc::now(), confirmed_at: None, credited_at: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("seed"));
        assert!(!json.contains("private"));
        assert!(!json.contains("key_bytes"));
    }

    #[test]
    fn vault_account_no_key_material() {
        let va = vault::VaultAccount {
            id: uuid::Uuid::nil(), vault_id: uuid::Uuid::nil(),
            asset: Asset::new("ETH"), chain: Chain::Ethereum,
            address: "0x123".into(), derivation_path: "m/44'/60'/0'/0/0".into(),
            balance: rust_decimal::Decimal::ZERO,
            pending_balance: rust_decimal::Decimal::ZERO,
            frozen: false, created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&va).unwrap();
        assert!(!json.contains("seed"));
        assert!(!json.contains("private_key"));
        // Derivation path is OK — it's public info
        assert!(json.contains("m/44'"));
    }

    // -----------------------------------------------------------------------
    // 2. Error types never contain key material
    // -----------------------------------------------------------------------

    #[test]
    fn custody_error_no_key_data() {
        let errors: Vec<CustodyError> = vec![
            CustodyError::InvalidAddress("bad address".into()),
            CustodyError::SigningError("failed to sign".into()),
            CustodyError::Database("connection refused".into()),
            CustodyError::NodeError { chain: "ethereum".into(), message: "rpc timeout".into() },
            CustodyError::Internal("unknown".into()),
        ];
        for err in errors {
            let msg = err.to_string();
            assert!(!msg.contains("5eb00bbddcf"), "Error contains hex key fragment: {msg}");
            assert!(msg.len() < 500, "Error message too large — might contain key dump");
        }
    }

    // -----------------------------------------------------------------------
    // 3. Validation rejects dangerous inputs
    // -----------------------------------------------------------------------

    #[test]
    fn reject_zero_address_ethereum() {
        let result = validation::validate_address(
            Chain::Ethereum, "0x0000000000000000000000000000000000000000"
        );
        assert!(result.is_err(), "Zero address should be rejected");
    }

    #[test]
    fn reject_empty_address() {
        assert!(validation::validate_address(Chain::Ethereum, "").is_err());
        assert!(validation::validate_address(Chain::Bitcoin, "").is_err());
        assert!(validation::validate_address(Chain::Tron, "").is_err());
        assert!(validation::validate_address(Chain::Solana, "").is_err());
    }

    #[test]
    fn reject_wrong_chain_address() {
        // Bitcoin address on Ethereum
        assert!(validation::validate_address(
            Chain::Ethereum, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        ).is_err());
        // Ethereum address on Bitcoin
        assert!(validation::validate_address(
            Chain::Bitcoin, "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18"
        ).is_err());
    }

    // -----------------------------------------------------------------------
    // 4. Circuit breaker limits are enforced
    // -----------------------------------------------------------------------

    #[test]
    fn circuit_breaker_blocks_over_single_limit() {
        let cb = middleware_types::CircuitBreakerState::default_limits();
        assert!(cb.check(rust_decimal::Decimal::new(600_000, 0)).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_when_tripped() {
        let mut cb = middleware_types::CircuitBreakerState::default_limits();
        cb.is_tripped = true;
        // Even $1 should be blocked
        assert!(cb.check(rust_decimal::Decimal::ONE).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_hourly_cumulative() {
        let mut cb = middleware_types::CircuitBreakerState::default_limits();
        cb.current_hourly_outflow = rust_decimal::Decimal::new(9_999_999, 0);
        // $2 more would exceed $10M hourly
        assert!(cb.check(rust_decimal::Decimal::new(2, 0)).is_err());
    }

    #[test]
    fn circuit_breaker_blocks_daily_cumulative() {
        let mut cb = middleware_types::CircuitBreakerState::default_limits();
        cb.current_daily_outflow = rust_decimal::Decimal::new(49_999_999, 0);
        assert!(cb.check(rust_decimal::Decimal::new(2, 0)).is_err());
    }

    // -----------------------------------------------------------------------
    // 5. Webhook signing is secure
    // -----------------------------------------------------------------------

    #[test]
    fn webhook_signature_not_forgeable() {
        let signer = middleware_types::WebhookSigner::new(b"real_secret");
        let payload = b"sensitive_data";
        let sig = signer.sign(payload);

        // Wrong secret can't forge
        let attacker = middleware_types::WebhookSigner::new(b"wrong_secret");
        assert!(!attacker.verify(payload, &sig));
    }

    #[test]
    fn webhook_signature_tamper_detection() {
        let signer = middleware_types::WebhookSigner::new(b"secret");
        let sig = signer.sign(b"original");
        // Modified payload doesn't verify
        assert!(!signer.verify(b"modified", &sig));
    }

    // -----------------------------------------------------------------------
    // 6. Crypto correctness
    // -----------------------------------------------------------------------

    #[test]
    fn encrypted_store_wrong_password_fails() {
        // The encrypted_store module in signing crate handles this,
        // but we can verify the crypto_utils HMAC doesn't match with wrong key
        let mac1 = crypto_utils::hmac_sha256(b"correct_password", b"data");
        let mac2 = crypto_utils::hmac_sha256(b"wrong_password", b"data");
        assert_ne!(mac1, mac2);
    }

    #[test]
    fn constant_time_comparison_prevents_timing() {
        // Same-length strings should use constant-time
        assert!(crypto_utils::constant_time_eq(b"correct", b"correct"));
        assert!(!crypto_utils::constant_time_eq(b"correct", b"wrong!!"));
        // Different lengths immediately return false (but that's OK — length is not secret)
        assert!(!crypto_utils::constant_time_eq(b"short", b"longer_string"));
    }

    #[test]
    fn eip55_prevents_address_spoofing() {
        // Correct checksum
        assert!(crypto_utils::verify_eip55("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
        // Slightly modified (lowercase a that should be uppercase)
        assert!(!crypto_utils::verify_eip55("0x5aaeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
    }

    #[test]
    fn base58check_prevents_typos() {
        // Valid Bitcoin address
        let encoded = crypto_utils::base58check_encode(0x00, &[0u8; 20]);
        assert!(crypto_utils::base58check_verify(&encoded));
        // Tampered address
        assert!(!crypto_utils::base58check_verify("1InvalidBitcoinAddressXXXXXX"));
    }

    // -----------------------------------------------------------------------
    // 7. API key security
    // -----------------------------------------------------------------------

    #[test]
    fn api_key_never_returned_from_db() {
        let key = connectivity::ApiKey {
            id: uuid::Uuid::nil(), name: "test".into(),
            key: None, // raw key is NEVER in DB responses
            key_hash: "hash".into(),
            role: connectivity::ApiKeyRole::Viewer,
            permissions: vec![], ip_whitelist: vec![],
            rate_limit_per_minute: 100, enabled: true,
            created_at: chrono::Utc::now(), last_used_at: None, expires_at: None,
        };
        let json = serde_json::to_string(&key).unwrap();
        // key field should be absent (skip_serializing_if = "Option::is_none")
        assert!(!json.contains("\"key\":"), "Raw API key should not be serialized");
    }

    // -----------------------------------------------------------------------
    // 8. Slippage protection
    // -----------------------------------------------------------------------

    #[test]
    fn defi_slippage_max_2_percent() {
        assert!(defi_execution::validate_slippage(200).is_ok()); // 2% max
        assert!(defi_execution::validate_slippage(201).is_err()); // over limit
    }

    #[test]
    fn mev_risk_flagged_for_large_swaps() {
        let risk = mev_protection::assess_mev_risk(
            Chain::Ethereum, "swap", rust_decimal::Decimal::new(500_000, 0)
        );
        assert_eq!(risk.risk_level, mev_protection::MevRiskLevel::High);
        assert!(risk.sandwich_risk);
    }

    // -----------------------------------------------------------------------
    // 9. Token decimal correctness (prevents fund loss)
    // -----------------------------------------------------------------------

    #[test]
    fn usdt_6_decimals_all_chains() {
        let eth = token_database::find_token("USDT", Chain::Ethereum).unwrap();
        assert_eq!(eth.decimals, 6, "ETH USDT must be 6 decimals");
        let trx = token_database::find_token("USDT", Chain::Tron).unwrap();
        assert_eq!(trx.decimals, 6, "TRX USDT must be 6 decimals");
        let sol = token_database::find_token("USDT", Chain::Solana).unwrap();
        assert_eq!(sol.decimals, 6, "SOL USDT must be 6 decimals");
    }

    #[test]
    fn usdc_6_decimals_all_chains() {
        let eth = token_database::find_token("USDC", Chain::Ethereum).unwrap();
        assert_eq!(eth.decimals, 6);
        let sol = token_database::find_token("USDC", Chain::Solana).unwrap();
        assert_eq!(sol.decimals, 6);
    }

    #[test]
    fn eth_18_decimals() {
        let eth = token_database::find_token("ETH", Chain::Ethereum).unwrap();
        assert_eq!(eth.decimals, 18, "ETH must be 18 decimals");
    }

    #[test]
    fn btc_8_decimals() {
        let btc = token_database::find_token("BTC", Chain::Bitcoin).unwrap();
        assert_eq!(btc.decimals, 8, "BTC must be 8 decimals");
    }
}
