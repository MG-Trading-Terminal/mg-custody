//! Multi-vault architecture.
//!
//! Logical vault separation: trading, treasury, user deposits.
//! Each vault has its own policies, limits, and address space.

use custody_common::Chain;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A logical vault (isolated fund container).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub id: Uuid,
    pub name: String,
    pub vault_type: VaultType,
    pub chains: Vec<Chain>,
    /// Per-asset balance tracking.
    pub balances: std::collections::HashMap<String, Decimal>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Vault types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultType {
    /// User deposit addresses — one per user+asset+chain.
    UserDeposits,
    /// Hot wallet for immediate withdrawals.
    Trading,
    /// Warm/cold treasury storage.
    Treasury,
    /// Fee collection vault.
    Fees,
    /// Staking vault.
    Staking,
}

/// Inter-vault transfer request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultTransfer {
    pub id: Uuid,
    pub from_vault: Uuid,
    pub to_vault: Uuid,
    pub asset: String,
    pub chain: Chain,
    pub amount: Decimal,
    pub status: VaultTransferStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultTransferStatus {
    Pending,
    Approved,
    Executed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_types() {
        assert_eq!(
            serde_json::to_string(&VaultType::Trading).unwrap(),
            "\"trading\""
        );
    }

    #[test]
    fn vault_transfer_status() {
        assert_eq!(
            serde_json::to_string(&VaultTransferStatus::Approved).unwrap(),
            "\"approved\""
        );
    }
}
