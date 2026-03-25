//! Policy engine for the custody service.
//!
//! Implements Fireblocks TAP-like transaction authorization:
//! - Programmable rules (per-chain, per-asset, per-amount, per-destination)
//! - Top-down first-match evaluation (secure default: deny)
//! - Address whitelisting with expiry and one-time use
//! - Multi-approver workflows (M-of-N quorum)
//! - Role-based access control (Owner/Admin/Signer/Editor/Viewer)

pub mod approval;
pub mod engine;
pub mod rbac;
pub mod rule;
pub mod whitelist;

pub use approval::{ApprovalRequest, ApprovalDecision, ApprovalStatus};
pub use engine::{PolicyEngine, PolicyDecision};
pub use rbac::{Role, Permission};
pub use rule::{PolicyRule, PolicyCondition, PolicyAction, DestinationType};
pub use whitelist::{Whitelist, WhitelistEntry};
