-- Add signed transaction bytes storage to withdrawals
-- The signed raw tx is stored after signing so the broadcaster can retrieve and submit it.

ALTER TABLE withdrawals ADD COLUMN IF NOT EXISTS signed_tx_bytes BYTEA;
ALTER TABLE withdrawals ADD COLUMN IF NOT EXISTS nonce BIGINT;
ALTER TABLE withdrawals ADD COLUMN IF NOT EXISTS gas_price_gwei DECIMAL(38,18);
