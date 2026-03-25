//! Tokenization types (Phase 14).
//!
//! NFT/RWA custody, token minting, smart contract lifecycle management.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Token standard types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenStandard {
    Erc20,
    Erc721,
    Erc1155,
    Spl,
    Trc20,
}

/// A managed token (deployed from custody).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedToken {
    pub id: Uuid,
    pub standard: TokenStandard,
    pub chain_id: u64,
    pub contract_address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Decimal,
    pub deployed_at: DateTime<Utc>,
    pub owner_vault: String,
}

/// Token lifecycle operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenOperation {
    Mint,
    Burn,
    Pause,
    Unpause,
    TransferOwnership,
    Freeze,  // freeze specific address
    Unfreeze,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_standard_serialization() {
        assert_eq!(serde_json::to_string(&TokenStandard::Erc20).unwrap(), "\"erc20\"");
        assert_eq!(serde_json::to_string(&TokenStandard::Erc721).unwrap(), "\"erc721\"");
    }

    #[test]
    fn token_operations() {
        let ops = vec![TokenOperation::Mint, TokenOperation::Burn, TokenOperation::Pause];
        assert_eq!(ops.len(), 3);
    }
}
