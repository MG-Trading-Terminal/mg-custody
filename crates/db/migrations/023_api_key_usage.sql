-- Migration 023: API key daily usage tracking.
--
-- Tracks per-key, per-day request counts. The upsert in record_api_key_usage
-- increments the counter atomically using ON CONFLICT DO UPDATE.
-- This table is append-heavy; primary key (key_id, date) ensures idempotency.

CREATE TABLE IF NOT EXISTS api_key_usage (
    key_id         UUID    NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    date           DATE    NOT NULL,
    request_count  INTEGER NOT NULL DEFAULT 0,
    last_endpoint  TEXT,
    last_ip        TEXT,
    PRIMARY KEY (key_id, date)
);

CREATE INDEX IF NOT EXISTS idx_api_key_usage_date ON api_key_usage(date);
