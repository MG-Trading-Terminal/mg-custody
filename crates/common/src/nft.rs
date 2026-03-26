//! NFT custody — ERC-721, ERC-1155, Solana Metaplex support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// An NFT held in custody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftAsset {
    pub id: Uuid,
    pub vault_id: Uuid,
    pub chain: Chain,
    pub contract_address: String,
    pub token_id: String,
    pub standard: NftStandard,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub metadata_url: Option<String>,
    pub collection: Option<String>,
    /// For ERC-1155: quantity held.
    pub quantity: u64,
    pub acquired_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NftStandard {
    Erc721,
    Erc1155,
    SolanaMetaplex,
    SolanaCnft, // compressed NFT
}

/// NFT transfer request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftTransferRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub contract_address: String,
    pub token_id: String,
    pub to_address: String,
    /// For ERC-1155: quantity to transfer.
    pub quantity: Option<u64>,
}

/// Encode ERC-721 `transferFrom(address,address,uint256)` calldata.
pub fn encode_erc721_transfer(from: &[u8; 20], to: &[u8; 20], token_id: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100);
    // Function selector: transferFrom(address,address,uint256) = 0x23b872dd
    data.extend_from_slice(&[0x23, 0xb8, 0x72, 0xdd]);
    // from address (padded to 32 bytes)
    let mut from_padded = [0u8; 32];
    from_padded[12..].copy_from_slice(from);
    data.extend_from_slice(&from_padded);
    // to address
    let mut to_padded = [0u8; 32];
    to_padded[12..].copy_from_slice(to);
    data.extend_from_slice(&to_padded);
    // token_id
    data.extend_from_slice(token_id);
    data
}

/// Encode ERC-1155 `safeTransferFrom(address,address,uint256,uint256,bytes)` calldata.
pub fn encode_erc1155_transfer(
    from: &[u8; 20], to: &[u8; 20],
    token_id: &[u8; 32], amount: &[u8; 32],
) -> Vec<u8> {
    let mut data = Vec::with_capacity(196);
    // Function selector: safeTransferFrom(address,address,uint256,uint256,bytes) = 0xf242432a
    data.extend_from_slice(&[0xf2, 0x42, 0x43, 0x2a]);
    // from
    let mut from_padded = [0u8; 32];
    from_padded[12..].copy_from_slice(from);
    data.extend_from_slice(&from_padded);
    // to
    let mut to_padded = [0u8; 32];
    to_padded[12..].copy_from_slice(to);
    data.extend_from_slice(&to_padded);
    // token_id
    data.extend_from_slice(token_id);
    // amount
    data.extend_from_slice(amount);
    // bytes offset (pointing to empty bytes)
    let mut offset = [0u8; 32];
    offset[31] = 0xa0; // 5 * 32 = 160
    data.extend_from_slice(&offset);
    // bytes length = 0
    data.extend_from_slice(&[0u8; 32]);
    data
}

/// Encode ERC-721 `approve(address,uint256)` calldata.
pub fn encode_erc721_approve(spender: &[u8; 20], token_id: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    // Function selector: approve(address,uint256) = 0x095ea7b3
    data.extend_from_slice(&[0x09, 0x5e, 0xa7, 0xb3]);
    let mut spender_padded = [0u8; 32];
    spender_padded[12..].copy_from_slice(spender);
    data.extend_from_slice(&spender_padded);
    data.extend_from_slice(token_id);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erc721_transfer_calldata() {
        let from = [0xAA; 20];
        let to = [0xBB; 20];
        let token_id = [0u8; 32];
        let data = encode_erc721_transfer(&from, &to, &token_id);
        assert_eq!(&data[..4], &[0x23, 0xb8, 0x72, 0xdd]);
        assert_eq!(data.len(), 100);
        // from at offset 16..36
        assert_eq!(&data[16..36], &[0xAA; 20]);
        // to at offset 48..68
        assert_eq!(&data[48..68], &[0xBB; 20]);
    }

    #[test]
    fn erc1155_transfer_calldata() {
        let from = [0xAA; 20];
        let to = [0xBB; 20];
        let token_id = [0u8; 32];
        let amount = {
            let mut a = [0u8; 32];
            a[31] = 5; // amount = 5
            a
        };
        let data = encode_erc1155_transfer(&from, &to, &token_id, &amount);
        assert_eq!(&data[..4], &[0xf2, 0x42, 0x43, 0x2a]);
        assert_eq!(data.len(), 196);
    }

    #[test]
    fn erc721_approve_calldata() {
        let spender = [0xCC; 20];
        let token_id = [0u8; 32];
        let data = encode_erc721_approve(&spender, &token_id);
        assert_eq!(&data[..4], &[0x09, 0x5e, 0xa7, 0xb3]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn nft_standard_serialization() {
        assert_eq!(serde_json::to_string(&NftStandard::Erc721).unwrap(), r#""erc721""#);
        assert_eq!(serde_json::to_string(&NftStandard::Erc1155).unwrap(), r#""erc1155""#);
        assert_eq!(serde_json::to_string(&NftStandard::SolanaMetaplex).unwrap(), r#""solana_metaplex""#);
    }

    #[test]
    fn nft_asset_structure() {
        let nft = NftAsset {
            id: Uuid::nil(), vault_id: Uuid::nil(),
            chain: Chain::Ethereum, contract_address: "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D".into(),
            token_id: "1234".into(), standard: NftStandard::Erc721,
            name: Some("Bored Ape #1234".into()), description: None,
            image_url: Some("ipfs://Qm...".into()), metadata_url: None,
            collection: Some("BAYC".into()), quantity: 1,
            acquired_at: Utc::now(),
        };
        let json = serde_json::to_string(&nft).unwrap();
        assert!(json.contains("Bored Ape"));
        assert!(json.contains("BAYC"));
    }

    #[test]
    fn nft_transfer_request() {
        let req = NftTransferRequest {
            vault_id: Uuid::nil(), chain: Chain::Ethereum,
            contract_address: "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D".into(),
            token_id: "1234".into(), to_address: "0xabc".into(),
            quantity: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("1234"));
    }
}
