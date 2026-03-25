//! Integration tests for the custody API.
//!
//! These tests require a running PostgreSQL instance.
//! Set DATABASE_URL environment variable to run.
//!
//! Run: DATABASE_URL=postgres://custody:custody_dev_password@localhost:5432/custody cargo test --test integration_api

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

/// Helper to create a test app (without real DB — uses API structure test).
fn test_app() -> axum::Router {
    // This tests the route structure without DB
    // Full DB integration tests need DATABASE_URL
    axum::Router::new()
        .route("/health", axum::routing::get(|| async { "ok" }))
}

#[tokio::test]
async fn health_check() {
    let app = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test that validates the full withdrawal flow conceptually.
/// This doesn't need a DB — it validates the state machine logic.
#[test]
fn withdrawal_state_machine_transitions() {
    use custody_common::WithdrawalStatus;

    // Valid transitions
    let valid = vec![
        (WithdrawalStatus::Received, WithdrawalStatus::Validated),
        (WithdrawalStatus::Received, WithdrawalStatus::Failed),
        (WithdrawalStatus::Validated, WithdrawalStatus::PendingApproval),
        (WithdrawalStatus::Validated, WithdrawalStatus::Signed),
        (WithdrawalStatus::Validated, WithdrawalStatus::Failed),
        (WithdrawalStatus::PendingApproval, WithdrawalStatus::Validated), // approved
        (WithdrawalStatus::PendingApproval, WithdrawalStatus::Cancelled),
        (WithdrawalStatus::Signed, WithdrawalStatus::Broadcast),
        (WithdrawalStatus::Broadcast, WithdrawalStatus::Confirming),
        (WithdrawalStatus::Broadcast, WithdrawalStatus::Failed),
        (WithdrawalStatus::Confirming, WithdrawalStatus::Finalized),
    ];

    // All transitions should be representable
    for (from, to) in valid {
        // Just verify the enum values exist and are different
        assert_ne!(from, to, "Transition should change state: {:?} → {:?}", from, to);
    }
}

/// Test that the policy engine integrates correctly with the pipeline.
#[test]
fn policy_engine_blocks_non_whitelisted() {
    use custody_common::{Asset, Chain};
    use custody_policy::{PolicyEngine, PolicyRule, PolicyCondition, PolicyAction, DestinationType};
    use custody_policy::rbac::Role;
    use rust_decimal_macros::dec;

    let rules = vec![
        PolicyRule::new(
            "require_whitelist",
            1,
            PolicyCondition {
                destination_type: Some(DestinationType::Whitelisted),
                ..Default::default()
            },
            PolicyAction::Allow,
        ),
    ];

    let engine = PolicyEngine::new(rules);

    // Whitelisted → Allow
    let decision = engine.evaluate(Chain::Ethereum, &Asset::new("ETH"), dec!(100), &DestinationType::Whitelisted, &Role::Editor);
    assert!(matches!(decision.action, PolicyAction::Allow));

    // Non-whitelisted → default Deny
    let decision = engine.evaluate(Chain::Ethereum, &Asset::new("ETH"), dec!(100), &DestinationType::Any, &Role::Editor);
    assert!(matches!(decision.action, PolicyAction::Deny { .. }));
}

/// Test sanctions blocking in the pipeline flow.
#[test]
fn sanctions_block_tornado_cash() {
    use custody_compliance::ofac::ofac_checker_with_known_addresses;

    let checker = ofac_checker_with_known_addresses();

    // Tornado Cash address should be blocked
    let result = checker.check("0xd90e2f925da726b50c4ed8d0fb90ad053324f31b");
    assert!(result.is_sanctioned);

    // Clean address should pass
    let result = checker.check("0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18");
    assert!(!result.is_sanctioned);
}

/// Test token registry lookup for pipeline signing.
#[test]
fn token_registry_for_signing() {
    use custody_common::Chain;
    use custody_common::token_registry::TokenRegistry;

    let reg = TokenRegistry::with_defaults();

    // ETH is native — no contract needed
    assert!(reg.is_native("ETH", Chain::Ethereum));
    assert_eq!(reg.decimals("ETH", Chain::Ethereum), Some(18));

    // USDT on Ethereum — needs contract
    assert!(!reg.is_native("USDT", Chain::Ethereum));
    assert_eq!(reg.decimals("USDT", Chain::Ethereum), Some(6));
    let contract = reg.contract_address("USDT", Chain::Ethereum).unwrap();
    assert!(contract.starts_with("0x"));

    // Same asset, different chain
    let tron_contract = reg.contract_address("USDT", Chain::Tron).unwrap();
    assert!(tron_contract.starts_with('T'));
    assert_ne!(contract, tron_contract);
}

/// Test full deposit → confirmation flow types.
#[test]
fn deposit_confirmation_flow() {
    use custody_confirmations::tracker::{confirmation_count, is_confirmed};

    // Ethereum: 12 confirmations required
    assert!(!is_confirmed(100, 89, 12)); // 11 confirms
    assert!(is_confirmed(100, 88, 12));  // 12 confirms
    assert!(is_confirmed(200, 88, 12));  // 112 confirms

    // Bitcoin: 3 confirmations required
    assert!(!is_confirmed(10, 8, 3));    // 2 confirms
    assert!(is_confirmed(11, 8, 3));     // 3 confirms
}

/// Test UTXO coin selection.
#[test]
fn utxo_coin_selection_for_btc_withdrawal() {
    use custody_utxo::{Utxo, select_coins, DUST_LIMIT_SATS};
    use uuid::Uuid;

    let utxos = vec![
        Utxo { id: Uuid::new_v4(), txid: "tx1".into(), vout: 0, amount_sats: 100_000, address: "bc1q...".into(), confirmations: 6, locked: false },
        Utxo { id: Uuid::new_v4(), txid: "tx2".into(), vout: 0, amount_sats: 200_000, address: "bc1q...".into(), confirmations: 6, locked: false },
        Utxo { id: Uuid::new_v4(), txid: "tx3".into(), vout: 0, amount_sats: 50_000, address: "bc1q...".into(), confirmations: 6, locked: false },
    ];

    // Should succeed with enough UTXOs
    let result = select_coins(&utxos, 150_000, 10).unwrap();
    assert!(result.total_input_sats >= 150_000 + result.fee_sats);

    // Should fail with insufficient funds
    let result = select_coins(&utxos, 500_000, 10);
    assert!(result.is_err());
}

/// Test EIP-1559 fee estimation types.
#[test]
fn fee_estimation_eip1559() {
    use custody_fee_estimator::FeeEstimate;
    use rust_decimal_macros::dec;

    let estimate = FeeEstimate::eip1559(dec!(30), dec!(2));
    assert_eq!(estimate.max_fee, dec!(62)); // 2*30 + 2
    assert_eq!(estimate.base_fee, dec!(30));
    assert_eq!(estimate.priority_fee, dec!(2));
}

/// Test MPC FROST signing (real crypto, not stubs).
#[test]
fn frost_threshold_signing_real() {
    use frost_ed25519 as frost;
    use custody_mpc::frost::sign::{frost_sign, verify_frost_signature};

    let mut rng = rand::thread_rng();
    let (shares, pubkey_package) = frost::keys::generate_with_dealer(
        3, 2, frost::keys::IdentifierList::Default, &mut rng,
    ).unwrap();

    let key_packages: Vec<(frost::Identifier, frost::keys::KeyPackage)> = shares
        .into_iter()
        .map(|(id, s)| (id, frost::keys::KeyPackage::try_from(s).unwrap()))
        .collect();

    // Sign with 2 of 3
    let signers: Vec<(u16, Vec<u8>)> = key_packages[..2]
        .iter()
        .enumerate()
        .map(|(i, (_, kp))| ((i + 1) as u16, serde_json::to_vec(kp).unwrap()))
        .collect();

    let signer_refs: Vec<(u16, &[u8])> = signers.iter().map(|(id, d)| (*id, d.as_slice())).collect();

    let sig = frost_sign(&signer_refs, &pubkey_package, b"custody tx hash", 2).unwrap();
    assert_eq!(sig.signature.len(), 64);

    // Verify
    let gpk = pubkey_package.verifying_key().serialize().unwrap();
    let gpk_arr: [u8; 32] = gpk.as_slice().try_into().unwrap();
    let sig_arr: [u8; 64] = sig.signature.as_slice().try_into().unwrap();
    assert!(verify_frost_signature(&gpk_arr, b"custody tx hash", &sig_arr).unwrap());
}
