//! HTTP API gateway for the custody service.
//!
//! Internal-only — never exposed to the internet directly.
//! All requests authenticated via HMAC-SHA256.

pub mod admin_routes;
pub mod nft_routes;
pub mod webhook_mgmt_routes;
pub mod compliance_routes;
pub mod auth;
pub mod push_routes;
pub mod auth_routes;
pub mod password_reset_routes;
pub mod totp_routes;
pub mod safe_routes;
pub mod openapi;
pub mod connectivity_routes;
pub mod embedded_routes;
pub mod exchange_routes;
pub mod fido2_routes;
pub mod errors;
pub mod governance_routes;
pub mod health;
pub mod middleware;
pub mod operations_routes;
pub mod ops_routes;
pub mod platform_routes;
pub mod rate_limit;
pub mod rbac_middleware;
pub mod request_id;
pub mod routes;
pub mod settlement_routes;
pub mod swap_routes;
pub mod staking_routes;
pub mod state;
pub mod user_routes;
pub mod vault_routes;
pub mod websocket;
