-- Readiness transition log.
--
-- Every time a feature in the readiness report flips state in an
-- "alertable" way (Ready → Degraded / Misconfigured, or back to
-- Ready), the refresh task writes a row here. The dashboard's
-- "Recent incidents" timeline reads from this table; operators
-- writing postmortems can query it by `feature` + time range to
-- reconstruct "when did scanner.ethereum go down last Thursday?".
--
-- The in-memory probe results are the source of truth for *current*
-- state (readiness report, Prometheus gauges, handler gates). This
-- table exists purely for history — it never gets read by the hot
-- path, so we don't index it on the refresh write and keep writes
-- as plain INSERTs.
--
-- Retention is a background concern: the refresh task does not
-- prune. A separate maintenance job (or a pg_cron rule on the DB
-- host) should delete rows older than N days. One row per transition
-- per feature is cheap — even a flapping feature produces only a few
-- hundred rows per day.

CREATE TABLE IF NOT EXISTS readiness_transitions (
    id BIGSERIAL PRIMARY KEY,
    -- Feature that transitioned (e.g. "scanner.ethereum").
    feature TEXT NOT NULL,
    -- One of: "degradation", "misconfiguration", "recovery".
    -- Left as TEXT instead of an enum because the set grows slowly
    -- and adding an enum value in PG is a schema migration we'd
    -- rather avoid for a log table.
    kind TEXT NOT NULL,
    -- Human-readable reason (e.g. the probe error, or the missing
    -- config key list). Nullable because recovery transitions don't
    -- carry a reason beyond the feature name.
    reason TEXT,
    -- When the transition was observed by the refresh task. `clock_timestamp()`
    -- rather than the transaction's `now()` because the refresh task
    -- is a long-lived tokio task, not a per-request transaction.
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

-- Lookups are by (feature, occurred_at DESC) for the timeline, and
-- by occurred_at alone for the "last 24 h" dashboard section.
CREATE INDEX IF NOT EXISTS idx_readiness_transitions_feature_time
    ON readiness_transitions (feature, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_readiness_transitions_time
    ON readiness_transitions (occurred_at DESC);
