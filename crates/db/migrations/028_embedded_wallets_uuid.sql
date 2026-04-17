-- Migrate embedded_wallets.user_id from TEXT to UUID.
-- Rows whose existing TEXT value isn't a valid UUID are parked on the nil
-- UUID (system / unassigned) rather than dropped — this is a dev-box
-- migration; in production the operator would run a one-off import to
-- attach workspace_users ids before applying this file.

ALTER TABLE embedded_wallets
    ADD COLUMN user_id_uuid UUID;

-- Best-effort copy: cast any valid UUID strings over. Invalid values
-- land on the nil UUID and can be re-parented manually later.
UPDATE embedded_wallets SET user_id_uuid =
    CASE
        WHEN user_id ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
            THEN user_id::uuid
        ELSE '00000000-0000-0000-0000-000000000000'::uuid
    END;

ALTER TABLE embedded_wallets
    DROP COLUMN user_id;

ALTER TABLE embedded_wallets
    RENAME COLUMN user_id_uuid TO user_id;

ALTER TABLE embedded_wallets
    ALTER COLUMN user_id SET NOT NULL;

DROP INDEX IF EXISTS idx_embedded_wallets_user;
CREATE INDEX idx_embedded_wallets_user ON embedded_wallets (user_id);
