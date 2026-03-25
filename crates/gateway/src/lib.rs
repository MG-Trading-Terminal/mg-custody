//! HTTP API gateway for the custody service.
//!
//! Internal-only — never exposed to the internet directly.
//! All requests authenticated via HMAC-SHA256.

pub mod admin_routes;
pub mod auth;
pub mod exchange_routes;
pub mod errors;
pub mod health;
pub mod middleware;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod websocket;
