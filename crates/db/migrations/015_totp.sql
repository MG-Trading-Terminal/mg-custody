-- Migration 015: TOTP 2FA support
--
-- Stores TOTP secrets encrypted at rest (AES-256-GCM).
-- Backup codes are stored as SHA-256 hashes — each code is single-use.
-- A pending_2fa_tokens table holds short-lived intermediate tokens
-- issued after password verification but before TOTP, so the login
-- flow can be completed in two steps without exposing a full session.

CREATE TABLE IF NOT EXISTS user_totp (
    user_id         UUID PRIMARY KEY REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- AES-256-GCM encrypted base32 secret. Format: hex(nonce)||':'||hex(ciphertext)
    secret_encrypted TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT false,
    -- 8 backup codes, stored as SHA-256(code) — single-use
    backup_codes    TEXT[] NOT NULL DEFAULT '{}',
    -- Which backup codes have been consumed (by index 0-7)
    backup_codes_used BOOLEAN[] NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ
);

-- Pending 2FA tokens: issued after password check, before TOTP verification.
-- Short-lived (5 minutes). Consumed once TOTP or backup code passes.
CREATE TABLE IF NOT EXISTS pending_2fa_tokens (
    token_hash  TEXT PRIMARY KEY,         -- SHA-256 of the raw token
    user_id     UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ               -- NULL = still valid
);

CREATE INDEX IF NOT EXISTS idx_pending_2fa_user ON pending_2fa_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_pending_2fa_expires ON pending_2fa_tokens(expires_at);
