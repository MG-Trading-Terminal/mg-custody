-- MG Custody: Initial Database Schema
-- All monetary amounts stored as DECIMAL(38,18) — never float
-- All timestamps in UTC

-- ============================================================================
-- Core tables
-- ============================================================================

-- Deposit addresses generated for users
CREATE TABLE IF NOT EXISTS addresses (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL,
    asset           TEXT NOT NULL,
    chain           TEXT NOT NULL,
    address         TEXT NOT NULL,
    derivation_path TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Idempotency: one address per (user_id, asset, chain)
    CONSTRAINT uq_addresses_user_asset_chain UNIQUE (user_id, asset, chain)
);

CREATE INDEX idx_addresses_chain ON addresses (chain);
CREATE INDEX idx_addresses_address ON addresses (address);

-- Atomic address index counter per chain
CREATE TABLE IF NOT EXISTS address_counters (
    chain       TEXT PRIMARY KEY,
    next_index  INTEGER NOT NULL DEFAULT 0
);

-- ============================================================================
-- Deposits
-- ============================================================================

CREATE TABLE IF NOT EXISTS deposits (
    id                      UUID PRIMARY KEY,
    user_id                 UUID NOT NULL,
    asset                   TEXT NOT NULL,
    chain                   TEXT NOT NULL,
    address                 TEXT NOT NULL,
    amount                  DECIMAL(38,18) NOT NULL,
    tx_hash                 TEXT NOT NULL,
    log_index               INTEGER,          -- for ERC-20 (multiple transfers per tx)
    block_height            BIGINT NOT NULL,
    confirmations           BIGINT NOT NULL DEFAULT 0,
    required_confirmations  BIGINT NOT NULL,
    status                  TEXT NOT NULL DEFAULT 'detected',
    detected_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confirmed_at            TIMESTAMPTZ,
    credited_at             TIMESTAMPTZ,

    -- Dedup: unique tx_hash + log_index (log_index NULL for native transfers)
    CONSTRAINT uq_deposits_tx_log UNIQUE (tx_hash, log_index)
);

CREATE INDEX idx_deposits_status ON deposits (status);
CREATE INDEX idx_deposits_chain ON deposits (chain);
CREATE INDEX idx_deposits_user ON deposits (user_id);

-- ============================================================================
-- Withdrawals
-- ============================================================================

CREATE TABLE IF NOT EXISTS withdrawals (
    id                      UUID PRIMARY KEY,
    exchange_withdrawal_id  UUID NOT NULL UNIQUE,  -- idempotency key from exchange
    -- Ownership: every withdrawal is bound to the user who initiated it and
    -- the vault whose funds are being moved. Required for authorization on
    -- cancel / accelerate / history queries — without these fields the
    -- ledger cannot be safely scoped per user.
    user_id                 UUID NOT NULL,
    vault_id                UUID,                  -- NULL only for legacy / exchange-HMAC flows
    asset                   TEXT NOT NULL,
    chain                   TEXT NOT NULL,
    destination_address     TEXT NOT NULL,
    amount                  DECIMAL(38,18) NOT NULL,
    fee                     DECIMAL(38,18),
    status                  TEXT NOT NULL DEFAULT 'received',
    wallet_tier             TEXT NOT NULL DEFAULT 'hot',
    tx_hash                 TEXT,
    block_height            BIGINT,
    confirmations           BIGINT NOT NULL DEFAULT 0,
    error_message           TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signed_at               TIMESTAMPTZ,
    broadcast_at            TIMESTAMPTZ,
    confirmed_at            TIMESTAMPTZ,
    delay_release_at        TIMESTAMPTZ
);

CREATE INDEX idx_withdrawals_status ON withdrawals (status);
CREATE INDEX idx_withdrawals_chain ON withdrawals (chain);
CREATE INDEX idx_withdrawals_exchange_id ON withdrawals (exchange_withdrawal_id);
CREATE INDEX idx_withdrawals_user_id ON withdrawals (user_id);
CREATE INDEX idx_withdrawals_vault_id ON withdrawals (vault_id);

-- ============================================================================
-- UTXO management (Bitcoin)
-- ============================================================================

CREATE TABLE IF NOT EXISTS utxos (
    id              UUID PRIMARY KEY,
    txid            TEXT NOT NULL,
    vout            INTEGER NOT NULL,
    amount_sats     BIGINT NOT NULL,
    address         TEXT NOT NULL,
    confirmations   BIGINT NOT NULL DEFAULT 0,
    locked          BOOLEAN NOT NULL DEFAULT FALSE,
    spending_tx     TEXT,                        -- txid that spent this UTXO
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_utxos_txid_vout UNIQUE (txid, vout)
);

CREATE INDEX idx_utxos_address ON utxos (address);
CREATE INDEX idx_utxos_unspent ON utxos (spending_tx) WHERE spending_tx IS NULL;

-- ============================================================================
-- Double-entry custody ledger
-- ============================================================================

