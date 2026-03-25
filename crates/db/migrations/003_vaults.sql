-- Level 5: Vault/account abstraction, internal transfers, managed assets.

CREATE TABLE IF NOT EXISTS vaults (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    owner_id UUID NOT NULL,
    auto_fuel BOOLEAN NOT NULL DEFAULT false,
    hidden BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(owner_id, name)
);

CREATE TABLE IF NOT EXISTS vault_accounts (
    id UUID PRIMARY KEY,
    vault_id UUID NOT NULL REFERENCES vaults(id),
    asset TEXT NOT NULL,
    chain TEXT NOT NULL,
    address TEXT NOT NULL,
    derivation_path TEXT NOT NULL,
    balance NUMERIC(38,18) NOT NULL DEFAULT 0,
    pending_balance NUMERIC(38,18) NOT NULL DEFAULT 0,
    frozen BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(vault_id, asset, chain)
);
CREATE INDEX IF NOT EXISTS idx_vault_accounts_vault ON vault_accounts(vault_id);
CREATE INDEX IF NOT EXISTS idx_vault_accounts_address ON vault_accounts(address);

CREATE TABLE IF NOT EXISTS internal_transfers (
    id UUID PRIMARY KEY,
    source_vault_id UUID NOT NULL REFERENCES vaults(id),
    dest_vault_id UUID NOT NULL REFERENCES vaults(id),
    asset TEXT NOT NULL,
    chain TEXT NOT NULL,
    amount NUMERIC(38,18) NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    idempotency_key TEXT UNIQUE,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_internal_transfers_source ON internal_transfers(source_vault_id);
CREATE INDEX IF NOT EXISTS idx_internal_transfers_dest ON internal_transfers(dest_vault_id);

CREATE TABLE IF NOT EXISTS managed_assets (
    id UUID PRIMARY KEY,
    symbol TEXT NOT NULL,
    chain TEXT NOT NULL,
    contract_address TEXT,
    decimals SMALLINT NOT NULL,
    is_native BOOLEAN NOT NULL DEFAULT false,
    enabled BOOLEAN NOT NULL DEFAULT true,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(symbol, chain)
);
