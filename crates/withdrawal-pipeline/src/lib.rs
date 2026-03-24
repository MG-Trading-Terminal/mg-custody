//! Withdrawal processing pipeline.
//!
//! State machine: Received -> Validated -> [PendingApproval] -> Signed -> Broadcast -> Confirming -> Finalized
//!
//! Enforces:
//! - Address format validation
//! - Circuit breaker limits
//! - Hot wallet balance check
//! - Tiered approval for large amounts
//! - Time delays for Tier 2+