CREATE TABLE IF NOT EXISTS custody_ledger (
    id              UUID PRIMARY KEY,
    entry_type      TEXT NOT NULL CHECK (entry_type IN ('debit', 'credit')),
    account         TEXT NOT NULL,       -- e.g., "hot:eth", "user:123:eth"
    amount          DECIMAL(38,18) NOT NULL,
    asset           TEXT NOT NULL,
    chain           TEXT NOT NULL,
    counterpart_id  UUID NOT NULL,       -- the other half of the double entry
    reference_id    UUID NOT NULL,       -- deposit/withdrawal/sweep ID
    reference_type  TEXT NOT NULL,       -- "deposit", "withdrawal", "sweep"
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ledger_account ON custody_ledger (account, asset);
CREATE INDEX idx_ledger_reference ON custody_ledger (reference_id);

-- ============================================================================
-- Scanner progress
-- ============================================================================

CREATE TABLE IF NOT EXISTS scan_progress (
    chain       TEXT PRIMARY KEY,
    last_block  BIGINT NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- Nonce counters (Ethereum-style)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nonce_counters (
    address     TEXT PRIMARY KEY,
    nonce       BIGINT NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- Webhook deliveries
-- ============================================================================

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id              UUID PRIMARY KEY,
    event_type      TEXT NOT NULL,
    payload         JSONB NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    max_attempts    INTEGER NOT NULL DEFAULT 10,
    status          TEXT NOT NULL DEFAULT 'pending',
    next_retry_at   TIMESTAMPTZ,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivered_at    TIMESTAMPTZ
);

CREATE INDEX idx_webhooks_pending ON webhook_deliveries (status, next_retry_at)
    WHERE status = 'pending';

-- ============================================================================
-- Audit log (tamper-evident)
-- ============================================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id              UUID PRIMARY KEY,
    timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    operation       TEXT NOT NULL,
    actor           TEXT NOT NULL,
    chain           TEXT,
    derivation_path TEXT,
    details         TEXT,
    prev_hash       TEXT           -- SHA-256 of previous entry (hash chain)
);

CREATE INDEX idx_audit_timestamp ON audit_log (timestamp DESC);

-- ============================================================================
-- Policy rules
-- ============================================================================

CREATE TABLE IF NOT EXISTS policy_rules (
    id          UUID PRIMARY KEY,
    name        TEXT NOT NULL,
    priority    INTEGER NOT NULL,
    condition   JSONB NOT NULL,
    action      JSONB NOT NULL,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_policy_rules_priority ON policy_rules (priority) WHERE enabled = TRUE;

-- ============================================================================
-- Address whitelist
-- ============================================================================

CREATE TABLE IF NOT EXISTS address_whitelist (
    id          UUID PRIMARY KEY,
    address     TEXT NOT NULL,
    chain       TEXT NOT NULL,
    label       TEXT NOT NULL,
    added_by    TEXT NOT NULL,
    approved_by JSONB NOT NULL DEFAULT '[]',
    expires_at  TIMESTAMPTZ,
    one_time    BOOLEAN NOT NULL DEFAULT FALSE,
    used        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_whitelist_chain_address UNIQUE (chain, address)
);

CREATE INDEX idx_whitelist_chain ON address_whitelist (chain);

-- ============================================================================
-- Approval requests
-- ============================================================================

CREATE TABLE IF NOT EXISTS approval_requests (
    id                  UUID PRIMARY KEY,
    withdrawal_id       UUID NOT NULL REFERENCES withdrawals(id),
    required_approvals  INTEGER NOT NULL,
    required_role       TEXT NOT NULL,
    decisions           JSONB NOT NULL DEFAULT '[]',
    status              TEXT NOT NULL DEFAULT 'pending',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at          TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_approvals_pending ON approval_requests (status)
    WHERE status = 'pending';
CREATE INDEX idx_approvals_withdrawal ON approval_requests (withdrawal_id);

-- ============================================================================
-- Compliance screening results
-- ============================================================================

CREATE TABLE IF NOT EXISTS compliance_screenings (
    id          UUID PRIMARY KEY,
    address     TEXT NOT NULL,
    chain       TEXT NOT NULL,
    risk_level  TEXT NOT NULL,
    risk_score  DOUBLE PRECISION NOT NULL,
    provider    TEXT NOT NULL,
    categories  JSONB NOT NULL DEFAULT '[]',
    details     TEXT,
    screened_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_screenings_address ON compliance_screenings (address, chain);

-- ============================================================================
-- Initialize address counters for known chains
-- ============================================================================

INSERT INTO address_counters (chain, next_index) VALUES
    ('bitcoin', 0),
    ('ethereum', 0),
    ('tron', 0),
    ('solana', 0)
ON CONFLICT (chain) DO NOTHING;

-- Initialize scan progress
INSERT INTO scan_progress (chain, last_block) VALUES
    ('bitcoin', 0),
    ('ethereum', 0),
    ('tron', 0),
    ('solana', 0)
ON CONFLICT (chain) DO NOTHING;
