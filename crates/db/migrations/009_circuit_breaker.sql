-- Phase 17C: Persistent circuit breaker state.
-- Fixes CRITICAL bug: counters previously reset on process restart.

CREATE TABLE IF NOT EXISTS circuit_breaker_state (
    id TEXT PRIMARY KEY DEFAULT 'global',
    hourly_window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    daily_window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    hourly_outflow_usd NUMERIC(38,18) NOT NULL DEFAULT 0,
    daily_outflow_usd NUMERIC(38,18) NOT NULL DEFAULT 0,
    is_tripped BOOLEAN NOT NULL DEFAULT false,
    tripped_at TIMESTAMPTZ,
    trip_reason TEXT,
    single_tx_limit_usd NUMERIC(38,18) NOT NULL DEFAULT 500000,
    hourly_limit_usd NUMERIC(38,18) NOT NULL DEFAULT 10000000,
    daily_limit_usd NUMERIC(38,18) NOT NULL DEFAULT 50000000,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the global circuit breaker state
INSERT INTO circuit_breaker_state (id) VALUES ('global') ON CONFLICT DO NOTHING;
