-- Readiness alert silences.
--
-- Operators frequently need to suppress readiness alerts during
-- planned maintenance (upgrading a node, rotating an HSM pin,
-- migrating a vendor). Without a silence mechanism they either:
--   (a) spam oncall with expected alerts, training everyone to
--       ignore the channel, or
--   (b) comment out the probe, forgetting to re-enable it.
--
-- A silence is a promise "this feature will look degraded for a
-- while, don't page me about it". It does NOT affect the live
-- readiness state — handlers still gate on feature_ready, so a
-- silenced + actually-degraded feature still returns 503 to callers.
-- The silence only muzzles the webhook + log event.
--
-- Scope is per-exact-feature. Glob patterns ("push.*") are a
-- straightforward follow-up; keeping it narrow means the matching
-- logic is an indexed lookup.

CREATE TABLE IF NOT EXISTS readiness_silences (
    id BIGSERIAL PRIMARY KEY,
    -- Exact feature name (e.g. "scanner.ethereum"). Case-sensitive
    -- to match the readiness report.
    feature TEXT NOT NULL,
    -- When the silence auto-expires. Always required — no "forever"
    -- silences, an operator would forget they existed.
    expires_at TIMESTAMPTZ NOT NULL,
    -- Free-form justification logged in the audit trail.
    reason TEXT NOT NULL,
    -- The user/session that created the silence. Populated by the
    -- handler from RequireAdmin's AuthContext.
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

-- Lookup is "is feature X silenced right now?", so index both
-- columns together with expiry trailing for the most common query.
CREATE INDEX IF NOT EXISTS idx_readiness_silences_feature_expires
    ON readiness_silences (feature, expires_at)
    WHERE expires_at > clock_timestamp();

-- Dashboard's "currently silenced" query wants everything still live.
CREATE INDEX IF NOT EXISTS idx_readiness_silences_expires_desc
    ON readiness_silences (expires_at DESC)
    WHERE expires_at > clock_timestamp();
