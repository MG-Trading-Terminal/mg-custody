-- Level 18: Address book label, notes, and last_used_at enrichment.
--
-- label was already present (NOT NULL). Adding notes and last_used_at
-- as nullable columns so existing rows are unaffected.

ALTER TABLE address_book ADD COLUMN IF NOT EXISTS notes        TEXT;
ALTER TABLE address_book ADD COLUMN IF NOT EXISTS last_used_at TIMESTAMPTZ;

-- Index to support fuzzy label search used by GET /v1/addresses/search
CREATE INDEX IF NOT EXISTS idx_address_book_label ON address_book USING gin(to_tsvector('simple', label));
