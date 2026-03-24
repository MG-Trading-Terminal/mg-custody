//! Balance reconciliation service.
//!
//! Runs every 10 minutes for hot wallets, every 6 hours for warm/cold.
//! Compares on-chain balances against internal ledger.
//! Alerts on discrepancy > 0.001%.
