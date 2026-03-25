//! Seed generation and management tool.
//!
//! Usage:
//!   custody-seed generate --output seed.enc --password <password>
//!   custody-seed verify --input seed.enc --password <password>
//!   custody-seed show-addresses --input seed.enc --password <password>

use anyhow::{Context, Result};
use custody_signing::{encrypt_seed, decrypt_seed, Password};

/// Generate a new random seed, encrypt it, and save to file.
pub fn generate_seed(output_path: &str, password: &str) -> Result<()> {
    // Generate 64 bytes of cryptographically secure random data
    let mut seed = [0u8; 64];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut seed);

    // Encrypt with AES-256-GCM
    let pw = Password::from_str(password);
    let encrypted = encrypt_seed(&seed, &pw)
        .context("Seed encryption failed")?;

    // Save to file
    std::fs::write(output_path, &encrypted)
        .with_context(|| format!("Cannot write to {output_path}"))?;

    // Show derived addresses for verification
    println!("Seed generated and encrypted to: {output_path}");
    println!("Encrypted size: {} bytes", encrypted.len());
    println!("");
    show_addresses_from_seed(&seed)?;

    // Zeroize seed from memory
    use zeroize::Zeroize;
    seed.zeroize();

    println!("");
    println!("IMPORTANT: Store this file securely. The password is the ONLY way to decrypt it.");
    println!("Consider creating a Shamir backup:");
    println!("  custody-seed backup --input {} --shares 5 --threshold 3", output_path);

    Ok(())
}

/// Verify that a seed file can be decrypted with the given password.
pub fn verify_seed(input_path: &str, password: &str) -> Result<()> {
    let encrypted = std::fs::read(input_path)
        .with_context(|| format!("Cannot read {input_path}"))?;

    let pw = Password::from_str(password);
    let seed = decrypt_seed(&encrypted, &pw)
        .context("Decryption failed — wrong password or corrupted file")?;

    println!("Seed file verified OK: {input_path}");
    println!("");
    show_addresses_from_seed(&seed)?;

    Ok(())
}

/// Show deposit addresses derived from a seed.
fn show_addresses_from_seed(seed: &[u8; 64]) -> Result<()> {
    println!("Derived addresses (index 0):");

    // Ethereum
    let eth_addr = custody_signing::ethereum::eth_address(seed, 0)?;
    println!("  ETH: {:#x}", eth_addr);

    // Bitcoin
    use custody_address_factory::hd::{SeedBytes, derive_secp256k1_pubkey, derivation_path};
    use custody_address_factory::bitcoin::{encode_p2wpkh, BitcoinNetwork};
    let seed_bytes = SeedBytes(*seed);
    let btc_path = derivation_path(custody_common::Chain::Bitcoin, 0)?;
    let btc_pk = derive_secp256k1_pubkey(&seed_bytes, &btc_path)?;
    let btc_addr = encode_p2wpkh(&btc_pk.compressed, BitcoinNetwork::Mainnet)?;
    println!("  BTC: {}", btc_addr);

    // Solana
    let sol_addr = custody_signing::solana::sol_address(seed, 0)?;
    println!("  SOL: {}", bs58::encode(&sol_addr).into_string());

    // Tron
    let tron_addr = custody_signing::tron::tron_address_hex(seed, 0)?;
    println!("  TRX: {}", tron_addr);

    Ok(())
}

/// Create Shamir backup of an encrypted seed file.
pub fn create_backup(input_path: &str, password: &str, shares: u8, threshold: u8) -> Result<()> {
    let encrypted = std::fs::read(input_path)?;
    let pw = Password::from_str(password);
    let seed = decrypt_seed(&encrypted, &pw)?;

    let share_data = custody_signing::split_3_of_5(&seed)?;

    for (i, share) in share_data.iter().enumerate() {
        let share_path = format!("{}.share{}", input_path, i + 1);
        let share_json = serde_json::json!({
            "index": share.index,
            "data": hex::encode(&share.data),
            "threshold": threshold,
            "total_shares": shares,
        });
        std::fs::write(&share_path, serde_json::to_string_pretty(&share_json)?)?;
        println!("Share {} written to: {}", i + 1, share_path);
    }

    println!("\nBackup complete: {}-of-{} Shamir shares", threshold, shares);
    println!("Store each share in a DIFFERENT location.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_verify_seed() {
        let tmp = format!("/tmp/test_seed_{}.enc", uuid::Uuid::new_v4());
        generate_seed(&tmp, "test-password-123").unwrap();

        // File should exist
        assert!(std::path::Path::new(&tmp).exists());

        // Verify with correct password
        verify_seed(&tmp, "test-password-123").unwrap();

        // Wrong password should fail
        assert!(verify_seed(&tmp, "wrong-password").is_err());

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn seed_addresses_are_deterministic() {
        let seed = [42u8; 64];
        let eth1 = custody_signing::ethereum::eth_address(&seed, 0).unwrap();
        let eth2 = custody_signing::ethereum::eth_address(&seed, 0).unwrap();
        assert_eq!(eth1, eth2);
    }
}
