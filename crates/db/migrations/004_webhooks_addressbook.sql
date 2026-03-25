-- Level 6: Webhook endpoints, address book.

CREATE TABLE IF NOT EXISTS webhook_endpoints (
    id UUID PRIMARY KEY,
    url TEXT NOT NULL,
    secret TEXT,
    event_filter TEXT[] NOT NULL DEFAULT '{}',
    chain_filter TEXT[] NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_enabled ON webhook_endpoints(enabled);

CREATE TABLE IF NOT EXISTS address_book (
    id UUID PRIMARY KEY,
    label TEXT NOT NULL,
    address TEXT NOT NULL,
    chain TEXT NOT NULL,
    tag TEXT,
    whitelisted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(address, chain)
);
CREATE INDEX IF NOT EXISTS idx_address_book_chain ON address_book(chain);
