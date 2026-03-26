-- Level 10: Scheduled transactions, compliance screenings (extended).

CREATE TABLE IF NOT EXISTS scheduled_transactions (
    id UUID PRIMARY KEY,
    vault_id UUID NOT NULL,
    asset TEXT NOT NULL,
    chain TEXT NOT NULL,
    destination_address TEXT NOT NULL,
    amount NUMERIC(38,18) NOT NULL,
    scheduled_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    withdrawal_id UUID,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    executed_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_scheduled_tx_status ON scheduled_transactions(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_scheduled_tx_vault ON scheduled_transactions(vault_id);
