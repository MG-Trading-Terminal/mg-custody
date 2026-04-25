-- Close the deposit double-credit window.
--
-- Before this migration, the confirmation tracker's order is:
--   1. credit_deposit  (writes ledger pairs for the deposit)
--   2. update_deposit_status -> Confirmed
-- A failure between 1 and 2 left the deposit in Detected status
-- but the ledger already credited. The next tracker tick re-ran
-- credit_deposit and added a SECOND set of ledger pairs — silent
-- double-credit until reconciliation caught it hours later.
--
-- The fix is twofold: this migration adds a partial UNIQUE index
-- on (reference_id, reference_type, entry_type) for the two
-- deposit-related reference types, and the writer uses
-- ON CONFLICT DO NOTHING. A retry after the failure window now
-- becomes a no-op insert, the deposit stays Detected, the next
-- tick re-tries the status flip without doubling the credit.
--
-- entry_type is part of the index because every deposit produces
-- two pairs of (debit, credit) rows that share reference_id +
-- reference_type but legitimately differ on entry_type. Without
-- it the SECOND insert of the FIRST call would already conflict.
--
-- The WHERE clause restricts the constraint to deposit
-- references — withdrawal_debit / withdrawal / sweep /
-- internal_transfer rows are unaffected (their idempotency story
-- is separate, deferred).

CREATE UNIQUE INDEX IF NOT EXISTS idx_custody_ledger_deposit_idem
    ON custody_ledger (reference_id, reference_type, entry_type)
    WHERE reference_type IN ('deposit', 'deposit_credit');
