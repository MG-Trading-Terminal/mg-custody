//! Graceful shutdown handling.
//!
//! On SIGTERM/SIGINT:
//! 1. Stop accepting new requests
//! 2. Wait for in-flight transactions to complete (up to timeout)
//! 3. Persist scanner progress
//! 4. Close DB connections
//! 5. Exit

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Tracks in-flight operations for graceful shutdown.
#[derive(Debug)]
pub struct ShutdownTracker {
    shutting_down: AtomicBool,
    in_flight: AtomicU64,
}

impl ShutdownTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            shutting_down: AtomicBool::new(false),
            in_flight: AtomicU64::new(0),
        })
    }

    /// Signal shutdown.
    pub fn signal_shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        tracing::info!(in_flight = self.in_flight(), "Shutdown signaled");
    }

    /// Check if shutdown is in progress.
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// Increment in-flight counter.
    pub fn enter(&self) -> Option<InFlightGuard> {
        if self.is_shutting_down() {
            return None; // Reject new operations during shutdown
        }
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        Some(InFlightGuard { tracker: self })
    }

    /// Current in-flight count.
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Wait for all in-flight operations to complete (with timeout).
    pub async fn wait_for_completion(&self, timeout: Duration) {
        let start = std::time::Instant::now();
        while self.in_flight() > 0 {
            if start.elapsed() > timeout {
                tracing::warn!(
                    remaining = self.in_flight(),
                    "Shutdown timeout — forcing exit with in-flight operations"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tracing::info!("All in-flight operations completed");
    }
}

impl Default for ShutdownTracker {
    fn default() -> Self {
        Self {
            shutting_down: AtomicBool::new(false),
            in_flight: AtomicU64::new(0),
        }
    }
}

/// RAII guard that decrements in-flight count on drop.
pub struct InFlightGuard<'a> {
    tracker: &'a ShutdownTracker,
}

impl<'a> Drop for InFlightGuard<'a> {
    fn drop(&mut self) {
        self.tracker.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Wait for SIGTERM or SIGINT signal.
pub async fn wait_for_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received SIGINT (Ctrl+C)"),
        _ = terminate => tracing::info!("Received SIGTERM"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_tracker_basic() {
        let tracker = ShutdownTracker::new();
        assert!(!tracker.is_shutting_down());
        assert_eq!(tracker.in_flight(), 0);

        let _guard = tracker.enter().unwrap();
        assert_eq!(tracker.in_flight(), 1);

        drop(_guard);
        assert_eq!(tracker.in_flight(), 0);
    }

    #[test]
    fn shutdown_rejects_new_operations() {
        let tracker = ShutdownTracker::new();
        tracker.signal_shutdown();

        assert!(tracker.is_shutting_down());
        assert!(tracker.enter().is_none()); // rejected
    }

    #[test]
    fn multiple_in_flight() {
        let tracker = ShutdownTracker::new();

        let g1 = tracker.enter().unwrap();
        let g2 = tracker.enter().unwrap();
        let g3 = tracker.enter().unwrap();
        assert_eq!(tracker.in_flight(), 3);

        drop(g1);
        assert_eq!(tracker.in_flight(), 2);

        drop(g2);
        drop(g3);
        assert_eq!(tracker.in_flight(), 0);
    }
}
