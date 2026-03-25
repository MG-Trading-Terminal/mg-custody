//! PostgreSQL persistence layer for the custody service.
//!
//! Tables: addresses, deposits, withdrawals, utxos, custody_ledger,
//! scan_progress, webhook_deliveries, audit_log.

pub mod address_book;
pub mod addresses;
pub mod api_keys;
pub mod assets;
pub mod audit;
pub mod connections;
pub mod custody_ledger;
pub mod deposits;
pub mod nonces;
pub mod notification_channels;
pub mod policy;
pub mod scan_progress;
pub mod transactions;
pub mod vaults;
pub mod webhook_deliveries;
pub mod webhook_endpoints;
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
