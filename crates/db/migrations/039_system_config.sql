-- Persistent runtime system configuration.
--
-- Before this migration, `get_system_config` / `update_system_config`
-- handlers returned `SystemConfig::default()` and never touched the
-- DB. The emergency-pause flag, withdrawal limits, and confirmation
-- overrides were purely in-memory — a restart wiped any operator
-- decision. That's specifically dangerous for the pause flag: the
-- operator flips it at 2am to stop a leak, a deploy or pod restart
-- at 3am silently re-enables withdrawals, and nobody knows until
-- the next outflow lands.
--
-- Singleton table: one row, `id = 1`, enforced by a CHECK constraint
-- so no accidental multi-row state. Handlers INSERT ON CONFLICT
-- DO UPDATE; the row is created by the first `update` call or
-- lazily on first read. Every change is additionally written to
-- audit_log at the handler layer (not by a trigger — we want the
-- actor id from AuthContext, not the Postgres user).

CREATE TABLE IF NOT EXISTS system_config (
    id                       INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    withdrawal_limit_single  NUMERIC NOT NULL,
    withdrawal_limit_hourly  NUMERIC NOT NULL,
    withdrawal_limit_daily   NUMERIC NOT NULL,
    withdrawals_paused       BOOLEAN NOT NULL,
    deposits_paused          BOOLEAN NOT NULL,
    maintenance_mode         BOOLEAN NOT NULL,
    -- JSONB so the ConfirmationOverride array can grow without
    -- a schema migration each time a chain is added.
    confirmation_overrides   JSONB   NOT NULL DEFAULT '[]'::jsonb,
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_by               TEXT
);
