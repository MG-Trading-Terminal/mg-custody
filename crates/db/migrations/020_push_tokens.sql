-- Migration 020: Device push notification tokens
--
-- Stores FCM (Android), APNs (iOS), and Web Push subscription tokens
-- for sending real-time custody alerts to operator devices.
--
-- Tokens are per-user, per-platform. A user can have multiple devices.
-- The UNIQUE constraint on `token` prevents duplicate registrations.

CREATE TABLE IF NOT EXISTS device_tokens (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- Platform: 'ios', 'android', 'web'
    platform        TEXT NOT NULL CHECK (platform IN ('ios', 'android', 'web')),
    -- Platform-specific token:
    --   ios:     APNs device token (hex, 64 chars)
    --   android: FCM registration token (variable)
    --   web:     JSON-encoded PushSubscription object
    token           TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ,
    -- Enforce one token registration per physical device across all users.
    CONSTRAINT uq_device_token UNIQUE (token)
);

-- For per-user device listing
CREATE INDEX IF NOT EXISTS idx_device_tokens_user
    ON device_tokens(user_id, created_at DESC);
