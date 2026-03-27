//! FIDO2/WebAuthn credential and challenge persistence (Phase 20).
//!
//! Security properties:
//! - Challenges stored as SHA-256 hashes (never plaintext)
//! - Challenges are single-use (consumed_at set after verification)
//! - Challenge expiry: 5 minutes
//! - Sign count must monotonically increase (cloned key detection)
//! - Credentials can be soft-disabled without deletion

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::CustodyDb;

/// Challenge expiry duration.
const CHALLENGE_EXPIRY_MINUTES: i64 = 5;

/// A stored FIDO2 credential.
#[derive(Debug, Clone)]
pub struct StoredCredential {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: String,
    pub public_key_cose: String,
    pub sign_count: i64,
    pub aaguid: Option<String>,
    pub device_name: Option<String>,
    pub registered_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub enabled: bool,
}

/// A stored challenge.
#[derive(Debug, Clone)]
pub struct StoredChallenge {
    pub challenge_hash: String,
    pub user_id: Uuid,
    pub purpose: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl StoredChallenge {
    /// Check if this challenge is valid (not expired, not consumed).
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.consumed_at.is_none() && now < self.expires_at
    }
}

/// Hash a raw challenge for storage/lookup.
pub fn hash_challenge(raw_challenge: &str) -> String {
    hex::encode(Sha256::digest(raw_challenge.as_bytes()))
}

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: Uuid,
    user_id: Uuid,
    credential_id: String,
    public_key_cose: String,
    sign_count: i64,
    aaguid: Option<String>,
    device_name: Option<String>,
    registered_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    enabled: bool,
}

