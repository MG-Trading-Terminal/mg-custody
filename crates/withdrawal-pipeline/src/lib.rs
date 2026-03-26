//! Withdrawal processing pipeline.
//!
//! State machine: Received → Validated → [PendingApproval] → Signed → Broadcast → Confirming → Finalized
//!
//! Enforces:
//! - Address format validation
//! - Circuit breaker limits
//! - Hot wallet balance check
//! - Tiered approval for large amounts
//! - Time delays for Tier 2+

pub mod fee_bump;
pub mod ipc_signing;
pub mod nonce_manager;
pub mod pipeline;
pub mod validation;

pub use nonce_manager::NonceManager;
pub use pipeline::WithdrawalPipeline;
