-- Withdrawal dependency chain.
--
-- Many on-chain flows are multi-step: Lido unstake needs an stETH
-- `approve` before `requestWithdrawals` can pull the tokens; Uniswap
-- swaps need approve before swap; WalletConnect sessions can order
-- multiple calls. Before this column the withdrawal pipeline ran all
-- Validated rows in parallel, which broke any flow whose steps had
-- ordering requirements.
--
-- `depends_on_withdrawal_id` is the simplest primitive that fits:
-- a row with a non-NULL dep is held at Validated until the
-- dependency row hits Confirmed / Finalized. If the dep ends in
-- Failed, the dependent is marked Failed too — we never broadcast
-- step 2 when step 1 reverted.
--
-- NULL = no dep = behaves exactly as before. Existing rows stay NULL.

ALTER TABLE withdrawals
    ADD COLUMN IF NOT EXISTS depends_on_withdrawal_id UUID
        REFERENCES withdrawals(id);

CREATE INDEX IF NOT EXISTS idx_withdrawals_depends_on
    ON withdrawals (depends_on_withdrawal_id)
    WHERE depends_on_withdrawal_id IS NOT NULL;
