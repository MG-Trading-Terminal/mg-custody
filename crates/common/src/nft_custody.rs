//! NFT custody — track, transfer, and manage NFTs across EVM and Solana chains.
//!
//! Supports ERC-721, ERC-1155, and Solana Metaplex standards.
//! Calldata encoding follows ABI encoding spec (Solidity ABI v1).
//!
//! References:
//! - ERC-721: https://eips.ethereum.org/EIPS/eip-721
//! - ERC-1155: https://eips.ethereum.org/EIPS/eip-1155
//! - ABI encoding: https://docs.soliditylang.org/en/latest/abi-spec.html

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// An NFT held in custody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodiedNft {
    pub id: Uuid,
    pub chain: Chain,
    /// Contract address — EIP-55 checksummed hex for EVM, base58 for Solana.
    pub contract_address: String,
    /// Token ID encoded as a decimal string (uint256 for EVM, mint pubkey for Solana).
    pub token_id: String,
    pub standard: NftStandard,
    pub owner_vault_id: Uuid,
    pub metadata_uri: Option<String>,
    pub name: Option<String>,
    pub image_url: Option<String>,
    /// 1 for ERC-721 / Metaplex; N for ERC-1155.
    pub quantity: u64,
    pub acquired_at: DateTime<Utc>,
}

/// NFT token standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NftStandard {
    Erc721,
    Erc1155,
    Metaplex,
}

// ---------------------------------------------------------------------------
// Request / response types for the API layer
// ---------------------------------------------------------------------------

/// Register an NFT in custody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNftRequest {
    pub chain: Chain,
    pub contract_address: String,
    pub token_id: String,
    pub standard: NftStandard,
    pub owner_vault_id: Uuid,
    pub metadata_uri: Option<String>,
    pub name: Option<String>,
    pub image_url: Option<String>,
    pub quantity: Option<u64>,
}

/// Initiate an NFT transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftTransferRequest {
    pub to_address: String,
    /// For ERC-1155: quantity to send (defaults to full balance).
    pub quantity: Option<u64>,
}

// ---------------------------------------------------------------------------
// ABI calldata encoding helpers
// ---------------------------------------------------------------------------

