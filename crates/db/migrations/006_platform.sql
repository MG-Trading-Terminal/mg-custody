-- Level 8: Workspaces, users, travel rule data.

CREATE TABLE IF NOT EXISTS workspaces (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    environment TEXT NOT NULL DEFAULT 'sandbox',
    owner_email TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS workspace_users (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    email TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'viewer',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,
    UNIQUE(workspace_id, email)
);
CREATE INDEX IF NOT EXISTS idx_workspace_users_workspace ON workspace_users(workspace_id);

CREATE TABLE IF NOT EXISTS travel_rule_data (
    id UUID PRIMARY KEY,
    withdrawal_id UUID NOT NULL UNIQUE,
    originator JSONB NOT NULL,
    beneficiary JSONB NOT NULL,
    amount NUMERIC(38,18) NOT NULL,
    asset TEXT NOT NULL,
    chain TEXT NOT NULL,
    threshold_currency TEXT NOT NULL DEFAULT 'USD',
    threshold_amount NUMERIC(38,18) NOT NULL,
    status TEXT NOT NULL DEFAULT 'required',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_travel_rule_withdrawal ON travel_rule_data(withdrawal_id);
