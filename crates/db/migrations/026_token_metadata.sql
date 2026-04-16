-- Token metadata extension for scaling to 500+ tokens.
-- Adds fields needed for dashboard display, price tracking, and CoinGecko integration.

ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS display_name TEXT;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS coingecko_id TEXT;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS logo_url TEXT;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS last_price_usd NUMERIC;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS last_price_updated_at TIMESTAMPTZ;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS market_cap_usd NUMERIC;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS is_stablecoin BOOLEAN DEFAULT false;
ALTER TABLE managed_assets ADD COLUMN IF NOT EXISTS tags TEXT[] DEFAULT '{}';

-- Index for price staleness queries
CREATE INDEX IF NOT EXISTS idx_managed_assets_price_updated ON managed_assets (last_price_updated_at)
    WHERE last_price_updated_at IS NOT NULL;

-- Index for stablecoin filtering
CREATE INDEX IF NOT EXISTS idx_managed_assets_stablecoin ON managed_assets (is_stablecoin)
    WHERE is_stablecoin = true;
