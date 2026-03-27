//! PostgreSQL persistence layer for the custody service.
//!
//! Tables: addresses, deposits, withdrawals, utxos, custody_ledger,
//! scan_progress, webhook_deliveries, audit_log.

pub mod address_book;
pub mod addresses;
pub mod api_keys;
pub mod assets;
pub mod audit;
pub mod circuit_breaker;
pub mod connections;
pub mod custody_ledger;
pub mod deployed_contracts;
pub mod deposits;
pub mod embedded_wallets;
pub mod governance;
pub mod nonces;
pub mod notification_channels;
pub mod policy;
pub mod scan_progress;
pub mod scheduled_tx;
pub mod transactions;
pub mod travel_rule;
pub mod vaults;
pub mod workspaces;
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
