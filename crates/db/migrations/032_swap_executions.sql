-- Persisted swap executions.
--
-- Before this migration, `POST /v1/swaps` built a SwapExecution record
-- in memory, logged it, and dropped it. The matching
-- `GET /v1/swaps/{id}` endpoint always returned 404 with the message
-- "swap persistence is not yet wired to DB" — the feature looked
-- functional but the status half was a dead end.
--
-- With this table the gateway writes on execute and reads on status.
-- `user_id` is the signing-role caller who invoked `execute_swap`;
-- status queries are scoped to that user unless the caller is Admin.
-- Amounts are stored NUMERIC(38,18) to match the rest of the ledger.

CREATE TABLE IF NOT EXISTS swap_executions (
    id              UUID            PRIMARY KEY,
    quote_id        UUID            NOT NULL,
    user_id         UUID            NOT NULL REFERENCES workspace_users(id),
    chain           TEXT            NOT NULL,
    protocol        TEXT            NOT NULL,
    from_token      TEXT            NOT NULL,
    to_token        TEXT            NOT NULL,
    amount_in       NUMERIC(38,18)  NOT NULL,
    expected_out    NUMERIC(38,18)  NOT NULL,
    minimum_out     NUMERIC(38,18)  NOT NULL,
    slippage_bps    INTEGER         NOT NULL,
    status          TEXT            NOT NULL,
    tx_hash         TEXT,
    calldata        TEXT,
    encoded_tx      TEXT,
    error_message   TEXT,
    created_at      TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_swap_executions_user
    ON swap_executions (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_swap_executions_quote
    ON swap_executions (quote_id);
