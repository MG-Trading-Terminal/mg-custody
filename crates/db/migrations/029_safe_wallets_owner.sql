-- Record which workspace user registered each Gnosis Safe.
-- Used for ownership scoping on propose / sign / execute — prevents a
-- user in one workspace from interacting with another workspace's Safe.

ALTER TABLE safe_wallets
    ADD COLUMN IF NOT EXISTS registered_by UUID;

CREATE INDEX IF NOT EXISTS idx_safe_wallets_registered_by
    ON safe_wallets (registered_by);
