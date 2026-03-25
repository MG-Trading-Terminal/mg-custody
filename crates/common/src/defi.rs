//! DeFi access types.
//!
//! Types for WalletConnect integration, DEX swaps, and token allowance management.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A WalletConnect session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConnectSession {
    pub id: Uuid,
    pub topic: String,
    pub dapp_name: String,
    pub dapp_url: String,
    pub chain_id: u64,
    pub wallet_address: String,
    pub connected_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A DEX swap request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapRequest {
    pub id: Uuid,
    pub chain_id: u64,
    pub from_token: String,
    pub to_token: String,
    pub amount_in: Decimal,
    pub min_amount_out: Decimal,
    pub slippage_bps: u32, // basis points (e.g., 50 = 0.5%)
    pub dex_aggregator: String, // "1inch", "0x", "jupiter"
}

/// Status of a swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapStatus {
    Pending,
    Quoted,
    Signed,
    Broadcast,
    Completed,
    Failed,
}

/// ERC-20 token allowance (approval).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAllowance {
    pub chain_id: u64,
    pub token_address: String,
    pub token_symbol: String,
    pub spender_address: String,
    pub spender_name: Option<String>,
    pub allowance: Decimal, // amount approved, Decimal::MAX for unlimited
    pub is_unlimited: bool,
}

impl TokenAllowance {
    /// Check if this allowance is potentially risky (unlimited approval).
    pub fn is_risky(&self) -> bool {
        self.is_unlimited
    }
}

/// Maximum slippage for institutional swaps (2%).
pub const MAX_SLIPPAGE_BPS: u32 = 200;

/// Validate swap slippage.
pub fn validate_slippage(slippage_bps: u32) -> bool {
    slippage_bps <= MAX_SLIPPAGE_BPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn slippage_validation() {
        assert!(validate_slippage(50)); // 0.5%
        assert!(validate_slippage(200)); // 2.0%
        assert!(!validate_slippage(201)); // > 2.0%
        assert!(!validate_slippage(1000)); // 10%
    }

    #[test]
    fn unlimited_allowance_is_risky() {
        let allowance = TokenAllowance {
            chain_id: 1,
            token_address: "0x123".into(),
            token_symbol: "USDT".into(),
            spender_address: "0xDEX".into(),
            spender_name: Some("Uniswap".into()),
            allowance: Decimal::MAX,
            is_unlimited: true,
        };
        assert!(allowance.is_risky());
    }

    #[test]
    fn limited_allowance_not_risky() {
        let allowance = TokenAllowance {
            chain_id: 1,
            token_address: "0x123".into(),
            token_symbol: "USDT".into(),
            spender_address: "0xDEX".into(),
            spender_name: None,
            allowance: dec!(1000),
            is_unlimited: false,
        };
        assert!(!allowance.is_risky());
    }

    #[test]
    fn swap_status_serialization() {
        assert_eq!(serde_json::to_string(&SwapStatus::Completed).unwrap(), "\"completed\"");
    }
}
