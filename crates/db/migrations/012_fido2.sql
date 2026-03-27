-- Migration 012: FIDO2/WebAuthn credential and challenge storage (Phase 20)
--
-- Credentials store the public key from hardware security keys (YubiKey, etc.).
-- Challenges are single-use, server-generated nonces for registration and authentication.

CREATE TABLE IF NOT EXISTS fido2_credentials (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- WebAuthn credential ID (base64url-encoded, from authenticator)
    credential_id TEXT NOT NULL UNIQUE,
    -- COSE public key (base64url-encoded)
    public_key_cose TEXT NOT NULL,
    -- Monotonically increasing sign counter (detects cloned keys)
    sign_count BIGINT NOT NULL DEFAULT 0,
    -- Authenticator AAGUID (identifies make/model)
    aaguid TEXT,
    -- Human-readable name (e.g., "YubiKey 5 NFC")
    device_name TEXT,
    -- Timestamps
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    -- Soft-disable (admin can revoke without deleting)
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX IF NOT EXISTS idx_fido2_credentials_user_id ON fido2_credentials(user_id);

CREATE TABLE IF NOT EXISTS fido2_challenges (
    -- SHA-256 hash of the challenge (never store raw challenge)
    challenge_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- "registration" or "authentication"
    purpose TEXT NOT NULL,
    -- Single-use: consumed_at is set after successful verification
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_fido2_challenges_user_id ON fido2_challenges(user_id);
CREATE INDEX IF NOT EXISTS idx_fido2_challenges_expires ON fido2_challenges(expires_at);
