-- Scheduled and recurring withdrawal definitions.
-- Each row describes a withdrawal rule; execution history is tracked via
-- executions_count + last_executed. The actual withdrawal rows are written
-- to the withdrawals table by the background scheduler.

CREATE TABLE IF NOT EXISTS scheduled_withdrawals (
    id              UUID        PRIMARY KEY,
    user_id         UUID        NOT NULL,
    chain           TEXT        NOT NULL,
    asset           TEXT        NOT NULL,
    -- Stored as TEXT to preserve exact decimal precision (never float)
    amount          TEXT        NOT NULL,
    destination     TEXT        NOT NULL,
    -- "once" | "daily" | "weekly" | "monthly"
    schedule_type   TEXT        NOT NULL,
    -- JSON blob with schedule-specific fields (at, hour_utc, day, day_of_month)
    schedule_config JSONB       NOT NULL DEFAULT '{}',
    next_execution  TIMESTAMPTZ NOT NULL,
    last_executed   TIMESTAMPTZ,
    executions_count INTEGER    NOT NULL DEFAULT 0,
    max_executions  INTEGER,
    enabled         BOOLEAN     NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Fast lookup for the scheduler: find all schedules due for execution
CREATE INDEX IF NOT EXISTS idx_scheduled_withdrawals_due
    ON scheduled_withdrawals (next_execution)
    WHERE enabled = true;

-- Lookup by user
CREATE INDEX IF NOT EXISTS idx_scheduled_withdrawals_user
    ON scheduled_withdrawals (user_id);
