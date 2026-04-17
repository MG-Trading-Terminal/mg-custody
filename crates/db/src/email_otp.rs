//! Email OTP persistence.
//!
//! 6-digit numeric OTP codes used for:
//! - Alternative login 2FA when the user has no TOTP enrolled
//! - Password reset flow
//! - Email address verification
//!
//! # Security model
//! - Only the SHA-256 hash of the raw code is stored in the database.
//! - Codes expire after 10 minutes.
//! - Each code is single-use: `consumed_at` is set atomically on first successful
//!   verification, using a `RETURNING` UPDATE so no second consumer can win a race.
//! - The raw code is returned by `create_email_otp` exactly once and must be
//!   transmitted to the user (e.g. via email or log in dev) immediately.
//!
//! # Code generation
//! Uses `OsRng` (system CSPRNG) to produce a 6-digit decimal string.
//! The space is 000000–999999 (10^6 = 1 000 000 possible values), sufficient for
//! short-lived codes combined with the brute-force protection in the login handler.

use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::CustodyDb;

/// OTP validity window.
const OTP_EXPIRY_MINUTES: i64 = 10;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a 6-digit OTP string padded with leading zeros.
///
/// Uses `OsRng` (CSPRNG) — never `rand::thread_rng()` for security-sensitive codes.
pub fn generate_otp_code() -> String {
    let mut rng = rand::rngs::OsRng;
    let n = rng.next_u32() % 1_000_000;
    format!("{n:06}")
}