impl From<CredentialRow> for StoredCredential {
    fn from(r: CredentialRow) -> Self {
        StoredCredential {
            id: r.id,
            user_id: r.user_id,
            credential_id: r.credential_id,
            public_key_cose: r.public_key_cose,
            sign_count: r.sign_count,
            aaguid: r.aaguid,
            device_name: r.device_name,
            registered_at: r.registered_at,
            last_used_at: r.last_used_at,
            enabled: r.enabled,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ChallengeRow {
    challenge_hash: String,
    user_id: Uuid,
    purpose: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
}

impl From<ChallengeRow> for StoredChallenge {
    fn from(r: ChallengeRow) -> Self {
        StoredChallenge {
            challenge_hash: r.challenge_hash,
            user_id: r.user_id,
            purpose: r.purpose,
            created_at: r.created_at,
            expires_at: r.expires_at,
            consumed_at: r.consumed_at,
        }
    }
}

impl CustodyDb {
    // -----------------------------------------------------------------------
    // Credentials
    // -----------------------------------------------------------------------

    /// Store a new FIDO2 credential after successful registration.
    pub async fn insert_fido2_credential(&self, cred: &StoredCredential) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO fido2_credentials
             (id, user_id, credential_id, public_key_cose, sign_count, aaguid, device_name, registered_at, enabled)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(cred.id)
        .bind(cred.user_id)
        .bind(&cred.credential_id)
        .bind(&cred.public_key_cose)
        .bind(cred.sign_count)
        .bind(&cred.aaguid)
        .bind(&cred.device_name)
        .bind(cred.registered_at)
        .bind(cred.enabled)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Find a credential by its WebAuthn credential_id.
    pub async fn find_fido2_credential_by_cred_id(
        &self,
        credential_id: &str,
    ) -> anyhow::Result<Option<StoredCredential>> {
        let row: Option<CredentialRow> = sqlx::query_as(
            "SELECT id, user_id, credential_id, public_key_cose, sign_count, aaguid,
                    device_name, registered_at, last_used_at, enabled
             FROM fido2_credentials WHERE credential_id = $1",
        )
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// List all credentials for a user.
    pub async fn list_fido2_credentials(
        &self,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<StoredCredential>> {
        let rows: Vec<CredentialRow> = sqlx::query_as(
            "SELECT id, user_id, credential_id, public_key_cose, sign_count, aaguid,
                    device_name, registered_at, last_used_at, enabled
             FROM fido2_credentials WHERE user_id = $1 ORDER BY registered_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Update sign count and last_used_at after successful authentication.
    /// Returns false if new_count <= current count (cloned key detection).
    pub async fn update_fido2_sign_count(
        &self,
        credential_id: &str,
        new_count: i64,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE fido2_credentials
             SET sign_count = $2, last_used_at = NOW()
             WHERE credential_id = $1 AND sign_count < $2 AND enabled = true",
        )
        .bind(credential_id)
        .bind(new_count)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Disable a credential (soft revoke).
    pub async fn disable_fido2_credential(&self, id: Uuid) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE fido2_credentials SET enabled = false WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Count active FIDO2 credentials for a user.
    pub async fn count_fido2_credentials(&self, user_id: Uuid) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM fido2_credentials WHERE user_id = $1 AND enabled = true",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Check if a user has any active FIDO2 credentials.
    pub async fn user_has_fido2(&self, user_id: Uuid) -> anyhow::Result<bool> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM fido2_credentials WHERE user_id = $1 AND enabled = true",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0 > 0)
    }

    // -----------------------------------------------------------------------
    // Challenges
    // -----------------------------------------------------------------------

    /// Create a challenge for registration or authentication.
    /// Returns the raw challenge (only time it's available in plaintext).
    pub async fn create_fido2_challenge(
        &self,
        user_id: Uuid,
        purpose: &str,
    ) -> anyhow::Result<String> {
        let raw_challenge = custody_common::fido2::generate_challenge();
        let challenge_hash = hash_challenge(&raw_challenge);
        let now = Utc::now();
        let expires_at = now + Duration::minutes(CHALLENGE_EXPIRY_MINUTES);

        sqlx::query(
            "INSERT INTO fido2_challenges (challenge_hash, user_id, purpose, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&challenge_hash)
        .bind(user_id)
        .bind(purpose)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(raw_challenge)
    }

    /// Verify and consume a challenge (single-use).
    /// Returns the challenge if valid, None if not found/expired/consumed.
    pub async fn consume_fido2_challenge(
        &self,
        raw_challenge: &str,
        purpose: &str,
    ) -> anyhow::Result<Option<StoredChallenge>> {
        let challenge_hash = hash_challenge(raw_challenge);
        let now = Utc::now();

        // Find and consume atomically
        let row: Option<ChallengeRow> = sqlx::query_as(
            "UPDATE fido2_challenges
             SET consumed_at = NOW()
             WHERE challenge_hash = $1
               AND purpose = $2
               AND consumed_at IS NULL
               AND expires_at > $3
             RETURNING challenge_hash, user_id, purpose, created_at, expires_at, consumed_at",
        )
        .bind(&challenge_hash)
        .bind(purpose)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    /// Cleanup expired challenges (background task).
    pub async fn cleanup_expired_challenges(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM fido2_challenges WHERE expires_at < NOW() OR consumed_at IS NOT NULL",
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
    fn challenge_hashing_deterministic() {
        assert_eq!(hash_challenge("test"), hash_challenge("test"));
    }

    #[test]
    fn challenge_hashing_different() {
        assert_ne!(hash_challenge("a"), hash_challenge("b"));
    }

    #[test]
    fn stored_challenge_validity() {
        let now = Utc::now();
        let valid = StoredChallenge {
            challenge_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            purpose: "registration".into(),
            created_at: now,
            expires_at: now + Duration::minutes(5),
            consumed_at: None,
        };
        assert!(valid.is_valid(now));
        assert!(valid.is_valid(now + Duration::minutes(4)));
        assert!(!valid.is_valid(now + Duration::minutes(6)));
    }

    #[test]
    fn consumed_challenge_invalid() {
        let now = Utc::now();
        let consumed = StoredChallenge {
            challenge_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            purpose: "authentication".into(),
            created_at: now,
            expires_at: now + Duration::minutes(5),
            consumed_at: Some(now),
        };
        assert!(!consumed.is_valid(now));
    }
}
