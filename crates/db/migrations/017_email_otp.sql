-- Migration 017: Email OTP — 2FA alternative and password-reset flow
--
-- email_otp holds short-lived 6-digit codes whose SHA-256 hashes are stored.
-- Raw codes are never persisted; only the hash is written to DB.
--
-- Purposes:
--   'login'          — alternative 2FA when the user has no TOTP enrolled
--   'password_reset' — verify identity before allowing a password change
--   'email_verify'   — verify ownership of an email address
--
-- A code is considered valid when:
--   consumed_at IS NULL AND expires_at > NOW()
--
-- Brute-force protection on verification is handled at the application layer
-- via login_history.count_failed_logins().

CREATE TABLE IF NOT EXISTS email_otp (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- SHA-256 hex of the raw 6-digit code. Raw code never stored.
    code_hash    TEXT NOT NULL,
    -- 'login', 'password_reset', 'email_verify'
    purpose      TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Codes expire after 10 minutes.
    expires_at   TIMESTAMPTZ NOT NULL,
    -- NULL = still valid; set to consumption timestamp when used.
    consumed_at  TIMESTAMPTZ
);

-- Lookup index: find the most-recent active code for a user+purpose pair.
CREATE INDEX IF NOT EXISTS idx_email_otp_user
    ON email_otp(user_id, purpose);
