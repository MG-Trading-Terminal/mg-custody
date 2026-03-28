-- Migration 022: device fingerprint + trusted devices + session limit enforcement.
--
-- A device fingerprint is a hash of (User-Agent + screen resolution + timezone).
-- When a user completes TOTP verification on a device they can opt to trust it
-- for 30 days, skipping 2FA on subsequent logins.
--
-- Session limit: max 5 active sessions per user. The application calls
-- enforce_session_limit() after creating a new session; this procedure revokes
-- the oldest session(s) if the count exceeds the configured maximum.

ALTER TABLE sessions ADD COLUMN IF NOT EXISTS device_fingerprint TEXT;

CREATE TABLE IF NOT EXISTS trusted_devices (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    device_fingerprint TEXT NOT NULL,
    name              TEXT,                     -- e.g. "Chrome on macOS"
    trusted_until     TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, device_fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_trusted_devices_user_id ON trusted_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_trusted_devices_until   ON trusted_devices(trusted_until);
