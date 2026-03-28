-- Migration 021: Add geolocation fields to login_history
--
-- Adds country_code and anomaly_flags columns to login_history.
-- country_code is populated by the geo_detection module on each login.
-- anomaly_flags stores a JSON array of detected anomalies (new_country, tor, vpn).
--
-- Migration is safe to run on existing data — columns default to NULL/empty.

ALTER TABLE login_history
    ADD COLUMN IF NOT EXISTS country_code  TEXT,
    ADD COLUMN IF NOT EXISTS is_tor        BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS is_vpn        BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS anomaly_flags JSONB;

-- Index for per-country login analysis (security reports)
CREATE INDEX IF NOT EXISTS idx_login_history_country
    ON login_history(user_id, country_code)
    WHERE country_code IS NOT NULL;
