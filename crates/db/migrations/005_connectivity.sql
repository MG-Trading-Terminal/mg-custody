-- Level 7: Exchange connections, API keys, notification channels, system config.

CREATE TABLE IF NOT EXISTS exchange_connections (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    connection_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    api_url TEXT,
    supported_chains TEXT[] NOT NULL DEFAULT '{}',
    auto_credit BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_heartbeat_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    ip_whitelist TEXT[] NOT NULL DEFAULT '{}',
    rate_limit_per_minute INTEGER NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);

CREATE TABLE IF NOT EXISTS notification_channels (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL,
    config JSONB NOT NULL,
    min_severity TEXT NOT NULL DEFAULT 'warning',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS system_config (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed default system config
INSERT INTO system_config (key, value) VALUES
    ('withdrawal_limit_single', '"500000"'),
    ('withdrawal_limit_hourly', '"10000000"'),
    ('withdrawal_limit_daily', '"50000000"'),
    ('withdrawals_paused', 'false'),
    ('deposits_paused', 'false'),
    ('maintenance_mode', 'false')
ON CONFLICT (key) DO NOTHING;
