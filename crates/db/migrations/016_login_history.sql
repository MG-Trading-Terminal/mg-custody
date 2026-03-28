-- Migration 016: Login history and account lockout
--
-- Records every login attempt (success and failure) for audit, brute-force
-- detection, and display in the user dashboard.
--
-- Brute-force protection: count_failed_logins() is used by the login handler
-- to enforce a 5-minute cooldown after 5 consecutive failures within 15 minutes.

CREATE TABLE IF NOT EXISTS login_history (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    ip_address      TEXT NOT NULL,
    user_agent      TEXT NOT NULL DEFAULT '',
    -- Authentication method: 'password', 'totp', 'fido2', 'backup_code', 'email_otp'
    method          TEXT NOT NULL DEFAULT 'password',
    success         BOOLEAN NOT NULL,
    -- Populated on failure: 'invalid_password', 'invalid_totp', 'account_disabled',
    --   'account_locked', 'invalid_email_otp'
    failure_reason  TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- For per-user history listing (newest-first) and brute-force count queries.
CREATE INDEX IF NOT EXISTS idx_login_history_user
    ON login_history(user_id, created_at DESC);
