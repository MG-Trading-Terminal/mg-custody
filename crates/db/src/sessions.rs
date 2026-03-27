//! Session persistence for RBAC (Phase 19).
//!
//! Sessions are Bearer tokens: the raw token is returned to the client once,
//! and only the SHA-256 hash is stored in the database. Token lookup is by hash.
//!
//! Security properties:
//! - Token never stored in plaintext (SHA-256 hash only)
//! - Session bound to IP + User-Agent (hijack detection)
//! - Absolute timeout: 8 hours from creation
//! - Idle timeout: 30 minutes since last activity
//! - Revocation: explicit DELETE or background cleanup

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// User role, mirroring `custody_policy::rbac::Role`.
/// Duplicated here to avoid dependency cycle (db cannot depend on policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer = 0,
    Editor = 1,
    Signer = 2,
    Admin = 3,
    Owner = 4,
}

impl Role {
    pub fn has_at_least(&self, required: &Role) -> bool {
        (*self as u8) >= (*required as u8)
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "owner" => Role::Owner,
            "admin" => Role::Admin,
            "signer" => Role::Signer,
            "editor" => Role::Editor,
            _ => Role::Viewer,
        }
    }
}

use crate::CustodyDb;

/// Absolute session lifetime.
const SESSION_ABSOLUTE_TIMEOUT_HOURS: i64 = 8;
/// Idle timeout — session expires if unused for this long.
const SESSION_IDLE_TIMEOUT_MINUTES: i64 = 30;

/// A session as loaded from the database, enriched with user info.
#[derive(Debug, Clone)]
pub struct Session {
    pub token_hash: String,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: Role,
    pub user_enabled: bool,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Session {
    /// Check if this session is still valid (not expired, not revoked, not idle).
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if !self.user_enabled {
            return false;
        }
        if now > self.expires_at {
            return false;
        }
        let idle_deadline = self.last_active_at
            + Duration::minutes(SESSION_IDLE_TIMEOUT_MINUTES);
        if now > idle_deadline {
            return false;
        }
        true
    }

    /// Check if request IP matches session IP.
    pub fn ip_matches(&self, request_ip: &str) -> bool {
        self.ip_address == request_ip
    }
}

/// Hash a raw token for storage/lookup.
pub fn hash_token(raw_token: &str) -> String {
    hex::encode(Sha256::digest(raw_token.as_bytes()))
}

/// Generate a cryptographically random session token (32 bytes, hex-encoded = 64 chars).
pub fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    token_hash: String,
    user_id: Uuid,
    ip_address: String,
    user_agent: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_active_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    // Joined from workspace_users
    workspace_id: Uuid,
    email: String,
    role: String,
    enabled: bool,
}

impl SessionRow {
    fn into_session(self) -> Session {
        let role = Role::from_str(&self.role);
        Session {
            token_hash: self.token_hash,
            user_id: self.user_id,
            workspace_id: self.workspace_id,
            email: self.email,
            role,
            user_enabled: self.enabled,
            ip_address: self.ip_address,
            user_agent: self.user_agent,
            created_at: self.created_at,
            expires_at: self.expires_at,
            last_active_at: self.last_active_at,
            revoked_at: self.revoked_at,
        }
    }
}

