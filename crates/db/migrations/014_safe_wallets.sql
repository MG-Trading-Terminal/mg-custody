-- Gnosis Safe multi-sig wallet registry.
-- Stores Safe proxy addresses, threshold, and current nonce.

CREATE TABLE IF NOT EXISTS safe_wallets (
    address        TEXT    PRIMARY KEY,
    chain          TEXT    NOT NULL,
    threshold      INTEGER NOT NULL,
    -- JSON array of owner address strings
    owners         JSONB   NOT NULL DEFAULT '[]',
    -- On-chain nonce at last sync
    nonce          BIGINT  NOT NULL DEFAULT 0,
    -- Workspace user who registered this Safe — used for ownership checks
    -- on propose / sign / execute. NULL for Safes imported before migration 029.
    registered_by  UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Multi-sig transactions proposed against a Safe.
CREATE TABLE IF NOT EXISTS safe_transactions (
    id              UUID        PRIMARY KEY,
    safe_address    TEXT        NOT NULL REFERENCES safe_wallets(address),
    to_address      TEXT        NOT NULL,
    -- Wei amount as TEXT to preserve exact Decimal precision
    value_wei       TEXT        NOT NULL DEFAULT '0',
    data_bytes      BYTEA       NOT NULL DEFAULT '\x',
    -- "call" | "delegate_call"
    operation       TEXT        NOT NULL DEFAULT 'call',
    safe_tx_hash    TEXT        NOT NULL,
    -- JSON array of SafeSignature objects
    signatures      JSONB       NOT NULL DEFAULT '[]',
    -- "proposed" | "signing" | "ready" | "executed" | "failed"
    status          TEXT        NOT NULL DEFAULT 'proposed',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_safe_txs_safe_address
    ON safe_transactions (lower(safe_address));

CREATE INDEX IF NOT EXISTS idx_safe_txs_status
    ON safe_transactions (status)
    WHERE status IN ('proposed', 'signing', 'ready');
