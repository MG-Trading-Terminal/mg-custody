//! Disaster recovery tests — prove that backup/restore works.
//!
//! Tests:
//! 1. Shamir 3-of-5 split → drop 2 shares → reconstruct → verify addresses match
//! 2. Encrypted seed → decrypt → derive same addresses
//! 3. Key rotation: new seed → new addresses → different from old

use custody_address_factory::allocator::{derive_address, AddressFactoryConfig};
use custody_address_factory::hd::SeedBytes;
use custody_common::Chain;
use custody_signing::shamir::{split_3_of_5, reconstruct_3_of_5, Share};
use custody_signing::{encrypt_seed, decrypt_seed, Password};

fn test_seed() -> [u8; 64] {
    let bytes = hex::decode(
        "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4"
    ).unwrap();
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&bytes);
    seed
}

// ── Shamir Backup & Restore ──────────────────────────────────────

#[test]
fn shamir_3of5_full_roundtrip() {
    let seed = test_seed();

    // Step 1: Split into 5 shares with threshold 3
    let shares = split_3_of_5(&seed).unwrap();
    assert_eq!(shares.len(), 5);

    // Step 2: Derive addresses from original seed
    let seed_bytes = SeedBytes(seed);
    let config = AddressFactoryConfig::default();
    let (orig_eth, _) = derive_address(&seed_bytes, Chain::Ethereum, 0, &config).unwrap();
    let (orig_btc, _) = derive_address(&seed_bytes, Chain::Bitcoin, 0, &config).unwrap();
    let (orig_sol, _) = derive_address(&seed_bytes, Chain::Solana, 0, &config).unwrap();

    // Step 3: Reconstruct from shares [0, 2, 4] (drop shares 1 and 3)
    let partial: Vec<Share> = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
    let recovered = reconstruct_3_of_5(&partial).unwrap();
    assert_eq!(recovered.len(), 64, "Recovered seed should be 64 bytes");

    let mut recovered_arr = [0u8; 64];
    recovered_arr.copy_from_slice(&recovered);

    // Step 4: Derive addresses from recovered seed — must match original
    let recovered_bytes = SeedBytes(recovered_arr);
    let (rec_eth, _) = derive_address(&recovered_bytes, Chain::Ethereum, 0, &config).unwrap();
    let (rec_btc, _) = derive_address(&recovered_bytes, Chain::Bitcoin, 0, &config).unwrap();
    let (rec_sol, _) = derive_address(&recovered_bytes, Chain::Solana, 0, &config).unwrap();

    assert_eq!(orig_eth, rec_eth, "ETH address must match after Shamir recovery");
    assert_eq!(orig_btc, rec_btc, "BTC address must match after Shamir recovery");
    assert_eq!(orig_sol, rec_sol, "SOL address must match after Shamir recovery");
}

#[test]
fn shamir_any_3_of_5_combination_works() {
    let seed = test_seed();
    let shares = split_3_of_5(&seed).unwrap();

    // All 10 possible 3-of-5 combinations
    let combos: Vec<[usize; 3]> = vec![
        [0,1,2], [0,1,3], [0,1,4], [0,2,3], [0,2,4],
        [0,3,4], [1,2,3], [1,2,4], [1,3,4], [2,3,4],
    ];

    for combo in combos {
        let subset: Vec<Share> = combo.iter().map(|&i| shares[i].clone()).collect();
        let recovered = reconstruct_3_of_5(&subset).unwrap();
        assert_eq!(recovered, seed.to_vec(),
            "Reconstruction from shares {:?} should match original seed", combo);
    }
}

#[test]
fn shamir_2_shares_insufficient() {
    let seed = test_seed();
    let shares = split_3_of_5(&seed).unwrap();

    // Only 2 shares — should NOT recover the original
    let partial: Vec<Share> = vec![shares[0].clone(), shares[1].clone()];
    // reconstruct_3_of_5 requires exactly 3 shares — but let's test the math
    // With only 2 shares and threshold 3, the interpolation produces wrong result
    let result = custody_signing::shamir::reconstruct(&partial);
    if let Ok(recovered) = result {
        // Recovered bytes exist but should NOT match original
        assert_ne!(recovered, seed.to_vec(),
            "2 shares must NOT recover the original seed (threshold=3)");
    }
    // If it errors, that's also acceptable
}

// ── Encrypted Seed Backup & Restore ──────────────────────────────

#[test]
fn encrypted_seed_roundtrip() {
    let seed = test_seed();
    let password = Password::from_str("my_strong_backup_password_2024!");

    // Encrypt
    let encrypted = encrypt_seed(&seed, &password).unwrap();
    assert!(encrypted.len() > 64, "Encrypted should be larger than raw seed");
    assert_ne!(&encrypted[..64], &seed[..], "Encrypted should not start with raw seed");

    // Decrypt
    let decrypted = decrypt_seed(&encrypted, &password).unwrap();
    assert_eq!(decrypted, seed, "Decrypted seed must match original");

    // Wrong password fails
    let wrong_pw = Password::from_str("wrong_password");
    assert!(decrypt_seed(&encrypted, &wrong_pw).is_err(), "Wrong password must fail");
}

#[test]
fn encrypted_seed_derives_same_addresses() {
    let seed = test_seed();
    let config = AddressFactoryConfig::default();

    // Derive original addresses
    let seed_bytes = SeedBytes(seed);
    let (orig_eth, _) = derive_address(&seed_bytes, Chain::Ethereum, 0, &config).unwrap();

    // Encrypt → decrypt → derive
    let password = Password::from_str("backup_test");
    let encrypted = encrypt_seed(&seed, &password).unwrap();
    let decrypted = decrypt_seed(&encrypted, &password).unwrap();

    let dec_bytes = SeedBytes(decrypted);
    let (dec_eth, _) = derive_address(&dec_bytes, Chain::Ethereum, 0, &config).unwrap();

    assert_eq!(orig_eth, dec_eth, "Address from encrypted backup must match original");
}

// ── Key Rotation ─────────────────────────────────────────────────

#[test]
fn new_seed_produces_different_addresses() {
    let config = AddressFactoryConfig::default();

    let seed1 = SeedBytes(test_seed());
    let (addr1_eth, _) = derive_address(&seed1, Chain::Ethereum, 0, &config).unwrap();
    let (addr1_btc, _) = derive_address(&seed1, Chain::Bitcoin, 0, &config).unwrap();

    // Generate a different seed (simulate rotation)
    let mut seed2_arr = [0u8; 64];
    seed2_arr[0] = 0xFF; // Just change one byte — completely different addresses
    let seed2 = SeedBytes(seed2_arr);
    let (addr2_eth, _) = derive_address(&seed2, Chain::Ethereum, 0, &config).unwrap();
    let (addr2_btc, _) = derive_address(&seed2, Chain::Bitcoin, 0, &config).unwrap();

    assert_ne!(addr1_eth, addr2_eth, "Rotated seed must produce different ETH address");
    assert_ne!(addr1_btc, addr2_btc, "Rotated seed must produce different BTC address");
}