/// Minimal session info for listing (without sensitive details).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub token_hash_prefix: String,
    pub user_id: Uuid,
    pub email: String,
    pub ip_address: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl CustodyDb {
    /// Create a new session. Returns the raw token (only time it's available).
    pub async fn create_session(
        &self,
        user_id: Uuid,
        ip_address: &str,
        user_agent: &str,
    ) -> anyhow::Result<String> {
        let raw_token = generate_token();
        let token_hash = hash_token(&raw_token);
        let now = Utc::now();
        let expires_at = now + Duration::hours(SESSION_ABSOLUTE_TIMEOUT_HOURS);

        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, ip_address, user_agent, created_at, expires_at, last_active_at)
             VALUES ($1, $2, $3, $4, $5, $6, $5)",
        )
        .bind(&token_hash)
        .bind(user_id)
        .bind(ip_address)
        .bind(user_agent)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(raw_token)
    }

    /// Load a session by raw token, joined with user info.
    /// Returns None if token doesn't exist.
    pub async fn load_session(&self, raw_token: &str) -> anyhow::Result<Option<Session>> {
        let token_hash = hash_token(raw_token);
        let row: Option<SessionRow> = sqlx::query_as(
            "SELECT s.token_hash, s.user_id, s.ip_address, s.user_agent,
                    s.created_at, s.expires_at, s.last_active_at, s.revoked_at,
                    u.workspace_id, u.email, u.role, u.enabled
             FROM sessions s
             JOIN workspace_users u ON s.user_id = u.id
             WHERE s.token_hash = $1",
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_session()))
    }

    /// Touch session — update last_active_at for idle timeout tracking.
    pub async fn touch_session(&self, token_hash: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE sessions SET last_active_at = NOW() WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Revoke a specific session.
    pub async fn revoke_session(&self, token_hash: &str) -> anyhow::Result<bool> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = NOW() WHERE token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token_hash)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Revoke all sessions for a user (e.g., on password change or user disable).
    pub async fn revoke_user_sessions(&self, user_id: Uuid) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// List active sessions for a user.
    pub async fn list_user_sessions(&self, user_id: Uuid) -> anyhow::Result<Vec<SessionInfo>> {
        let rows: Vec<(String, Uuid, String, String, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT s.token_hash, s.user_id, u.email, s.ip_address,
                    s.created_at, s.last_active_at, s.expires_at
             FROM sessions s
             JOIN workspace_users u ON s.user_id = u.id
             WHERE s.user_id = $1 AND s.revoked_at IS NULL AND s.expires_at > NOW()
             ORDER BY s.created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(hash, user_id, email, ip, created, last_active, expires)| {
            SessionInfo {
                token_hash_prefix: hash[..8].to_string(),
                user_id,
                email,
                ip_address: ip,
                created_at: created,
                last_active_at: last_active,
                expires_at: expires,
            }
        }).collect())
    }

    /// Cleanup expired and revoked sessions (background task).
    pub async fn cleanup_expired_sessions(&self) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM sessions WHERE expires_at < NOW() OR revoked_at IS NOT NULL",
        )
        .execute(&self.pool)
        .await?;
        let count = result.rows_affected();
        if count > 0 {
            tracing::info!(cleaned = count, "Expired sessions cleaned up");
        }
        Ok(count)
    }

    /// Find workspace user by email (for login).
    pub async fn find_user_by_email(
        &self,
        email: &str,
    ) -> anyhow::Result<Option<(Uuid, String, Option<String>)>> {
        // Returns (user_id, role, password_hash)
        let row: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
            "SELECT id, role, password_hash FROM workspace_users
             WHERE email = $1 AND enabled = true",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Set password hash for a user.
    pub async fn set_user_password_hash(
        &self,
        user_id: Uuid,
        password_hash: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE workspace_users SET password_hash = $2 WHERE id = $1",
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update last_login_at for a user.
    pub async fn update_last_login(&self, user_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE workspace_users SET last_login_at = NOW() WHERE id = $1",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hashing_deterministic() {
        let token = "abc123";
        assert_eq!(hash_token(token), hash_token(token));
    }

    #[test]
    fn token_hashing_different_inputs() {
        assert_ne!(hash_token("token_a"), hash_token("token_b"));
    }

    #[test]
    fn generated_token_is_64_hex_chars() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn session_validity_checks() {
        let now = Utc::now();
        let session = Session {
            token_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: "test@test.com".into(),
            role: Role::Admin,
            user_enabled: true,
            ip_address: "1.2.3.4".into(),
            user_agent: "test".into(),
            created_at: now,
            expires_at: now + Duration::hours(8),
            last_active_at: now,
            revoked_at: None,
        };

        // Valid session
        assert!(session.is_valid(now));

        // Expired session
        assert!(!session.is_valid(now + Duration::hours(9)));

        // Idle session (31 minutes inactive)
        assert!(!session.is_valid(now + Duration::minutes(31)));

        // Still valid at 29 minutes
        assert!(session.is_valid(now + Duration::minutes(29)));
    }

    #[test]
    fn revoked_session_invalid() {
        let now = Utc::now();
        let session = Session {
            token_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: "test@test.com".into(),
            role: Role::Admin,
            user_enabled: true,
            ip_address: "1.2.3.4".into(),
            user_agent: "test".into(),
            created_at: now,
            expires_at: now + Duration::hours(8),
            last_active_at: now,
            revoked_at: Some(now),
        };
        assert!(!session.is_valid(now));
    }

    #[test]
    fn disabled_user_session_invalid() {
        let now = Utc::now();
        let session = Session {
            token_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: "test@test.com".into(),
            role: Role::Admin,
            user_enabled: false,
            ip_address: "1.2.3.4".into(),
            user_agent: "test".into(),
            created_at: now,
            expires_at: now + Duration::hours(8),
            last_active_at: now,
            revoked_at: None,
        };
        assert!(!session.is_valid(now));
    }

    #[test]
    fn ip_binding() {
        let now = Utc::now();
        let session = Session {
            token_hash: "abc".into(),
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: "test@test.com".into(),
            role: Role::Admin,
            user_enabled: true,
            ip_address: "1.2.3.4".into(),
            user_agent: "test".into(),
            created_at: now,
            expires_at: now + Duration::hours(8),
            last_active_at: now,
            revoked_at: None,
        };
        assert!(session.ip_matches("1.2.3.4"));
        assert!(!session.ip_matches("5.6.7.8"));
    }
}
