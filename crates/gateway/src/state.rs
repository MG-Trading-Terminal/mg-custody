//! Application state shared across all route handlers.
//!
//! Contains all the service dependencies needed by the API handlers,
//! wrapped in Arc for shared ownership across async tasks.

use std::sync::Arc;

use custody_address_factory::{AddressAllocator, SeedBytes};
use custody_common::email::EmailSender;
use custody_common::emergency::FreezeFlag;
use custody_common::events::EventBroadcaster;
use custody_db::CustodyDb;
use custody_withdrawal_pipeline::WithdrawalPipeline;

use crate::rate_limit::UserRateLimiter;

/// Shared application state passed to all route handlers via axum's State.
#[derive(Clone)]
pub struct AppState {
    pub db: CustodyDb,
    pub address_allocator: Arc<AddressAllocator>,
    pub seed: Arc<SeedBytes>,
    pub withdrawal_pipeline: Arc<WithdrawalPipeline>,
    pub event_broadcaster: Arc<EventBroadcaster>,
    /// Per-user sliding window rate limiter (shared across all request handlers).
    pub user_rate_limiter: Arc<UserRateLimiter>,
    /// Emergency freeze flag — when true all outbound operations are halted.
    pub frozen: FreezeFlag,
    /// Outbound email transport. Console-printing in dev (`console`), real
    /// SMTP when wired. Never dispatch email via raw `tracing` — use this.
    pub email: Arc<EmailSender>,
}
