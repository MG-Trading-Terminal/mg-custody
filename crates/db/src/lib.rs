//! PostgreSQL persistence layer for the custody service.
//!
//! Tables: addresses, deposits, withdrawals, utxos, custody_ledger,
//! scan_progress, webhook_deliveries, audit_log.

pub mod addresses;
pub mod audit;
pub mod custody_ledger;
pub mod deposits;
pub mod nonces;
pub mod policy;
pub mod scan_progress;
pub mod webhook_deliveries;
pub mod utxos;
pub mod withdrawals;

use sqlx::PgPool;

/// Database connection pool wrapper.
#[derive(Clone)]
pub struct CustodyDb {
    pub pool: PgPool,
}

impl CustodyDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
