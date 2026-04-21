//! TOTP 2FA persistence.
//!
//! Stores TOTP secrets encrypted with AES-256-GCM. The caller supplies
//! the 32-byte encryption key (derived from the service's master secret).
//!
//! Secret storage format: `{hex(nonce)}:{hex(ciphertext)}`
//! where nonce is 12 random bytes (96-bit GCM nonce).
//!
//! Backup codes are stored as SHA-256(code) and are single-use.
//! `backup_codes_used` is a parallel boolean array: index i is true when
//! backup_codes[i] has been consumed.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng as AesOsRng},
    Aes256Gcm, Key, Nonce,
};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::CustodyDb;

/// Pending-2FA token expiry.
const PENDING_TOKEN_EXPIRY_MINUTES: i64 = 5;
/// Number of backup codes to generate.
pub const BACKUP_CODE_COUNT: usize = 8;
/// Length of each backup code (characters).
pub const BACKUP_CODE_LENGTH: usize = 8;

// ---------------------------------------------------------------------------
// Encryption helpers (AES-256-GCM)
// ---------------------------------------------------------------------------

/// Encrypt a plaintext string with AES-256-GCM.
///
/// Returns `"{hex(nonce)}:{hex(ciphertext)}"`.
pub fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    AesOsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("AES-GCM encrypt failed: {e}"))?;
    Ok(format!("{}:{}", hex::encode(nonce_bytes), hex::encode(ciphertext)))
}

/// Decrypt a ciphertext produced by `encrypt_secret`.
pub fn decrypt_secret(key: &[u8; 32], stored: &str) -> anyhow::Result<String> {
    let (nonce_hex, ct_hex) = stored
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid encrypted secret format"))?;
    let nonce_bytes = hex::decode(nonce_hex)
        .map_err(|_| anyhow::anyhow!("Invalid nonce hex"))?;
    let ciphertext = hex::decode(ct_hex)
        .map_err(|_| anyhow::anyhow!("Invalid ciphertext hex"))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_slice())
        .map_err(|e| anyhow::anyhow!("AES-GCM decrypt failed: {e}"))?;
    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("Decrypted bytes not UTF-8: {e}"))
}

// ---------------------------------------------------------------------------
// Backup codes
// ---------------------------------------------------------------------------

/// Generate BACKUP_CODE_COUNT random alphanumeric backup codes.
///
/// Returns `(raw_codes, hashed_codes)`. Store the hashes; show raw_codes once.
pub fn generate_backup_codes() -> (Vec<String>, Vec<String>) {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/I/1 ambiguous chars
    // OS CSPRNG — backup codes are a 2FA fallback credential;
    // predictability = account takeover. Same reasoning as the
    // session-token path.
    let mut rng = rand::rngs::OsRng;
    let raw: Vec<String> = (0..BACKUP_CODE_COUNT)
        .map(|_| {
            let mut code = String::with_capacity(BACKUP_CODE_LENGTH);
            for _ in 0..BACKUP_CODE_LENGTH {
                let idx = (rng.next_u32() as usize) % CHARSET.len();
                code.push(CHARSET[idx] as char);
            }
            code
        })
        .collect();
    let hashed: Vec<String> = raw
        .iter()
        .map(|c| hex::encode(Sha256::digest(c.as_bytes())))
        .collect();
    (raw, hashed)
}

/// Hash a backup code for DB lookup.
pub fn hash_backup_code(code: &str) -> String {
    hex::encode(Sha256::digest(code.to_uppercase().as_bytes()))
}

// ---------------------------------------------------------------------------
// Pending 2FA token helpers
// ---------------------------------------------------------------------------

