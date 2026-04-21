//! Generic confirmation tracker for deposits and withdrawals.
//!
//! Runs as a background loop, polling for pending deposits/withdrawals
//! and updating their confirmation counts based on current block heights.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use custody_common::{Chain, DepositStatus, WithdrawalStatus};
use custody_db::CustodyDb;

use crate::WebhookSender;

/// Function type for getting the current block height of a chain.
pub type BlockHeightFn = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<u64>> + Send>> + Send + Sync,
>;

/// Function type for looking up whether a tx hash succeeded on chain.
/// `Ok(Some(true))` = tx included and receipt reports success;
/// `Ok(Some(false))` = tx included but reverted / logic-failed;
/// `Ok(None)` = not yet visible on chain (treat as "keep waiting");
/// `Err(_)` = RPC down / transport issue (retry next tick).
///
/// The EVM revert case is what this hook exists for: previously the
/// confirmation tracker only checked block-count and flipped to
/// Finalized + fired `withdrawal_confirmed` for every tx that reached
/// `required_confirmations`. A reverted ERC-20 transfer (blacklisted
/// recipient, paused token, failing hook) still accrues confirmations
/// — so the custody service told the exchange the withdrawal landed
/// while the token never moved. Classic silent-failure pattern.
pub type TxSuccessFn = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<Option<bool>>> + Send>>
        + Send
        + Sync,
>;

/// Tracks confirmations for deposits and withdrawals across all chains.
pub struct ConfirmationTracker {
    db: CustodyDb,
    block_height_fns: HashMap<Chain, BlockHeightFn>,
    /// Per-chain hook to check whether a tx succeeded. Optional —
    /// chains without a hook fall back to the block-count-only path
    /// (same as before). Wiring this in main.rs for ETH is the
    /// minimum honest position; other chains can follow.
    tx_success_fns: HashMap<Chain, TxSuccessFn>,
    webhook_sender: Arc<WebhookSender>,
}

impl ConfirmationTracker {
    pub fn new(
        db: CustodyDb,
        block_height_fns: HashMap<Chain, BlockHeightFn>,
        webhook_sender: Arc<WebhookSender>,
    ) -> Self {
        Self {
            db,
            block_height_fns,
            tx_success_fns: HashMap::new(),
            webhook_sender,
        }
    }

    /// Attach a per-chain tx-success hook. See `TxSuccessFn` docs for
    /// the semantics this closure must respect.
    pub fn with_tx_success_fn(mut self, chain: Chain, f: TxSuccessFn) -> Self {
        self.tx_success_fns.insert(chain, f);
        self
    }

    /// Run one tick: check all pending deposits/withdrawals and update confirmations.
    pub async fn tick(&self) -> Result<()> {
        self.check_deposits().await?;
        self.check_withdrawals().await?;
        Ok(())
    }

