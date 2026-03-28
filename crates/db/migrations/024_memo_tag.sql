-- Migration 024: memo and destination_tag support for withdrawals and deposits.
--
-- Adds optional memo and destination_tag columns required by Cosmos, Stellar,
-- and XRP-style networks that embed routing information in the transaction.
--
-- memo          — free-text memo string (max 256 bytes enforced at the app layer).
-- destination_tag — numeric routing tag (XRP-specific; stored as BIGINT to hold u64).
-- source_memo   — memo as parsed from the inbound on-chain transaction.

ALTER TABLE withdrawals ADD COLUMN IF NOT EXISTS memo TEXT;
ALTER TABLE withdrawals ADD COLUMN IF NOT EXISTS destination_tag BIGINT;

ALTER TABLE deposits ADD COLUMN IF NOT EXISTS memo TEXT;
ALTER TABLE deposits ADD COLUMN IF NOT EXISTS source_memo TEXT;
