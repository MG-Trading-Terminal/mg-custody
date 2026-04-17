-- Bitcoin UTXO consolidation jobs.
--
-- The sweep scheduler in server/main.rs identifies groups of small
-- UTXOs it wants to merge into a single hot-wallet output. Before
-- this migration the scheduler logged the plan and stopped; the
-- pipeline integration was a TODO comment next to a
-- "bc1q_hot_wallet_placeholder" literal (fixed in an earlier commit).
--
-- A consolidation job is the persisted handle that lets a separate
-- worker sign + broadcast independently of the scheduler's discovery
-- pass, with crash-safe progress. One row per planned consolidation
-- tx; status advances Received → Signed → Broadcast → Confirmed or
-- Failed, following the same shape as the user-withdrawal state
-- machine. The JSONB `inputs` column carries the exact set of UTXOs
-- the plan selected so re-running after a restart replays the same
-- inputs rather than re-running UTXO selection.

CREATE TABLE IF NOT EXISTS consolidation_jobs (
    id                   UUID        PRIMARY KEY,
    status               TEXT        NOT NULL
        CHECK (status IN ('received','signed','broadcast','confirmed','failed')),
    destination_address  TEXT        NOT NULL,
    inputs               JSONB       NOT NULL,
    total_input_sats     BIGINT      NOT NULL,
    output_sats          BIGINT      NOT NULL,
    fee_sats             BIGINT      NOT NULL,
    fee_rate_sat_per_vb  BIGINT      NOT NULL,
    signed_tx_bytes      BYTEA,
    tx_hash              TEXT,
    block_height         BIGINT,
    confirmations        BIGINT      NOT NULL DEFAULT 0,
    error_message        TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signed_at            TIMESTAMPTZ,
    broadcast_at         TIMESTAMPTZ,
    confirmed_at         TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_consolidation_jobs_status
    ON consolidation_jobs (status, created_at);

CREATE INDEX IF NOT EXISTS idx_consolidation_jobs_tx_hash
    ON consolidation_jobs (tx_hash);
