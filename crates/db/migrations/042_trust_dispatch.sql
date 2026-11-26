ALTER TABLE travel_rule_data
    ADD COLUMN IF NOT EXISTS trust_parties JSONB,
    ADD COLUMN IF NOT EXISTS trust_record_id TEXT,
    ADD COLUMN IF NOT EXISTS trust_tx_hash TEXT,
    ADD COLUMN IF NOT EXISTS trust_attempted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_travel_rule_trust_pending
    ON travel_rule_data (created_at, id)
    WHERE trust_parties IS NOT NULL AND status IN ('required', 'dispatching');