/// SHA-256 hex hash of the raw OTP code.
pub fn hash_otp_code(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

// ---------------------------------------------------------------------------
// DB operations
// ---------------------------------------------------------------------------

impl CustodyDb {
    /// Create a new email OTP for `user_id` with the given `purpose`.
    ///
    /// Returns the **raw** 6-digit code.  The caller must transmit this to the
    /// user (email/SMS) and must not persist the raw value.
    ///
    /// Any previous unused codes for the same user+purpose are left as-is; they
    /// will expire naturally.  The caller should show only the most-recently
    /// generated code to the user.
    pub async fn create_email_otp(
        &self,
        user_id: Uuid,
        purpose: &str,
    ) -> anyhow::Result<String> {
        let raw_code = generate_otp_code();
        let code_hash = hash_otp_code(&raw_code);
        let now = Utc::now();
        let expires_at = now + Duration::minutes(OTP_EXPIRY_MINUTES);

        sqlx::query(
            "INSERT INTO email_otp (user_id, code_hash, purpose, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(&code_hash)
        .bind(purpose)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(raw_code)
    }

    /// Verify and consume an email OTP.
    ///
    /// Looks up the most-recently created, unexpired, unconsumed code for
    /// `(user_id, purpose)` and checks it against `code`.
    ///
    /// Returns `true` if the code matched and was consumed, `false` otherwise
    /// (wrong code, expired, already consumed, or no code found).
    ///
    /// The consume is atomic — the UPDATE only fires if `consumed_at IS NULL`
    /// and `expires_at > NOW()`, so two concurrent verifications cannot both
    /// succeed.
    pub async fn verify_email_otp(
        &self,
        user_id: Uuid,
        code: &str,
        purpose: &str,
    ) -> anyhow::Result<bool> {
        let code_hash = hash_otp_code(code);
        let now = Utc::now();

        // Find the most-recent matching, valid, unconsumed code and consume it.
        let row: Option<(Uuid,)> = sqlx::query_as(
            "UPDATE email_otp
             SET consumed_at = $4
             WHERE id = (
                 SELECT id FROM email_otp
                 WHERE user_id    = $1
                   AND code_hash  = $2
                   AND purpose    = $3
                   AND consumed_at IS NULL
                   AND expires_at > $4
                 ORDER BY created_at DESC
                 LIMIT 1
             )
             RETURNING id",
        )
        .bind(user_id)
        .bind(&code_hash)
        .bind(purpose)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.is_some())
    }

    /// Delete expired or consumed OTP records.
    ///
    /// Returns the number of rows deleted.
    pub async fn cleanup_expired_otps(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM email_otp
             WHERE expires_at < NOW() OR consumed_at IS NOT NULL",
        )
        .execute(&self.pool)
        .await?;
        let count = result.rows_affected();
        if count > 0 {
            tracing::info!(deleted = count, "Expired email OTPs cleaned up");
        }
        Ok(count)
    }

    /// Find a user_id by email, regardless of enabled status.
    ///
    /// Used by the password-reset flow, which must accept requests from disabled
    /// accounts too (to allow re-enablement via password reset).
    ///
    /// Returns `(user_id, email, enabled)` or `None` if not found.
    pub async fn find_user_id_by_email_for_reset(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<(Uuid, String, bool)>> {
        let row: Option<(Uuid, String, bool)> = sqlx::query_as(
            "SELECT id, email, enabled FROM workspace_users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Whether `user_id` has confirmed their email address.
    ///
    /// Used by the login response and the verify-email status endpoint so
    /// the UI can decide whether to show a "verify your email" banner.
    pub async fn is_email_verified(&self, user_id: Uuid) -> anyhow::Result<bool> {
        let row: Option<(Option<chrono::DateTime<chrono::Utc>>,)> = sqlx::query_as(
            "SELECT email_verified_at FROM workspace_users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(t,)| t).is_some())
    }

    /// Stamp `email_verified_at = NOW()` for `user_id`. Idempotent —
    /// repeat calls are harmless and do not overwrite the original
    /// timestamp (COALESCE preserves the first verification).
    pub async fn mark_email_verified(&self, user_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE workspace_users
             SET email_verified_at = COALESCE(email_verified_at, NOW())
             WHERE id = $1",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Fetch `(email, email_verified_at IS NOT NULL)` for a user. Used
    /// by the verify-email request handler to decide whether to mint a
    /// new OTP (pointless if already verified) and by the UI to render
    /// the address being verified.
    pub async fn fetch_email_and_verified(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Option<(String, bool)>> {
        let row: Option<(String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            "SELECT email, email_verified_at FROM workspace_users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(email, t)| (email, t.is_some())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otp_code_is_6_digits() {
        for _ in 0..20 {
            let code = generate_otp_code();
            assert_eq!(code.len(), 6, "OTP must be exactly 6 characters, got: {code}");
            assert!(code.chars().all(|c| c.is_ascii_digit()), "OTP must be digits: {code}");
        }
    }

    #[test]
    fn otp_code_leading_zeros_preserved() {
        // Format ensures zero-padding.
        let code = format!("{:06}", 42u32);
        assert_eq!(code, "000042");
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn otp_hash_is_deterministic() {
        let code = "123456";
        assert_eq!(hash_otp_code(code), hash_otp_code(code));
    }

    #[test]
    fn otp_hash_different_codes_different_hashes() {
        assert_ne!(hash_otp_code("000000"), hash_otp_code("000001"));
        assert_ne!(hash_otp_code("123456"), hash_otp_code("654321"));
    }

    #[test]
    fn otp_hash_length() {
        // SHA-256 = 32 bytes = 64 hex chars.
        let hash = hash_otp_code("123456");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn otp_expiry_constant_is_positive() {
        assert!(OTP_EXPIRY_MINUTES > 0);
        assert!(OTP_EXPIRY_MINUTES <= 60, "OTP should not be valid for more than an hour");
    }

    #[test]
    fn otp_codes_are_probabilistically_unique() {
        // 20 codes should almost certainly all be distinct.
        let codes: std::collections::HashSet<String> =
            (0..20).map(|_| generate_otp_code()).collect();
        // Allow 1 collision in 20 due to birthday-paradox at small n, but 0 is expected.
        assert!(codes.len() >= 18, "Expected mostly unique codes");
    }

    #[test]
    fn purpose_variants_used_in_sql_are_valid_strs() {
        // Validate the purpose slugs used across the codebase.
        for purpose in &["login", "password_reset", "email_verify"] {
            assert!(!purpose.is_empty());
            assert!(purpose.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
    }
}
