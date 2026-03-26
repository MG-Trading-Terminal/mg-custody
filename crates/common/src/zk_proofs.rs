//! Zero-knowledge proofs — Proof of Reserves, Proof of Solvency, commitment schemes.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Proof of Reserves — cryptographic proof that custody holds claimed assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfReserves {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub total_liabilities: Decimal,
    pub total_assets: Decimal,
    pub reserve_ratio: Decimal,
    pub merkle_root: String,
    pub asset_proofs: Vec<AssetProof>,
    pub is_solvent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetProof {
    pub asset: String,
    pub chain: crate::Chain,
    pub claimed_balance: Decimal,
    pub on_chain_balance: Decimal,
    pub address: String,
    pub block_height: u64,
    pub verified: bool,
}

/// Pedersen commitment: C = g^v * h^r (value hidden, but verifiable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedersenCommitment {
    pub commitment: String,
    pub asset: String,
    pub blinding_factor_hash: String,
}

/// Range proof — proves value is in [0, 2^64) without revealing it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeProof {
    pub commitment: String,
    pub proof: String,
    pub bit_length: u32,
}

/// Merkle proof for user balance verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceMerkleProof {
    pub user_id_hash: String,
    pub balance_commitment: String,
    pub merkle_path: Vec<MerkleNode>,
    pub root: String,
    pub leaf_index: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    pub hash: String,
    pub position: MerklePosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerklePosition { Left, Right }

/// Verify a Merkle proof (simplified — uses SHA-256).
pub fn verify_merkle_proof(leaf_hash: &str, proof: &[MerkleNode], root: &str) -> bool {
    use sha2::Digest;
    let mut current = hex::decode(leaf_hash).unwrap_or_default();

    for node in proof {
        let sibling = hex::decode(&node.hash).unwrap_or_default();
        let mut hasher = sha2::Sha256::new();
        match node.position {
            MerklePosition::Left => {
                hasher.update(&sibling);
                hasher.update(&current);
            }
            MerklePosition::Right => {
                hasher.update(&current);
                hasher.update(&sibling);
            }
        }
        current = hasher.finalize().to_vec();
    }

    hex::encode(&current) == root
}

/// Build a Merkle tree from leaf hashes and return the root.
pub fn build_merkle_root(leaves: &[String]) -> String {
    use sha2::Digest;
    if leaves.is_empty() { return String::new(); }
    if leaves.len() == 1 { return leaves[0].clone(); }

    let mut level: Vec<Vec<u8>> = leaves.iter()
        .map(|l| hex::decode(l).unwrap_or_default())
        .collect();

    while level.len() > 1 {
        let mut next = Vec::new();
        for chunk in level.chunks(2) {
            let mut hasher = sha2::Sha256::new();
            hasher.update(&chunk[0]);
            if chunk.len() > 1 {
                hasher.update(&chunk[1]);
            } else {
                hasher.update(&chunk[0]); // duplicate last
            }
            next.push(hasher.finalize().to_vec());
        }
        level = next;
    }

    hex::encode(&level[0])
}

