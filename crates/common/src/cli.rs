//! CLI tool types and helpers for key ceremonies, wallet info, address derivation.
//!
//! These types support offline CLI operations (no DB needed).

use serde::{Deserialize, Serialize};
use crate::Chain;

// ---------------------------------------------------------------------------
// Key ceremony
// ---------------------------------------------------------------------------

/// Key ceremony step for Shamir backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyStep {
    pub step_number: u32,
    pub total_steps: u32,
    pub action: CeremonyAction,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeremonyAction {
    GenerateSeed,
    EncryptSeed,
    SplitShares { threshold: u8, total: u8 },
    DistributeShare { share_index: u8, custodian: String },
    VerifyReconstruction,
    SecureDelete,
}

/// Generate the key ceremony steps for a 3-of-5 Shamir backup.
pub fn key_ceremony_steps(custodians: &[String]) -> Vec<CeremonyStep> {
    let total = 3 + custodians.len() as u32 + 2; // gen/encrypt/split + distribute + verify/delete
    let mut steps = vec![
        CeremonyStep {
            step_number: 1, total_steps: total,
            action: CeremonyAction::GenerateSeed, completed: false,
        },
        CeremonyStep {
            step_number: 2, total_steps: total,
            action: CeremonyAction::EncryptSeed, completed: false,
        },
        CeremonyStep {
            step_number: 3, total_steps: total,
            action: CeremonyAction::SplitShares { threshold: 3, total: 5 },
            completed: false,
        },
    ];

    for (i, custodian) in custodians.iter().enumerate() {
        steps.push(CeremonyStep {
            step_number: 4 + i as u32, total_steps: total,
            action: CeremonyAction::DistributeShare {
                share_index: i as u8,
                custodian: custodian.clone(),
            },
            completed: false,
        });
    }

    steps.push(CeremonyStep {
        step_number: total - 1, total_steps: total,
        action: CeremonyAction::VerifyReconstruction, completed: false,
    });
    steps.push(CeremonyStep {
        step_number: total, total_steps: total,
        action: CeremonyAction::SecureDelete, completed: false,
    });

    steps
}

// ---------------------------------------------------------------------------
// Wallet info
// ---------------------------------------------------------------------------

/// Summary information about wallet state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub chain: Chain,
    pub address: String,
    pub derivation_path: String,
    pub key_type: String,
    pub algorithm: String,
}

/// Generate wallet info for all 4 chains at index 0.
pub fn default_wallet_info() -> Vec<WalletInfo> {
    vec![
        WalletInfo {
            chain: Chain::Bitcoin,
            address: "(derived at runtime)".into(),
            derivation_path: "m/84'/0'/0'/0/0".into(),
            key_type: "P2WPKH".into(),
            algorithm: "secp256k1".into(),
        },
        WalletInfo {
            chain: Chain::Ethereum,
            address: "(derived at runtime)".into(),
            derivation_path: "m/44'/60'/0'/0/0".into(),
            key_type: "EOA".into(),
            algorithm: "secp256k1".into(),
        },
        WalletInfo {
            chain: Chain::Tron,
            address: "(derived at runtime)".into(),
            derivation_path: "m/44'/195'/0'/0/0".into(),
            key_type: "Base58Check".into(),
            algorithm: "secp256k1".into(),
        },
        WalletInfo {
            chain: Chain::Solana,
            address: "(derived at runtime)".into(),
            derivation_path: "m/44'/501'/0'/0'".into(),
            key_type: "Ed25519".into(),
            algorithm: "ed25519".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// Integration test helpers
// ---------------------------------------------------------------------------

/// Test fixture for creating a mock chain environment.
#[derive(Debug, Clone)]
pub struct TestFixture {
    pub chain: Chain,
    pub test_address: String,
    pub test_tx_hash: String,
    pub test_block_height: u64,
}

impl TestFixture {
    pub fn ethereum() -> Self {
        Self {
            chain: Chain::Ethereum,
            test_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18".into(),
            test_tx_hash: "0xabc123def456789012345678901234567890123456789012345678901234abcd".into(),
            test_block_height: 19_500_000,
        }
    }

    pub fn bitcoin() -> Self {
        Self {
            chain: Chain::Bitcoin,
            test_address: "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu".into(),
            test_tx_hash: "abc123def456789012345678901234567890123456789012345678901234abcd".into(),
            test_block_height: 840_000,
        }
    }

    pub fn tron() -> Self {
        Self {
            chain: Chain::Tron,
            test_address: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".into(),
            test_tx_hash: "abc123def456".into(),
            test_block_height: 60_000_000,
        }
    }

    pub fn solana() -> Self {
        Self {
            chain: Chain::Solana,
            test_address: "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".into(),
            test_tx_hash: "5UfDuPMqLm3yFZPNjfWkqSVkdP1c5ov24".into(),
            test_block_height: 250_000_000,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::ethereum(), Self::bitcoin(), Self::tron(), Self::solana()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_ceremony_steps_count() {
        let custodians = vec![
            "Alice".into(), "Bob".into(), "Charlie".into(),
            "Dave".into(), "Eve".into(),
        ];
        let steps = key_ceremony_steps(&custodians);
        // 3 (gen/encrypt/split) + 5 (distribute) + 2 (verify/delete) = 10
        assert_eq!(steps.len(), 10);
        assert_eq!(steps.first().unwrap().step_number, 1);
        assert_eq!(steps.last().unwrap().step_number, 10);
    }

    #[test]
    fn key_ceremony_none_completed() {
        let steps = key_ceremony_steps(&["A".into(), "B".into(), "C".into()]);
        assert!(steps.iter().all(|s| !s.completed));
    }

    #[test]
    fn default_wallet_info_all_chains() {
        let info = default_wallet_info();
        assert_eq!(info.len(), 4);
        assert_eq!(info[0].chain, Chain::Bitcoin);
        assert_eq!(info[1].chain, Chain::Ethereum);
        assert_eq!(info[2].chain, Chain::Tron);
        assert_eq!(info[3].chain, Chain::Solana);
    }

    #[test]
    fn wallet_derivation_paths() {
        let info = default_wallet_info();
        assert!(info[0].derivation_path.contains("84'")); // BIP-84 for BTC
        assert!(info[1].derivation_path.contains("60'")); // ETH coin type
        assert!(info[2].derivation_path.contains("195'")); // TRX coin type
        assert!(info[3].derivation_path.contains("501'")); // SOL coin type
    }

    #[test]
    fn test_fixtures_all_chains() {
        let fixtures = TestFixture::all();
        assert_eq!(fixtures.len(), 4);
        assert_eq!(fixtures[0].chain, Chain::Ethereum);
        assert!(fixtures[0].test_address.starts_with("0x"));
        assert!(fixtures[1].test_address.starts_with("bc1"));
        assert!(fixtures[2].test_address.starts_with("T"));
    }

    #[test]
    fn ceremony_action_serialization() {
        let action = CeremonyAction::SplitShares { threshold: 3, total: 5 };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("split_shares"));
        assert!(json.contains("\"threshold\":3"));
    }
}
