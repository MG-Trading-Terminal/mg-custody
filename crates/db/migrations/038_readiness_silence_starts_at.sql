-- Scheduled maintenance windows for readiness silences.
--
-- Until now a silence was always "from now until expires_at". That's
-- fine for firefighting but operators plan maintenance ahead of
-- time: "Saturday 02:00 UTC to 04:00 UTC, during which vendor X is
-- taking a downtime" doesn't fit a start-now model — pre-creating
-- those silences an hour before would still alert during the hour
-- before the window.
--
-- `starts_at` defaults to `clock_timestamp()` so existing rows and
-- callers that don't specify a start keep working as "silence from
-- right now". Matching logic in the refresh task filters
-- `starts_at <= now() AND expires_at > now()`.

ALTER TABLE readiness_silences
    ADD COLUMN IF NOT EXISTS starts_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp();

-- The "active now" index now needs both bounds. Drop the old one
-- and recreate with the scheduled-start constraint included. A
-- partial index on (expires_at > now()) alone was sufficient before
-- but misses pre-scheduled rows that haven't started yet.
DROP INDEX IF EXISTS idx_readiness_silences_feature_expires;
CREATE INDEX IF NOT EXISTS idx_readiness_silences_active
    ON readiness_silences (feature, starts_at, expires_at)
    WHERE expires_at > clock_timestamp();