/// Hash a user balance leaf: SHA-256(user_id_hash || asset || balance_str).
pub fn hash_balance_leaf(user_id_hash: &str, asset: &str, balance: Decimal) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(user_id_hash.as_bytes());
    hasher.update(asset.as_bytes());
    hasher.update(balance.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a proof of reserves attestation.
pub fn generate_attestation(
    assets: &[(String, crate::Chain, Decimal, Decimal, String, u64)], // (asset, chain, claimed, on_chain, addr, block)
    total_liabilities: Decimal,
) -> ProofOfReserves {
    let asset_proofs: Vec<AssetProof> = assets.iter().map(|(asset, chain, claimed, on_chain, addr, block)| {
        AssetProof {
            asset: asset.clone(), chain: *chain,
            claimed_balance: *claimed, on_chain_balance: *on_chain,
            address: addr.clone(), block_height: *block,
            verified: on_chain >= claimed,
        }
    }).collect();

    let total_assets: Decimal = asset_proofs.iter().map(|p| p.on_chain_balance).sum();
    let reserve_ratio = if total_liabilities > Decimal::ZERO {
        total_assets / total_liabilities
    } else { Decimal::ZERO };

    let leaves: Vec<String> = asset_proofs.iter()
        .map(|p| hash_balance_leaf(&p.address, &p.asset, p.on_chain_balance))
        .collect();
    let merkle_root = build_merkle_root(&leaves);

    ProofOfReserves {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        total_liabilities, total_assets, reserve_ratio,
        merkle_root,
        asset_proofs,
        is_solvent: total_assets >= total_liabilities,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use crate::Chain;

    #[test]
    fn merkle_root_single_leaf() {
        let root = build_merkle_root(&["aabb".into()]);
        assert_eq!(root, "aabb");
    }

    #[test]
    fn merkle_root_two_leaves() {
        let root = build_merkle_root(&["aa".into(), "bb".into()]);
        assert!(!root.is_empty());
        assert_eq!(root.len(), 64); // SHA-256 = 32 bytes = 64 hex
    }

    #[test]
    fn merkle_root_deterministic() {
        let leaves = vec!["aa".into(), "bb".into(), "cc".into()];
        let r1 = build_merkle_root(&leaves);
        let r2 = build_merkle_root(&leaves);
        assert_eq!(r1, r2);
    }

    #[test]
    fn merkle_root_order_matters() {
        let r1 = build_merkle_root(&["aa".into(), "bb".into()]);
        let r2 = build_merkle_root(&["bb".into(), "aa".into()]);
        assert_ne!(r1, r2);
    }

    #[test]
    fn verify_simple_proof() {
        use sha2::Digest;
        let leaf_a = hex::encode(sha2::Sha256::digest(b"leaf_a"));
        let leaf_b = hex::encode(sha2::Sha256::digest(b"leaf_b"));
        let root = build_merkle_root(&[leaf_a.clone(), leaf_b.clone()]);

        let proof = vec![MerkleNode { hash: leaf_b, position: MerklePosition::Right }];
        assert!(verify_merkle_proof(&leaf_a, &proof, &root));
    }

    #[test]
    fn hash_balance_leaf_deterministic() {
        let h1 = hash_balance_leaf("user123", "ETH", dec!(1.5));
        let h2 = hash_balance_leaf("user123", "ETH", dec!(1.5));
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_balance_different_amounts() {
        let h1 = hash_balance_leaf("user1", "ETH", dec!(1.0));
        let h2 = hash_balance_leaf("user1", "ETH", dec!(2.0));
        assert_ne!(h1, h2);
    }

    #[test]
    fn generate_attestation_solvent() {
        let assets = vec![
            ("ETH".into(), Chain::Ethereum, dec!(100), dec!(105), "0x1".into(), 19500000u64),
            ("BTC".into(), Chain::Bitcoin, dec!(10), dec!(10), "bc1q".into(), 840000),
        ];
        let proof = generate_attestation(&assets, dec!(110));
        assert!(proof.is_solvent);
        assert!(proof.reserve_ratio >= Decimal::ONE);
        assert_eq!(proof.asset_proofs.len(), 2);
    }

    #[test]
    fn generate_attestation_insolvent() {
        let assets = vec![
            ("ETH".into(), Chain::Ethereum, dec!(100), dec!(50), "0x1".into(), 19500000u64),
        ];
        let proof = generate_attestation(&assets, dec!(100));
        assert!(!proof.is_solvent);
        assert!(proof.reserve_ratio < Decimal::ONE);
    }

    #[test]
    fn attestation_all_verified() {
        let assets = vec![
            ("USDC".into(), Chain::Ethereum, dec!(1000), dec!(1000), "0x1".into(), 19500000u64),
        ];
        let proof = generate_attestation(&assets, dec!(1000));
        assert!(proof.asset_proofs.iter().all(|p| p.verified));
    }

    #[test]
    fn pedersen_commitment_structure() {
        let c = PedersenCommitment {
            commitment: "0xabc123".into(),
            asset: "ETH".into(),
            blinding_factor_hash: "0xdef456".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("blinding_factor"));
    }

    #[test]
    fn range_proof_structure() {
        let rp = RangeProof {
            commitment: "0xcommit".into(),
            proof: "0xproof_data".into(),
            bit_length: 64,
        };
        assert_eq!(rp.bit_length, 64);
    }

    #[test]
    fn merkle_position_serialization() {
        assert_eq!(serde_json::to_string(&MerklePosition::Left).unwrap(), r#""left""#);
        assert_eq!(serde_json::to_string(&MerklePosition::Right).unwrap(), r#""right""#);
    }
}
