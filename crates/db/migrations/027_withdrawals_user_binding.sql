-- Bind withdrawals to the initiating user and source vault.
-- Required for per-user authorization on cancel / accelerate / history.
-- Safe to apply to existing databases where 001_initial_schema.sql has
-- already run in its pre-binding shape.

ALTER TABLE withdrawals
    ADD COLUMN IF NOT EXISTS user_id  UUID,
    ADD COLUMN IF NOT EXISTS vault_id UUID;

-- Existing rows have no known owner. Park them on the nil UUID so the
-- NOT NULL constraint can be applied without a destructive drop.
UPDATE withdrawals SET user_id = '00000000-0000-0000-0000-000000000000'
 WHERE user_id IS NULL;

ALTER TABLE withdrawals
    ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_withdrawals_user_id  ON withdrawals (user_id);
CREATE INDEX IF NOT EXISTS idx_withdrawals_vault_id ON withdrawals (vault_id);
