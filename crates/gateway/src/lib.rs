//! HTTP API gateway for the custody service.
//!
//! Internal-only — never exposed to the internet directly.
//! All requests authenticated via HMAC-SHA256.

pub mod auth;
pub mod routes;
pub mod state;
