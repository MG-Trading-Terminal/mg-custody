//! Real-time event broadcasting.
//!
//! Supports both WebSocket subscriptions and webhook push.
//! Events are broadcast to all connected clients when deposits/withdrawals change state.

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::CustodyEvent;

/// Event broadcaster — sends events to all subscribers.
pub struct EventBroadcaster {
    sender: broadcast::Sender<CustodyEvent>,
}

impl EventBroadcaster {
    /// Create a new broadcaster with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Create with default capacity (1000 events).
    pub fn with_default_capacity() -> Self {
        Self::new(1000)
    }

    /// Broadcast an event to all subscribers.
    pub fn broadcast(&self, event: CustodyEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to events. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<CustodyEvent> {
        self.sender.subscribe()
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// Event filter for WebSocket subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Subscribe to specific event types (None = all).
    pub event_types: Option<Vec<String>>,
    /// Subscribe to specific chains (None = all).
    pub chains: Option<Vec<String>>,
    /// Subscribe to specific assets (None = all).
    pub assets: Option<Vec<String>>,
}

impl EventFilter {
    /// Match all events.
    pub fn all() -> Self {
        Self { event_types: None, chains: None, assets: None }
    }

    /// Check if an event matches this filter.
    pub fn matches(&self, event: &CustodyEvent) -> bool {
        // Type filter
        if let Some(ref types) = self.event_types {
            let event_type = match event {
                CustodyEvent::DepositConfirmed { .. } => "deposit_confirmed",
                CustodyEvent::WithdrawalBroadcast { .. } => "withdrawal_broadcast",
                CustodyEvent::WithdrawalConfirmed { .. } => "withdrawal_confirmed",
                CustodyEvent::WithdrawalFailed { .. } => "withdrawal_failed",
            };
            if !types.iter().any(|t| t == event_type) {
                return false;
            }
        }

        // Chain filter
        if let Some(ref chains) = self.chains {
            let chain = match event {
                CustodyEvent::DepositConfirmed { chain, .. } => chain.as_str(),
                CustodyEvent::WithdrawalBroadcast { chain, .. } => chain.as_str(),
                CustodyEvent::WithdrawalConfirmed { .. } => return true, // no chain in this variant
                CustodyEvent::WithdrawalFailed { .. } => return true,
            };
            if !chains.iter().any(|c| c == chain) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Asset, Chain};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[test]
    fn broadcaster_send_receive() {
        let broadcaster = EventBroadcaster::with_default_capacity();
        let mut rx = broadcaster.subscribe();

        let event = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::new_v4(),
            user_address: "0x123".into(),
            user_id: Uuid::new_v4(),
            asset: Asset::new("ETH"),
            chain: Chain::Ethereum,
            amount: dec!(1.5),
            tx_hash: "0xabc".into(),
            confirmations: 12,
            block_height: 1000,
        };

        broadcaster.broadcast(event.clone());

        let received = rx.try_recv().unwrap();
        match received {
            CustodyEvent::DepositConfirmed { confirmations, .. } => {
                assert_eq!(confirmations, 12);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn multiple_subscribers() {
        let broadcaster = EventBroadcaster::with_default_capacity();
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();

        assert_eq!(broadcaster.subscriber_count(), 2);

        let event = CustodyEvent::WithdrawalFailed {
            withdrawal_id: Uuid::new_v4(),
            exchange_withdrawal_id: Uuid::new_v4(),
            user_id: None,
            reason: "test".into(),
        };

        let count = broadcaster.broadcast(event);
        assert_eq!(count, 2);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn event_filter_all() {
        let filter = EventFilter::all();
        let event = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::new_v4(), user_address: "x".into(),
            user_id: Uuid::new_v4(), asset: Asset::new("ETH"),
            chain: Chain::Ethereum, amount: dec!(1), tx_hash: "x".into(),
            confirmations: 1, block_height: 1,
        };
        assert!(filter.matches(&event));
    }

    #[test]
    fn event_filter_by_type() {
        let filter = EventFilter {
            event_types: Some(vec!["deposit_confirmed".into()]),
            chains: None,
            assets: None,
        };

        let deposit = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::new_v4(), user_address: "x".into(),
            user_id: Uuid::new_v4(), asset: Asset::new("ETH"),
            chain: Chain::Ethereum, amount: dec!(1), tx_hash: "x".into(),
            confirmations: 1, block_height: 1,
        };
        assert!(filter.matches(&deposit));

        let withdrawal = CustodyEvent::WithdrawalFailed {
            withdrawal_id: Uuid::new_v4(),
            exchange_withdrawal_id: Uuid::new_v4(),
            user_id: None,
            reason: "test".into(),
        };
        assert!(!filter.matches(&withdrawal));
    }

    #[test]
    fn event_filter_by_chain() {
        let filter = EventFilter {
            event_types: None,
            chains: Some(vec!["ethereum".into()]),
            assets: None,
        };

        let eth_event = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::new_v4(), user_address: "x".into(),
            user_id: Uuid::new_v4(), asset: Asset::new("ETH"),
            chain: Chain::Ethereum, amount: dec!(1), tx_hash: "x".into(),
            confirmations: 1, block_height: 1,
        };
        assert!(filter.matches(&eth_event));

        let btc_event = CustodyEvent::DepositConfirmed {
            deposit_id: Uuid::new_v4(), user_address: "x".into(),
            user_id: Uuid::new_v4(), asset: Asset::new("BTC"),
            chain: Chain::Bitcoin, amount: dec!(1), tx_hash: "x".into(),
            confirmations: 1, block_height: 1,
        };
        assert!(!filter.matches(&btc_event));
    }
}
