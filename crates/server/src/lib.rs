//! Library surface for the custody server binary.
//!
//! The server is primarily a binary (`main.rs`), but a handful of
//! modules — currently just `readiness_build` — hold pure functions
//! that integration tests under `tests/` need to call. Those tests
//! compile as external crates, which cannot see `mod` declarations
//! inside `main.rs`, so we expose the testable modules here.
//!
//! `main.rs` still owns the tokio runtime, signal handling, and
//! background-task wiring — this lib stays deliberately thin.

pub mod readiness_build;