    /// Run the tracker loop with a configurable interval.
    pub async fn run(&self, interval: Duration) -> Result<()> {
        loop {
            if let Err(e) = self.tick().await {
                tracing::error!(error = %e, "Confirmation tracker tick failed");
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn check_deposits(&self) -> Result<()> {
        let pending = self.db.deposits_by_status(DepositStatus::Detected).await?;

        for deposit in pending {
            let Some(height_fn) = self.block_height_fns.get(&deposit.chain) else {
                continue;
            };

            let current_height = match height_fn().await {
                Ok(h) => h,
                Err(e) => {
                    tracing::debug!(chain = ?deposit.chain, error = %e, "Block height unavailable, skipping");
                    continue;
                }
            };
            let confirmations = current_height.saturating_sub(deposit.block_height);

            if confirmations >= deposit.required_confirmations {
                // Transition to Confirmed
                self.db
                    .update_deposit_status(deposit.id, DepositStatus::Confirmed, confirmations)
                    .await?;

                // Credit deposit in the double-entry ledger
                let _ = self.db.credit_deposit(
                    deposit.id,
                    deposit.asset.as_str(),
                    deposit.chain,
                    deposit.amount,
                    deposit.user_id,
                ).await;

                // Fire webhook
                let event = custody_common::CustodyEvent::DepositConfirmed {
                    deposit_id: deposit.id,
                    user_address: deposit.address.clone(),
                    user_id: deposit.user_id,
                    asset: deposit.asset.clone(),
                    chain: deposit.chain,
                    amount: deposit.amount,
                    tx_hash: deposit.tx_hash.clone(),
                    confirmations,
                    block_height: deposit.block_height,
                };

                if let Err(e) = self.webhook_sender.send(&event).await {
                    tracing::error!(
                        deposit_id = %deposit.id,
                        error = %e,
                        "Failed to send deposit_confirmed webhook"
                    );
                }

                tracing::info!(
                    deposit_id = %deposit.id,
                    confirmations,
                    "Deposit confirmed"
                );
            } else {
                // Update confirmation count
                self.db
                    .update_deposit_status(deposit.id, DepositStatus::Detected, confirmations)
                    .await?;
            }
        }

        Ok(())
    }

    async fn check_withdrawals(&self) -> Result<()> {
        let confirming = self
            .db
            .withdrawals_by_status(WithdrawalStatus::Confirming)
            .await?;

        for withdrawal in confirming {
            let Some(height_fn) = self.block_height_fns.get(&withdrawal.chain) else {
                continue;
            };

            let current_height = match height_fn().await {
                Ok(h) => h,
                Err(_) => continue, // RPC down, skip this chain
            };
            // If the withdrawal row has no block_height yet, the
            // broadcaster hasn't persisted the mined location —
            // either the tx is still pending, or a DB write is
            // mid-flight. Previously we used `unwrap_or(0)`,
            // which made `confirmations = current_height - 0 =
            // current_height` (a huge number), promoted the row
            // straight to Finalized, and fired a
            // withdrawal_confirmed webhook for a tx whose real
            // on-chain position we didn't know. Skip this tick
            // and let the next one pick up the real value.
            let Some(block_height) = withdrawal.block_height else {
                tracing::debug!(
                    withdrawal_id = %withdrawal.id,
                    chain = ?withdrawal.chain,
                    "Confirming withdrawal without block_height — skipping \
                     tick; broadcaster hasn't persisted the mined location"
                );
                continue;
            };
            let confirmations = current_height.saturating_sub(block_height);
            let required = withdrawal.chain.required_confirmations();

            if confirmations >= required {
                // Before marking Finalized: if a tx-success hook is
                // wired for this chain, ask on-chain whether the tx
                // actually succeeded. A reverted ERC-20 transfer
                // still accumulates block confirmations but the
                // token never moved; flipping to Finalized + sending
                // `withdrawal_confirmed` would tell the exchange a
                // credit happened that didn't.
                let tx_hash = withdrawal.tx_hash.clone().unwrap_or_default();
                if let Some(tx_success) = self.tx_success_fns.get(&withdrawal.chain)
                    && !tx_hash.is_empty()
                {
                    match tx_success(tx_hash.clone()).await {
                        Ok(Some(false)) => {
                            tracing::error!(
                                withdrawal_id = %withdrawal.id,
                                tx_hash = %tx_hash,
                                chain = ?withdrawal.chain,
                                "Withdrawal tx reverted / failed on-chain — \
                                 marking Failed instead of Finalized; NOT sending \
                                 withdrawal_confirmed webhook"
                            );
                            self.db
                                .update_withdrawal_status(withdrawal.id, WithdrawalStatus::Failed)
                                .await?;
                            continue;
                        }
                        Ok(None) => {
                            // Receipt not yet visible even though
                            // block_height implies it should be.
                            // Common during a short reorg; wait
                            // another tick.
                            tracing::debug!(
                                withdrawal_id = %withdrawal.id,
                                tx_hash = %tx_hash,
                                "tx receipt not yet visible despite confirmations — skipping tick"
                            );
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!(
                                withdrawal_id = %withdrawal.id,
                                error = %e,
                                "tx-success lookup failed; NOT finalising yet, will retry"
                            );
                            continue;
                        }
                        Ok(Some(true)) => {} // happy path, fall through
                    }
                }

                self.db
                    .update_withdrawal_status(withdrawal.id, WithdrawalStatus::Finalized)
                    .await?;

                let event = custody_common::CustodyEvent::WithdrawalConfirmed {
                    withdrawal_id: withdrawal.id,
                    exchange_withdrawal_id: withdrawal.exchange_withdrawal_id,
                    user_id: Some(withdrawal.user_id),
                    tx_hash: withdrawal.tx_hash.unwrap_or_default(),
                    block_height,
                    confirmations,
                };

                if let Err(e) = self.webhook_sender.send(&event).await {
                    tracing::error!(
                        withdrawal_id = %withdrawal.id,
                        error = %e,
                        "Failed to send withdrawal_confirmed webhook"
                    );
                }

                tracing::info!(
                    withdrawal_id = %withdrawal.id,
                    confirmations,
                    "Withdrawal finalized"
                );
            }
        }

        Ok(())
    }
}

/// Calculate the number of confirmations for a transaction.
pub fn confirmation_count(current_block: u64, tx_block: u64) -> u64 {
    current_block.saturating_sub(tx_block)
}

/// Check if a transaction has enough confirmations.
pub fn is_confirmed(current_block: u64, tx_block: u64, required: u64) -> bool {
    confirmation_count(current_block, tx_block) >= required
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_count_basic() {
        assert_eq!(confirmation_count(100, 88), 12);
        assert_eq!(confirmation_count(100, 100), 0);
        assert_eq!(confirmation_count(100, 101), 0); // future block → 0 (saturating)
    }

    #[test]
    fn is_confirmed_ethereum() {
        // Ethereum requires 12 confirmations
        assert!(!is_confirmed(99, 88, 12)); // 11 confirms
        assert!(is_confirmed(100, 88, 12));  // 12 confirms
        assert!(is_confirmed(200, 88, 12));  // 112 confirms
    }

    #[test]
    fn is_confirmed_bitcoin() {
        // Bitcoin requires 3 confirmations
        assert!(!is_confirmed(10, 8, 3)); // 2 confirms
        assert!(is_confirmed(11, 8, 3));   // 3 confirms
    }

    #[test]
    fn is_confirmed_at_exact_threshold() {
        assert!(is_confirmed(12, 0, 12));  // exactly 12
        assert!(!is_confirmed(11, 0, 12)); // 11 — not enough
    }
}
