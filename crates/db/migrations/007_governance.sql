-- Level 9: Vault policies, approval workflows, transaction notes.

CREATE TABLE IF NOT EXISTS vault_policies (
    id UUID PRIMARY KEY,
    vault_id UUID NOT NULL REFERENCES vaults(id),
    name TEXT NOT NULL,
    rules JSONB NOT NULL DEFAULT '[]',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_vault_policies_vault ON vault_policies(vault_id);

CREATE TABLE IF NOT EXISTS approval_workflows (
    id UUID PRIMARY KEY,
    withdrawal_id UUID NOT NULL,
    vault_id UUID NOT NULL,
    required_approvals INTEGER NOT NULL DEFAULT 1,
    current_approvals INTEGER NOT NULL DEFAULT 0,
    decisions JSONB NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_approval_workflows_withdrawal ON approval_workflows(withdrawal_id);
CREATE INDEX IF NOT EXISTS idx_approval_workflows_status ON approval_workflows(status);

CREATE TABLE IF NOT EXISTS transaction_notes (
    id UUID PRIMARY KEY,
    transaction_id UUID NOT NULL,
    note TEXT NOT NULL,
    author TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_transaction_notes_tx ON transaction_notes(transaction_id);
