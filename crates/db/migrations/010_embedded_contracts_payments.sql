-- Level 30: Embedded wallets, deployed contracts, payment flows.

CREATE TABLE IF NOT EXISTS embedded_wallets (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    wallet_type TEXT NOT NULL DEFAULT 'non_custodial',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_active_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_embedded_wallets_user ON embedded_wallets(user_id);

CREATE TABLE IF NOT EXISTS embedded_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id UUID NOT NULL REFERENCES embedded_wallets(id),
    chain TEXT NOT NULL,
    address TEXT NOT NULL,
    derivation_path TEXT NOT NULL,
    is_deployed BOOLEAN NOT NULL DEFAULT false,
    UNIQUE(wallet_id, chain)
);

CREATE TABLE IF NOT EXISTS deployed_contracts (
    id UUID PRIMARY KEY,
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    name TEXT,
    abi_hash TEXT,
    deployer_vault_id UUID,
    deploy_tx_hash TEXT,
    verified BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(address, chain)
);
CREATE INDEX IF NOT EXISTS idx_deployed_contracts_chain ON deployed_contracts(chain);

CREATE TABLE IF NOT EXISTS payment_flows (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    steps JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    executed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS transaction_labels (
    transaction_id UUID NOT NULL,
    labels TEXT[] NOT NULL DEFAULT '{}',
    department TEXT,
    cost_center TEXT,
    counterparty_name TEXT,
    purpose TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (transaction_id)
);
