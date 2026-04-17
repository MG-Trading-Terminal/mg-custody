-- ETH (Lido) / Solana (Marinade, future) staking positions.
--
-- Before this migration the staking routes returned 501 honestly; the
-- user could see "not wired" but had no way to actually stake through
-- the service. This table is the backbone of a real staking pipeline:
-- each row is one staking intent + its lifecycle.
--
-- A staking request is really a special withdrawal (the pipeline signs
-- and broadcasts a payable contract call). We keep that linkage via
-- `withdrawal_id` so the same state machine (Received → Signed →
-- Broadcast → Confirmed) covers both deposits into Lido and regular
-- outbound transfers — one pipeline, one audit trail.
--
-- Status:
--   pending       — row created, withdrawal inserted, pipeline has not
--                   signed yet
--   confirmed     — underlying withdrawal confirmed on-chain; user now
--                   holds stETH / native staked amount
--   unstaking     — unstake requested (Lido: queued via withdrawal NFT)
--   unstaked      — unstake completed, funds returned
--   failed        — signing or broadcast failed; operator review
--
-- `protocol` is a free-form string (`lido`, `rocketpool`, `marinade`,
-- `eth_native`) so new protocols slot in without a schema change.

CREATE TABLE IF NOT EXISTS staking_positions (
    id                   UUID        PRIMARY KEY,
    user_id              UUID        NOT NULL REFERENCES workspace_users(id),
    chain                TEXT        NOT NULL
        CHECK (chain IN ('ethereum', 'solana')),
    protocol             TEXT        NOT NULL,
    validator_address    TEXT        NOT NULL,
    asset                TEXT        NOT NULL,
    amount               NUMERIC(38, 18) NOT NULL,
    status               TEXT        NOT NULL
        CHECK (status IN ('pending', 'confirmed', 'unstaking', 'unstaked', 'failed')),
    /// Withdrawal row that drives signing + broadcast. NULL only for
    /// externally-initiated positions we later import by indexing.
    withdrawal_id        UUID        REFERENCES withdrawals(id),
    tx_hash              TEXT,
    unstake_tx_hash      TEXT,
    error_message        TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confirmed_at         TIMESTAMPTZ,
    unstaked_at          TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_staking_positions_user
    ON staking_positions (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_staking_positions_withdrawal
    ON staking_positions (withdrawal_id);

CREATE INDEX IF NOT EXISTS idx_staking_positions_protocol
    ON staking_positions (chain, protocol);