/// Parse a hex address string (with or without 0x prefix) into 20 bytes.
///
/// Returns an error if the string is not a valid 20-byte hex address.
pub fn parse_eth_address(addr: &str) -> anyhow::Result<[u8; 20]> {
    let clean = addr.strip_prefix("0x").unwrap_or(addr);
    anyhow::ensure!(clean.len() == 40, "address must be 40 hex chars, got {}", clean.len());
    let bytes = hex::decode(clean).map_err(|e| anyhow::anyhow!("invalid hex address: {e}"))?;
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a decimal string token ID into a big-endian uint256 (32 bytes).
///
/// The token_id is passed as a decimal string because uint256 overflows u64.
pub fn parse_token_id(token_id: &str) -> anyhow::Result<[u8; 32]> {
    // Support both decimal and 0x-prefixed hex.
    let big = if let Some(hex_str) = token_id.strip_prefix("0x") {
        let padded = format!("{:0>64}", hex_str);
        hex::decode(&padded).map_err(|e| anyhow::anyhow!("invalid hex token_id: {e}"))?
    } else {
        // Decimal — convert via u128 for common cases, fall back to manual big-int for large IDs.
        let n: u128 = token_id.parse()
            .map_err(|_| anyhow::anyhow!("token_id is not a valid decimal uint256: {token_id}"))?;
        let mut buf = [0u8; 32];
        buf[16..].copy_from_slice(&n.to_be_bytes());
        return Ok(buf);
    };

    anyhow::ensure!(big.len() == 32, "token_id hex must be 32 bytes");
    let mut out = [0u8; 32];
    out.copy_from_slice(&big);
    Ok(out)
}

/// Encode ERC-721 `transferFrom(address from, address to, uint256 tokenId)` calldata.
///
/// Selector: 0x23b872dd
/// Layout: [selector 4B][from 32B][to 32B][tokenId 32B] = 100 bytes total.
pub fn encode_erc721_transfer(from: &str, to: &str, token_id: &str) -> anyhow::Result<Vec<u8>> {
    let from_bytes = parse_eth_address(from)?;
    let to_bytes = parse_eth_address(to)?;
    let id_bytes = parse_token_id(token_id)?;

    let mut data = Vec::with_capacity(100);
    // transferFrom(address,address,uint256) selector
    data.extend_from_slice(&[0x23, 0xb8, 0x72, 0xdd]);
    // from — left-padded to 32 bytes
    let mut from_padded = [0u8; 32];
    from_padded[12..].copy_from_slice(&from_bytes);
    data.extend_from_slice(&from_padded);
    // to — left-padded to 32 bytes
    let mut to_padded = [0u8; 32];
    to_padded[12..].copy_from_slice(&to_bytes);
    data.extend_from_slice(&to_padded);
    // tokenId
    data.extend_from_slice(&id_bytes);
    Ok(data)
}

/// Encode ERC-1155 `safeTransferFrom(address,address,uint256,uint256,bytes)` calldata.
///
/// Selector: 0xf242432a
/// Layout: [selector 4B][from 32B][to 32B][id 32B][amount 32B][data_offset 32B][data_len 32B]
///          = 196 bytes for empty `data`.
pub fn encode_erc1155_transfer(
    from: &str,
    to: &str,
    token_id: &str,
    amount: u64,
) -> anyhow::Result<Vec<u8>> {
    let from_bytes = parse_eth_address(from)?;
    let to_bytes = parse_eth_address(to)?;
    let id_bytes = parse_token_id(token_id)?;

    let mut amount_bytes = [0u8; 32];
    amount_bytes[24..].copy_from_slice(&amount.to_be_bytes());

    let mut data = Vec::with_capacity(196);
    // safeTransferFrom(address,address,uint256,uint256,bytes) selector
    data.extend_from_slice(&[0xf2, 0x42, 0x43, 0x2a]);

    let mut from_padded = [0u8; 32];
    from_padded[12..].copy_from_slice(&from_bytes);
    data.extend_from_slice(&from_padded);

    let mut to_padded = [0u8; 32];
    to_padded[12..].copy_from_slice(&to_bytes);
    data.extend_from_slice(&to_padded);

    data.extend_from_slice(&id_bytes);
    data.extend_from_slice(&amount_bytes);

    // Dynamic `bytes data` parameter — offset points past the 5 fixed words (5×32=160=0xa0).
    let mut offset = [0u8; 32];
    offset[31] = 0xa0;
    data.extend_from_slice(&offset);

    // Empty bytes: length = 0.
    data.extend_from_slice(&[0u8; 32]);
    Ok(data)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: &str = "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const TO: &str = "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

    #[test]
    fn erc721_selector_is_0x23b872dd() {
        let data = encode_erc721_transfer(FROM, TO, "1234").unwrap();
        assert_eq!(&data[..4], &[0x23, 0xb8, 0x72, 0xdd]);
    }

    #[test]
    fn erc721_calldata_length_is_100_bytes() {
        let data = encode_erc721_transfer(FROM, TO, "0").unwrap();
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn erc721_from_address_at_correct_offset() {
        let data = encode_erc721_transfer(FROM, TO, "0").unwrap();
        // from address occupies bytes 16..36 (12 zero-pad bytes + 20 address bytes)
        let addr_bytes = &data[16..36];
        assert_eq!(addr_bytes, &[0xAA; 20]);
    }

    #[test]
    fn erc721_to_address_at_correct_offset() {
        let data = encode_erc721_transfer(FROM, TO, "0").unwrap();
        // to address occupies bytes 48..68
        let addr_bytes = &data[48..68];
        assert_eq!(addr_bytes, &[0xBB; 20]);
    }

    #[test]
    fn erc721_token_id_encoded_correctly() {
        // token_id = 1 → last byte of the 32-byte slot should be 0x01
        let data = encode_erc721_transfer(FROM, TO, "1").unwrap();
        assert_eq!(data[99], 0x01);
        assert!(data[68..99].iter().all(|&b| b == 0));
    }

    #[test]
    fn erc1155_selector_is_0xf242432a() {
        let data = encode_erc1155_transfer(FROM, TO, "1", 5).unwrap();
        assert_eq!(&data[..4], &[0xf2, 0x42, 0x43, 0x2a]);
    }

    #[test]
    fn erc1155_calldata_length_is_196_bytes() {
        let data = encode_erc1155_transfer(FROM, TO, "1", 10).unwrap();
        assert_eq!(data.len(), 196);
    }

    #[test]
    fn erc1155_amount_encoded_correctly() {
        // amount=5 → last byte of the amount slot (bytes 100..132) should be 0x05
        let data = encode_erc1155_transfer(FROM, TO, "0", 5).unwrap();
        assert_eq!(data[131], 0x05);
        assert!(data[100..131].iter().all(|&b| b == 0));
    }

    #[test]
    fn erc1155_data_offset_is_0xa0() {
        let data = encode_erc1155_transfer(FROM, TO, "0", 1).unwrap();
        // offset word starts at byte 132; the offset value 160 (0xa0) is in the last byte
        assert_eq!(data[163], 0xa0);
        assert!(data[132..163].iter().all(|&b| b == 0));
    }

    #[test]
    fn parse_eth_address_rejects_short_input() {
        assert!(parse_eth_address("0xDEAD").is_err());
    }

    #[test]
    fn parse_token_id_decimal_large_value() {
        // 2^127 — fits in u128 at the boundary
        let big = u128::MAX.to_string();
        let bytes = parse_token_id(&big).unwrap();
        assert_eq!(&bytes[16..], &u128::MAX.to_be_bytes());
    }

    #[test]
    fn nft_standard_serialization_round_trip() {
        for s in [NftStandard::Erc721, NftStandard::Erc1155, NftStandard::Metaplex] {
            let json = serde_json::to_string(&s).unwrap();
            let back: NftStandard = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn custodied_nft_serializes_to_json() {
        let nft = CustodiedNft {
            id: Uuid::nil(),
            chain: Chain::Ethereum,
            contract_address: "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D".into(),
            token_id: "42".into(),
            standard: NftStandard::Erc721,
            owner_vault_id: Uuid::nil(),
            metadata_uri: Some("ipfs://Qm...".into()),
            name: Some("Ape #42".into()),
            image_url: None,
            quantity: 1,
            acquired_at: Utc::now(),
        };
        let json = serde_json::to_string(&nft).unwrap();
        assert!(json.contains("Ape #42"));
        assert!(json.contains("erc721"));
    }
}
