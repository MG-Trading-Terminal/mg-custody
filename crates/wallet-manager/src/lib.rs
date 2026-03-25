//! Hot/warm/cold wallet tier management.
//!
//! Manages fund distribution across wallet tiers:
//! - Hot: max 5% AUM — immediate withdrawals, auto-refilled from warm
//! - Warm: ~15% AUM — MPC-signed sweeps to/from hot
//! - Cold: ~80% AUM — multi-party ceremony for access
//!
//! Generates sweep recommendations when thresholds are exceeded.
//! Also implements a Gas Station for auto-funding deposit addresses.

pub mod gas_station;
pub mod gas_station_loop;
pub mod sweep;
pub mod sweep_scheduler;
pub mod tiers;
pub mod vault;

pub use tiers::{TierConfig, TierStatus, WalletTierManager};
pub use sweep::{SweepRecommendation, SweepDirection};
pub use gas_station::{GasStationConfig, GasStation};
