-- Migration 011: Session management for RBAC (Phase 19)
--
-- Adds password_hash to workspace_users and creates sessions table.
-- Sessions are stored in PostgreSQL (not Redis) per design: "all state in PostgreSQL".

-- Add password_hash to workspace_users (argon2id hash)
ALTER TABLE workspace_users ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- Sessions table — Bearer token auth for API and dashboard
CREATE TABLE IF NOT EXISTS sessions (
    -- SHA-256 of the actual token (token never stored in plaintext)
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES workspace_users(id) ON DELETE CASCADE,
    -- Binding: reject if IP or UA changes mid-session
    ip_address TEXT NOT NULL,
    user_agent TEXT NOT NULL DEFAULT '',
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,         -- absolute timeout (8h from creation)
    last_active_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),  -- idle timeout tracking (30m)
    -- Revocation
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

-- Audit: permission denials
-- Extends existing audit_log with new operation types (no schema change needed,
-- operation is stored as JSON text: "permission_denied", "session_created", etc.)
