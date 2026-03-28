-- Rate limit persistence.
--
-- Stores per-key sliding window counters so rate limit state survives restarts.
-- The in-memory SlidingWindow is the fast path; this table is the persistence layer.
-- A background task batch-writes dirty counters every 10 seconds.
--
-- window_seconds: the length of the rate limit window (default 60 = per-minute)
-- window_start:   when the current window began — rows older than window_start + window_seconds are stale
-- count:          number of requests recorded in the current window

CREATE TABLE IF NOT EXISTS rate_limit_counters (
    key             TEXT        PRIMARY KEY,
    count           INTEGER     NOT NULL DEFAULT 0,
    window_start    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    window_seconds  INTEGER     NOT NULL DEFAULT 60
);

CREATE INDEX IF NOT EXISTS idx_rate_limit_counters_window
    ON rate_limit_counters (window_start);