/// Generate a pending-2FA token (raw hex, 32 bytes).
///
/// Drawn from `OsRng`. A pending-2FA token bypasses the
/// password — guessing one skips straight to 2FA challenge,
/// which is at worst a 6-digit brute force. Do not give an
/// attacker any RNG-side headstart.
pub fn generate_pending_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Hash a pending-2FA token for DB storage.
pub fn hash_pending_token(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

// ---------------------------------------------------------------------------
// DB operations
// ---------------------------------------------------------------------------

/// A stored TOTP record (secret still encrypted).
#[derive(Debug, Clone)]
pub struct StoredTotp {
    pub user_id: Uuid,
    pub secret_encrypted: String,
    pub enabled: bool,
    pub backup_codes: Vec<String>,      // SHA-256 hashes
    pub backup_codes_used: Vec<bool>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    /// Last accepted TOTP counter for this user. Used to reject
    /// replay of the same code within its 90-second validity
    /// window. None means no TOTP code has ever succeeded yet.
    pub last_used_counter: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct TotpRow {
    user_id: Uuid,
    secret_encrypted: String,
    enabled: bool,
    backup_codes: Vec<String>,
    backup_codes_used: Vec<bool>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    last_used_counter: Option<i64>,
}

impl From<TotpRow> for StoredTotp {
    fn from(r: TotpRow) -> Self {
        StoredTotp {
            user_id: r.user_id,
            secret_encrypted: r.secret_encrypted,
            enabled: r.enabled,
            backup_codes: r.backup_codes,
            backup_codes_used: r.backup_codes_used,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            last_used_counter: r.last_used_counter,
        }
    }
}

impl CustodyDb {
    /// Upsert a TOTP record (insert or replace on user_id conflict).
    ///
    /// Called during setup: inserts with enabled=false, backup codes populated.
    pub async fn upsert_totp_setup(
        &self,
        user_id: Uuid,
        secret_encrypted: &str,
        backup_code_hashes: &[String],
    ) -> anyhow::Result<()> {
        let used_flags: Vec<bool> = vec![false; backup_code_hashes.len()];
        sqlx::query(
            "INSERT INTO user_totp (user_id, secret_encrypted, enabled, backup_codes, backup_codes_used)
             VALUES ($1, $2, false, $3, $4)
             ON CONFLICT (user_id) DO UPDATE
             SET secret_encrypted = EXCLUDED.secret_encrypted,
                 enabled = false,
                 backup_codes = EXCLUDED.backup_codes,
                 backup_codes_used = EXCLUDED.backup_codes_used",
        )
        .bind(user_id)
        .bind(secret_encrypted)
        .bind(backup_code_hashes)
        .bind(&used_flags)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Enable TOTP for a user (called after successful first verify).
    pub async fn enable_totp(&self, user_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE user_totp SET enabled = true WHERE user_id = $1 AND enabled = false",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Disable and delete TOTP for a user.
    pub async fn delete_totp(&self, user_id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM user_totp WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Load TOTP record for a user (None if not set up).
    pub async fn load_totp(&self, user_id: Uuid) -> anyhow::Result<Option<StoredTotp>> {
        let row: Option<TotpRow> = sqlx::query_as(
            "SELECT user_id, secret_encrypted, enabled, backup_codes, backup_codes_used,
                    created_at, last_used_at, last_used_counter
             FROM user_totp WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(StoredTotp::from))
    }

    /// Record the accepted TOTP counter atomically.
    ///
    /// Returns `Ok(true)` when the update wrote a strictly greater
    /// counter — success, first-use of this code. Returns
    /// `Ok(false)` when the existing `last_used_counter` is
    /// already ≥ the submitted value; that is a replay and the
    /// caller MUST reject the login. The SQL uses `WHERE
    /// (existing IS NULL OR existing < $2)` so two concurrent
    /// verify-2fa calls with the same code can't both succeed:
    /// exactly one will see rows_affected = 1, the loser sees 0.
    pub async fn record_totp_counter(
        &self,
        user_id: Uuid,
        counter: i64,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE user_totp
             SET last_used_counter = $2, last_used_at = NOW()
             WHERE user_id = $1
               AND (last_used_counter IS NULL OR last_used_counter < $2)",
        )
        .bind(user_id)
        .bind(counter)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Consume a backup code: mark it used and return true on success, false if not found/already used.
    ///
    /// Iterates the stored hashes and marks the matching index as used atomically.
    pub async fn consume_backup_code(
        &self,
        user_id: Uuid,
        code_hash: &str,
    ) -> anyhow::Result<bool> {
        // Load current record
        let record = match self.load_totp(user_id).await? {
            Some(r) if r.enabled => r,
            _ => return Ok(false),
        };

        // Find an unused code that matches
        let idx = record
            .backup_codes
            .iter()
            .zip(record.backup_codes_used.iter())
            .position(|(hash, &used)| !used && hash == code_hash);

        let Some(idx) = idx else { return Ok(false) };

        // Mark it used via array index update
        let result = sqlx::query(
            "UPDATE user_totp
             SET backup_codes_used[$2] = true, last_used_at = NOW()
             WHERE user_id = $1 AND backup_codes_used[$2] = false",
        )
        .bind(user_id)
        .bind((idx + 1) as i32) // PostgreSQL arrays are 1-indexed
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Update last_used_at for TOTP (called on successful code verification).
    pub async fn touch_totp_used(&self, user_id: Uuid) -> anyhow::Result<()> {
        sqlx::query("UPDATE user_totp SET last_used_at = NOW() WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Pending 2FA tokens
    // ---------------------------------------------------------------------------

    /// Issue a pending-2FA token after successful password verification.
    ///
    /// Returns the raw token (only time it is visible — never stored plaintext).
    pub async fn create_pending_2fa_token(&self, user_id: Uuid) -> anyhow::Result<String> {
        let raw = generate_pending_token();
        let hash = hash_pending_token(&raw);
        let now = Utc::now();
        let expires_at = now + Duration::minutes(PENDING_TOKEN_EXPIRY_MINUTES);

        sqlx::query(
            "INSERT INTO pending_2fa_tokens (token_hash, user_id, created_at, expires_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&hash)
        .bind(user_id)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(raw)
    }

    /// Consume a pending-2FA token: mark it consumed and return its user_id.
    ///
    /// Returns None if the token is unknown, expired, or already consumed.
    pub async fn consume_pending_2fa_token(
        &self,
        raw_token: &str,
    ) -> anyhow::Result<Option<Uuid>> {
        let hash = hash_pending_token(raw_token);
        let now = Utc::now();

        let row: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE pending_2fa_tokens
             SET consumed_at = $2
             WHERE token_hash = $1
               AND consumed_at IS NULL
               AND expires_at > $2
             RETURNING user_id",
        )
        .bind(&hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(uid,)| uid))
    }

    /// Cleanup expired pending tokens (background maintenance).
    pub async fn cleanup_pending_2fa_tokens(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM pending_2fa_tokens WHERE expires_at < NOW() OR consumed_at IS NOT NULL",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = "JBSWY3DPEHPK3PXP"; // example base32 secret
        let encrypted = encrypt_secret(&key, plaintext).expect("encrypt ok");
        let decrypted = decrypt_secret(&key, &encrypted).expect("decrypt ok");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_unique_ciphertexts() {
        let key = [0x42u8; 32];
        let pt = "JBSWY3DPEHPK3PXP";
        let c1 = encrypt_secret(&key, pt).unwrap();
        let c2 = encrypt_secret(&key, pt).unwrap();
        // Different nonces → different ciphertexts
        assert_ne!(c1, c2);
    }

    #[test]
    fn decrypt_fails_wrong_key() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let encrypted = encrypt_secret(&key1, "secret").unwrap();
        assert!(decrypt_secret(&key2, &encrypted).is_err());
    }

    #[test]
    fn decrypt_fails_tampered_ciphertext() {
        let key = [0x42u8; 32];
        let mut encrypted = encrypt_secret(&key, "secret").unwrap();
        // Flip last byte of ciphertext hex
        let last = encrypted.pop().unwrap();
        encrypted.push(if last == 'a' { 'b' } else { 'a' });
        assert!(decrypt_secret(&key, &encrypted).is_err());
    }

    #[test]
    fn backup_codes_generation() {
        let (raw, hashed) = generate_backup_codes();
        assert_eq!(raw.len(), BACKUP_CODE_COUNT);
        assert_eq!(hashed.len(), BACKUP_CODE_COUNT);
        for (code, hash) in raw.iter().zip(hashed.iter()) {
            assert_eq!(code.len(), BACKUP_CODE_LENGTH);
            assert_eq!(hash, &hash_backup_code(code));
        }
    }

    #[test]
    fn backup_codes_are_unique() {
        let (raw, _) = generate_backup_codes();
        let mut seen = std::collections::HashSet::new();
        for code in &raw {
            assert!(seen.insert(code.as_str()), "duplicate backup code: {code}");
        }
    }

    #[test]
    fn backup_code_hash_case_insensitive() {
        let code = "ABCDEFGH";
        assert_eq!(hash_backup_code(code), hash_backup_code(&code.to_lowercase()));
    }

    #[test]
    fn pending_token_is_64_hex_chars() {
        let token = generate_pending_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn pending_token_hashing_deterministic() {
        let token = "abc123";
        assert_eq!(hash_pending_token(token), hash_pending_token(token));
    }

    #[test]
    fn encrypted_secret_format() {
        let key = [0u8; 32];
        let ct = encrypt_secret(&key, "hello").unwrap();
        // Format must be "hex:hex"
        assert!(ct.contains(':'), "must contain ':'");
        let (nonce_hex, ct_hex) = ct.split_once(':').unwrap();
        assert_eq!(nonce_hex.len(), 24); // 12 bytes = 24 hex chars
        assert!(!ct_hex.is_empty());
        // Both parts must be valid hex
        hex::decode(nonce_hex).expect("nonce must be valid hex");
        hex::decode(ct_hex).expect("ciphertext must be valid hex");
    }
}
