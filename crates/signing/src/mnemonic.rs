//! BIP-39 mnemonic support for seed backup and recovery.
//!
//! The industry standard for custody backup: 24 words that encode
//! 256 bits of entropy. Users write down words → can restore wallet anywhere.
//!
//! Flow:
//! 1. Generate: create 24-word mnemonic → derive 64-byte seed
//! 2. Backup: show 24 words to user (ONCE, in secure environment)
//! 3. Restore: user enters 24 words → derive same seed → same addresses
//!
//! Reference: BIP-39 https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki

use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use zeroize::Zeroize;

/// Generate a new 24-word BIP-39 mnemonic.
///
/// Returns the mnemonic phrase and the derived 64-byte seed.
/// The seed is derived using PBKDF2-HMAC-SHA512 with 2048 iterations
/// (BIP-39 standard, passphrase = empty string).
pub fn generate_mnemonic() -> Result<(String, [u8; 64])> {
    // Generate 32 bytes of entropy → 24 words
    let mut entropy = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut entropy);

    let mnemonic = Mnemonic::from_entropy(&entropy)
        .context("Failed to generate 24-word mnemonic")?;

    let phrase = mnemonic.to_string();
    let seed = mnemonic_to_seed(&phrase, "")?;

    // Zeroize entropy
    entropy.zeroize();

    Ok((phrase, seed))
}

/// Derive a 64-byte seed from a mnemonic phrase.
///
/// Uses BIP-39 standard derivation with optional passphrase.
/// Most custody implementations use empty passphrase.
pub fn mnemonic_to_seed(phrase: &str, passphrase: &str) -> Result<[u8; 64]> {
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .context("Invalid mnemonic phrase — check words and spelling")?;

    let seed_bytes = mnemonic.to_seed(passphrase);
    let mut seed = [0u8; 64];
    seed.copy_from_slice(&seed_bytes);
    Ok(seed)
}

/// Validate a mnemonic phrase without deriving the seed.
///
/// Returns Ok(word_count) if valid, Err with details if not.
pub fn validate_mnemonic(phrase: &str) -> Result<usize> {
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, phrase)
        .context("Invalid mnemonic phrase")?;

    let count = mnemonic.words().count();
    Ok(count)
}

/// Generate mnemonic from existing entropy (raw bytes).
///
/// This is for converting an existing raw seed to a mnemonic.
/// Takes 32 bytes of entropy → produces 24 words.
pub fn entropy_to_mnemonic(entropy: &[u8; 32]) -> Result<String> {
    let mnemonic = Mnemonic::from_entropy(entropy)
        .context("Failed to create mnemonic from entropy")?;
    Ok(mnemonic.to_string())
}

/// Display a mnemonic in numbered format for user backup.
///
/// Returns a formatted string like:
/// ```text
///  1. abandon     2. ability     3. able
///  4. about       5. above       6. absent
/// ...
/// ```
pub fn format_mnemonic_for_display(phrase: &str) -> String {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    let mut output = String::new();

    for (i, word) in words.iter().enumerate() {
        output.push_str(&format!("{:>2}. {:<12}", i + 1, word));
        if (i + 1) % 3 == 0 {
            output.push('\n');
        }
    }

    output
}

/// Securely zeroize a mnemonic string in memory.
pub fn zeroize_mnemonic(phrase: &mut String) {
    phrase.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_24_word_mnemonic() {
        let (phrase, seed) = generate_mnemonic().unwrap();
        let words: Vec<&str> = phrase.split_whitespace().collect();
        assert_eq!(words.len(), 24, "Should generate 24 words");
        assert_eq!(seed.len(), 64, "Seed should be 64 bytes");
        // Seed should not be all zeros
        assert!(seed.iter().any(|&b| b != 0), "Seed should not be zero");
    }

    #[test]
    fn mnemonic_is_deterministic() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let seed1 = mnemonic_to_seed(phrase, "").unwrap();
        let seed2 = mnemonic_to_seed(phrase, "").unwrap();
        assert_eq!(seed1, seed2, "Same mnemonic = same seed");
    }

    #[test]
    fn different_mnemonic_different_seed() {
        let (phrase1, seed1) = generate_mnemonic().unwrap();
        let (phrase2, seed2) = generate_mnemonic().unwrap();
        assert_ne!(phrase1, phrase2, "Two mnemonics should differ");
        assert_ne!(seed1, seed2, "Seeds should differ");
    }

    #[test]
    fn passphrase_changes_seed() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        let seed_no_pass = mnemonic_to_seed(phrase, "").unwrap();
        let seed_with_pass = mnemonic_to_seed(phrase, "my secret passphrase").unwrap();
        assert_ne!(seed_no_pass, seed_with_pass, "Passphrase should change seed");
    }

    #[test]
    fn invalid_mnemonic_rejected() {
        assert!(mnemonic_to_seed("not a valid mnemonic", "").is_err());
        assert!(mnemonic_to_seed("", "").is_err());
        assert!(mnemonic_to_seed("abandon abandon abandon", "").is_err()); // too short
    }

    #[test]
    fn validate_mnemonic_works() {
        let (phrase, _) = generate_mnemonic().unwrap();
        assert_eq!(validate_mnemonic(&phrase).unwrap(), 24);
        assert!(validate_mnemonic("invalid words here").is_err());
    }

    #[test]
    fn format_display() {
        let phrase = "abandon ability able about above absent absorb abstract absurd abuse access accident";
        let formatted = format_mnemonic_for_display(phrase);
        assert!(formatted.contains(" 1. abandon"));
        assert!(formatted.contains(" 2. ability"));
        assert!(formatted.contains("12. accident"));
    }

    #[test]
    fn generate_and_restore_same_addresses() {
        use crate::ethereum::eth_address;

        let (phrase, seed) = generate_mnemonic().unwrap();

        // Derive ETH address from original seed
        let addr1 = eth_address(&seed, 0).unwrap();

        // Restore from mnemonic
        let restored_seed = mnemonic_to_seed(&phrase, "").unwrap();
        let addr2 = eth_address(&restored_seed, 0).unwrap();

        assert_eq!(addr1, addr2, "Restored seed must produce same ETH address");
    }

    #[test]
    fn entropy_to_mnemonic_roundtrip() {
        let entropy = [0x42u8; 32];
        let phrase = entropy_to_mnemonic(&entropy).unwrap();
        let words: Vec<&str> = phrase.split_whitespace().collect();
        assert_eq!(words.len(), 24);

        // Same entropy = same mnemonic
        let phrase2 = entropy_to_mnemonic(&entropy).unwrap();
        assert_eq!(phrase, phrase2);
    }
}
