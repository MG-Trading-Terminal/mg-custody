//! Application state shared across all route handlers.
//!
//! Contains all the service dependencies needed by the API handlers,
//! wrapped in Arc for shared ownership across async tasks.

use std::sync::Arc;

use custody_address_factory::{AddressAllocator, SeedBytes};
use custody_db::CustodyDb;
use custody_withdrawal_pipeline::WithdrawalPipeline;

/// Shared application state passed to all route handlers via axum's State.
#[derive(Clone)]
pub struct AppState {
    pub db: CustodyDb,
    pub address_allocator: Arc<AddressAllocator>,
    pub seed: Arc<SeedBytes>,
    pub withdrawal_pipeline: Arc<WithdrawalPipeline>,
}
