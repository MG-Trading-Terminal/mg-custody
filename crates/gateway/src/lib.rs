//! HTTP API gateway for the custody service.
//!
//! Internal-only — never exposed to the internet directly.
//! All requests authenticated via HMAC-SHA256.

pub mod admin_routes;
pub mod auth;
pub mod connectivity_routes;
pub mod exchange_routes;
pub mod errors;
pub mod health;
pub mod middleware;
pub mod ops_routes;
pub mod platform_routes;
pub mod rate_limit;
pub mod request_id;
pub mod routes;
pub mod settlement_routes;
pub mod staking_routes;
pub mod state;
pub mod vault_routes;
pub mod websocket;
