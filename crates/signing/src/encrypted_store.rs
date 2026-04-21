//! AES-256-GCM encrypted seed storage.
//!
//! Encrypts the 64-byte HD wallet seed at rest. The encryption key is derived
//! from a password using PBKDF2-HMAC-SHA256 (600,000 iterations per OWASP 2023).
//!
//! Wire format: `version (1) || salt (32) || nonce (12) || ciphertext+tag (80)`
//! Total: 125 bytes.
//!
//! For production, the password should come from HashiCorp Vault or an HSM.
//! This module provides the cryptographic layer only — storage I/O is the caller's concern.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, ensure};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Current wire format version.
const FORMAT_VERSION: u8 = 1;

/// PBKDF2 iteration count (OWASP 2023 recommendation for HMAC-SHA256).
const PBKDF2_ITERATIONS: u32 = 600_000;

/// Salt length in bytes.
const SALT_LEN: usize = 32;

/// AES-GCM nonce length in bytes.
const NONCE_LEN: usize = 12;

/// Seed length in bytes (BIP-39 output).
const SEED_LEN: usize = 64;

/// AES-GCM tag length in bytes.
const TAG_LEN: usize = 16;

/// Total encrypted payload length: version + salt + nonce + ciphertext + tag.
const ENCRYPTED_LEN: usize = 1 + SALT_LEN + NONCE_LEN + SEED_LEN + TAG_LEN;

/// Encryption password wrapper with zeroize.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Password(pub Vec<u8>);

impl Password {
    pub fn from_str(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password(***)")
    }
}

/// Derived encryption key with zeroize.
#[derive(Zeroize, ZeroizeOnDrop)]
struct DerivedKey([u8; 32]);

/// Derive a 256-bit key from password + salt using PBKDF2-HMAC-SHA256.
fn derive_key(password: &Password, salt: &[u8]) -> Result<DerivedKey> {
    let mut key = DerivedKey([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&password.0, salt, PBKDF2_ITERATIONS, &mut key.0);
    Ok(key)
}

/// Encrypt a 64-byte seed with AES-256-GCM.
///
/// Returns the encrypted payload (125 bytes) containing version, salt, nonce,
/// and ciphertext+tag. A fresh random salt and nonce are generated each time.
pub fn encrypt_seed(seed: &[u8; SEED_LEN], password: &Password) -> Result<Vec<u8>> {
    // OS CSPRNG, not `thread_rng()`. AES-GCM nonce reuse under
    // the same key leaks plaintext XOR — any path that could
    // repeat a 12-byte nonce is catastrophic, so we pull directly
    // from `OsRng` and sidestep the thread-local reseed cache
    // entirely. Same rationale for salt: a grep-auditor looking
    // for key-material entropy sources should find `OsRng` here
    // immediately rather than have to reason about whether
    // `thread_rng()` is currently wired to OS entropy in the
    // version of `rand` we've pinned.
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    let mut os_rng = rand::rngs::OsRng;
    os_rng.fill_bytes(&mut salt);
    os_rng.fill_bytes(&mut nonce_bytes);

    let mut key = derive_key(password, &salt)?;

    let cipher = Aes256Gcm::new_from_slice(&key.0)
        .context("AES-256-GCM key initialization failed")?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, seed.as_ref())
        .map_err(|e| anyhow::anyhow!("AES-256-GCM encryption failed: {e}"))?;

    key.zeroize();

    // Wire format: version || salt || nonce || ciphertext+tag
    let mut output = Vec::with_capacity(ENCRYPTED_LEN);
    output.push(FORMAT_VERSION);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    debug_assert_eq!(output.len(), ENCRYPTED_LEN);

    tracing::info!("Seed encrypted (AES-256-GCM, PBKDF2 {}i)", PBKDF2_ITERATIONS);
    Ok(output)
}

/// Decrypt an encrypted seed payload back to the 64-byte seed.
///
/// Validates the format version and returns an error on wrong password
/// (authentication tag mismatch).
pub fn decrypt_seed(encrypted: &[u8], password: &Password) -> Result<[u8; SEED_LEN]> {
    ensure!(
        encrypted.len() == ENCRYPTED_LEN,
        "Invalid encrypted seed length: expected {ENCRYPTED_LEN}, got {}",
        encrypted.len()
    );
    ensure!(
        encrypted[0] == FORMAT_VERSION,
        "Unsupported encrypted seed format version: {}",
        encrypted[0]
    );

    let salt = &encrypted[1..1 + SALT_LEN];
    let nonce_bytes = &encrypted[1 + SALT_LEN..1 + SALT_LEN + NONCE_LEN];
    let ciphertext = &encrypted[1 + SALT_LEN + NONCE_LEN..];

    let mut key = derive_key(password, salt)?;

    let cipher = Aes256Gcm::new_from_slice(&key.0)
        .context("AES-256-GCM key initialization failed")?;

    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed: wrong password or corrupted data"))?;

    key.zeroize();

    ensure!(
        plaintext.len() == SEED_LEN,
        "Decrypted seed has wrong length: expected {SEED_LEN}, got {}",
        plaintext.len()
    );

    let mut seed = [0u8; SEED_LEN];
    seed.copy_from_slice(&plaintext);
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 64] {
        let mut seed = [0u8; 64];
        for (i, b) in seed.iter_mut().enumerate() {
            *b = i as u8;
        }
        seed
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let seed = test_seed();
        let password = Password::from_str("test-password-2024");

        let encrypted = encrypt_seed(&seed, &password).unwrap();
        assert_eq!(encrypted.len(), ENCRYPTED_LEN);
        assert_eq!(encrypted[0], FORMAT_VERSION);

        let decrypted = decrypt_seed(&encrypted, &password).unwrap();
        assert_eq!(decrypted, seed);
    }

    #[test]
    fn wrong_password_fails() {
        let seed = test_seed();
        let password = Password::from_str("correct-password");
        let wrong = Password::from_str("wrong-password");

        let encrypted = encrypt_seed(&seed, &password).unwrap();
        let result = decrypt_seed(&encrypted, &wrong);

        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("wrong password"),
            "Should indicate wrong password"
        );
    }

    #[test]
    fn corrupted_data_fails() {
        let seed = test_seed();
        let password = Password::from_str("test-password");

        let mut encrypted = encrypt_seed(&seed, &password).unwrap();
        // Flip a byte in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        assert!(decrypt_seed(&encrypted, &password).is_err());
    }

    #[test]
    fn wrong_length_rejected() {
        let password = Password::from_str("test");
        assert!(decrypt_seed(&[0u8; 10], &password).is_err());
    }

    #[test]
    fn wrong_version_rejected() {
        let seed = test_seed();
        let password = Password::from_str("test");
        let mut encrypted = encrypt_seed(&seed, &password).unwrap();
        encrypted[0] = 99; // bogus version
        assert!(decrypt_seed(&encrypted, &password).is_err());
    }

    #[test]
    fn each_encryption_produces_different_output() {
        let seed = test_seed();
        let password = Password::from_str("test");

        let enc1 = encrypt_seed(&seed, &password).unwrap();
        let enc2 = encrypt_seed(&seed, &password).unwrap();

        // Different salt + nonce → different ciphertext
        assert_ne!(enc1, enc2);

        // Both decrypt to the same seed
        assert_eq!(decrypt_seed(&enc1, &password).unwrap(), seed);
        assert_eq!(decrypt_seed(&enc2, &password).unwrap(), seed);
    }

    #[test]
    fn password_debug_is_redacted() {
        let pw = Password::from_str("secret");
        assert_eq!(format!("{:?}", pw), "Password(***)");
    }
}
