//! Merkle-tree Proof of Reserves.
//!
//! Creates a Merkle tree of all user balances. Users can verify their
//! inclusion without seeing other users' balances.
//!
//! Reference: <https://vitalik.eth.limo/general/2022/11/19/proof_of_solvency.html>

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A leaf in the Merkle tree (one user's balance for one asset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceLeaf {
    pub user_id: Uuid,
    pub asset: String,
    pub balance: Decimal,
    /// Hash of this leaf (SHA-256 of user_id + asset + balance).
    pub hash: String,
}

impl BalanceLeaf {
    pub fn new(user_id: Uuid, asset: &str, balance: Decimal) -> Self {
        let data = format!("{}:{}:{}", user_id, asset, balance);
        let hash = hex::encode(Sha256::digest(data.as_bytes()));
        Self {
            user_id,
            asset: asset.to_string(),
            balance,
            hash,
        }
    }
}

/// A Merkle proof for a user's inclusion in the reserves tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: BalanceLeaf,
    /// Sibling hashes from leaf to root.
    pub siblings: Vec<String>,
    /// Direction of each sibling (true = right, false = left).
    pub directions: Vec<bool>,
    pub root_hash: String,
}

impl MerkleProof {
    /// Verify this proof against a known root hash.
    pub fn verify(&self) -> bool {
        let mut current = self.leaf.hash.clone();
        for (sibling, is_right) in self.siblings.iter().zip(&self.directions) {
            let combined = if *is_right {
                format!("{}{}", current, sibling)
            } else {
                format!("{}{}", sibling, current)
            };
            current = hex::encode(Sha256::digest(combined.as_bytes()));
        }
        current == self.root_hash
    }
}

/// A Proof of Reserves attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservesAttestation {
    pub id: Uuid,
    /// Merkle root of all user balances.
    pub merkle_root: String,
    /// Total reserves per asset.
    pub total_reserves: Vec<AssetReserve>,
    /// On-chain addresses holding the reserves.
    pub reserve_addresses: Vec<ReserveAddress>,
    pub attestation_time: DateTime<Utc>,
    pub block_heights: Vec<(String, u64)>, // (chain, block_height) at attestation time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetReserve {
    pub asset: String,
    pub total_user_balances: Decimal,
    pub total_on_chain: Decimal,
    pub ratio: Decimal, // on_chain / user_balances (>= 1.0 means fully reserved)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveAddress {
    pub chain: String,
    pub address: String,
    pub balance: Decimal,
    pub tier: String, // "hot", "warm", "cold"
}

/// Build a simple Merkle tree from leaves and return the root hash.
pub fn merkle_root(leaves: &[BalanceLeaf]) -> String {
    if leaves.is_empty() {
        return hex::encode(Sha256::digest(b"empty"));
    }

    let mut hashes: Vec<String> = leaves.iter().map(|l| l.hash.clone()).collect();

    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        for chunk in hashes.chunks(2) {
            let combined = if chunk.len() == 2 {
                format!("{}{}", chunk[0], chunk[1])
            } else {
                format!("{}{}", chunk[0], chunk[0]) // duplicate odd leaf
            };
            next_level.push(hex::encode(Sha256::digest(combined.as_bytes())));
        }
        hashes = next_level;
    }

    hashes.into_iter().next().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn balance_leaf_hash_deterministic() {
        let user = Uuid::from_u128(1);
        let leaf1 = BalanceLeaf::new(user, "ETH", dec!(10.5));
        let leaf2 = BalanceLeaf::new(user, "ETH", dec!(10.5));
        assert_eq!(leaf1.hash, leaf2.hash);
    }

    #[test]
    fn different_balances_different_hashes() {
        let user = Uuid::from_u128(1);
        let leaf1 = BalanceLeaf::new(user, "ETH", dec!(10.0));
        let leaf2 = BalanceLeaf::new(user, "ETH", dec!(20.0));
        assert_ne!(leaf1.hash, leaf2.hash);
    }

    #[test]
    fn merkle_root_single_leaf() {
        let leaves = vec![BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100))];
        let root = merkle_root(&leaves);
        assert!(!root.is_empty());
        assert_eq!(root.len(), 64); // SHA-256 hex
    }

    #[test]
    fn merkle_root_deterministic() {
        let leaves = vec![
            BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100)),
            BalanceLeaf::new(Uuid::from_u128(2), "ETH", dec!(200)),
        ];
        let root1 = merkle_root(&leaves);
        let root2 = merkle_root(&leaves);
        assert_eq!(root1, root2);
    }

    #[test]
    fn merkle_root_changes_with_balance() {
        let leaves1 = vec![
            BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100)),
            BalanceLeaf::new(Uuid::from_u128(2), "ETH", dec!(200)),
        ];
        let leaves2 = vec![
            BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100)),
            BalanceLeaf::new(Uuid::from_u128(2), "ETH", dec!(201)), // changed
        ];
        assert_ne!(merkle_root(&leaves1), merkle_root(&leaves2));
    }

    #[test]
    fn merkle_proof_verification() {
        // Simple 2-leaf tree
        let leaf_a = BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100));
        let leaf_b = BalanceLeaf::new(Uuid::from_u128(2), "ETH", dec!(200));

        let root = merkle_root(&[leaf_a.clone(), leaf_b.clone()]);

        // Proof for leaf_a: sibling is leaf_b (right)
        let proof = MerkleProof {
            leaf: leaf_a,
            siblings: vec![leaf_b.hash.clone()],
            directions: vec![true], // sibling is on the right
            root_hash: root,
        };

        assert!(proof.verify());
    }

    #[test]
    fn invalid_proof_fails() {
        let leaf = BalanceLeaf::new(Uuid::from_u128(1), "ETH", dec!(100));
        let proof = MerkleProof {
            leaf,
            siblings: vec!["wrong_hash".to_string()],
            directions: vec![true],
            root_hash: "also_wrong".to_string(),
        };
        assert!(!proof.verify());
    }

    #[test]
    fn empty_tree_root() {
        let root = merkle_root(&[]);
        assert!(!root.is_empty());
    }
}
