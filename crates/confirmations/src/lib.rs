//! Confirmation tracking and webhook dispatch.
//!
//! Monitors pending deposits until they reach required confirmations,
//! then sends a webhook to the exchange.
//!
//! Also tracks withdrawals in `Confirming` status.

pub mod tracker;
pub mod webhook;

pub use tracker::ConfirmationTracker;
pub use webhook::WebhookSender;
