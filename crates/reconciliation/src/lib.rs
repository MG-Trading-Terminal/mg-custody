//! Balance reconciliation and Proof of Reserves.
//!
//! Reconciles on-chain balances against internal ledger.
//! Merkle-tree Proof of Reserves for auditor verification.
//!
//! Hot wallets: every 10 minutes. Warm/cold: every 6 hours.
//! Alert on discrepancy > 0.001%.

pub mod balance_provider;
pub mod eth_balance;
pub mod reconciler;
pub mod proof_of_reserves;
pub mod service;

pub use balance_provider::BalanceProvider;
pub use reconciler::{ReconciliationResult, DiscrepancyAlert};
pub use proof_of_reserves::{MerkleProof, ReservesAttestation};
pub use service::ReconciliationService;
