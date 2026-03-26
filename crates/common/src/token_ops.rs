//! Tokenization execution — ERC-20 deploy, mint, burn, pause operations.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

/// Token deployment request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployTokenRequest {
    pub vault_id: Uuid,
    pub chain: Chain,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_supply: Decimal,
    pub mintable: bool,
    pub burnable: bool,
    pub pausable: bool,
}

/// Result of token deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedToken {
    pub id: Uuid,
    pub contract_address: String,
    pub chain: Chain,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Decimal,
    pub owner_address: String,
    pub tx_hash: String,
    pub deployed_at: DateTime<Utc>,
}

/// Token operation types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum TokenOperation {
    Mint { to: String, amount: Decimal },
    Burn { from: String, amount: Decimal },
    Pause,
    Unpause,
    TransferOwnership { new_owner: String },
    Freeze { account: String },
    Unfreeze { account: String },
}

/// Build ERC-20 constructor calldata for deployment.
///
/// Simplified: encodes name, symbol, decimals, initial_supply as constructor args.
pub fn erc20_constructor_args(
    name: &str,
    symbol: &str,
    decimals: u8,
    initial_supply: Decimal,
) -> Vec<u8> {
    let mut data = Vec::new();
    // This is a simplified ABI encoding for constructor(string,string,uint8,uint256)
    // In production, use proper ABI encoding with offsets

    // Encode initial_supply as uint256 (32 bytes, big-endian)
    let supply_str = initial_supply.to_string().replace('.', "");
    let supply_bytes = supply_str.as_bytes();

    // name length + name bytes + symbol length + symbol bytes + decimals + supply
    data.extend_from_slice(&[0u8; 28]); // padding
    data.extend_from_slice(&(name.len() as u32).to_be_bytes());
    data.extend_from_slice(name.as_bytes());
    // Pad to 32 bytes
    let name_pad = 32 - (name.len() % 32);
    if name_pad < 32 { data.extend(vec![0u8; name_pad]); }

    data.extend_from_slice(&[0u8; 28]);
    data.extend_from_slice(&(symbol.len() as u32).to_be_bytes());
    data.extend_from_slice(symbol.as_bytes());
    let sym_pad = 32 - (symbol.len() % 32);
    if sym_pad < 32 { data.extend(vec![0u8; sym_pad]); }

    // decimals as uint8 in 32-byte slot
    let mut dec_slot = [0u8; 32];
    dec_slot[31] = decimals;
    data.extend_from_slice(&dec_slot);

    // supply as bytes in 32-byte slot
    let mut supply_slot = [0u8; 32];
    let start = 32 - supply_bytes.len().min(32);
    supply_slot[start..start + supply_bytes.len().min(32)]
        .copy_from_slice(&supply_bytes[..supply_bytes.len().min(32)]);
    data.extend_from_slice(&supply_slot);

    data
}

/// Encode ERC-20 `mint(address,uint256)` calldata.
pub fn encode_mint_calldata(to: &[u8; 20], amount_raw: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    // Function selector: mint(address,uint256) = 0x40c10f19
    data.extend_from_slice(&[0x40, 0xc1, 0x0f, 0x19]);
    let mut addr_padded = [0u8; 32];
    addr_padded[12..].copy_from_slice(to);
    data.extend_from_slice(&addr_padded);
    data.extend_from_slice(amount_raw);
    data
}

/// Encode ERC-20 `burn(uint256)` calldata.
pub fn encode_burn_calldata(amount_raw: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    // Function selector: burn(uint256) = 0x42966c68
    data.extend_from_slice(&[0x42, 0x96, 0x6c, 0x68]);
    data.extend_from_slice(amount_raw);
    data
}

/// Encode `pause()` calldata.
pub fn encode_pause_calldata() -> Vec<u8> {
    // Function selector: pause() = 0x8456cb59
    vec![0x84, 0x56, 0xcb, 0x59]
}

/// Encode `unpause()` calldata.
pub fn encode_unpause_calldata() -> Vec<u8> {
    // Function selector: unpause() = 0x3f4ba83a
    vec![0x3f, 0x4b, 0xa8, 0x3a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn mint_calldata_selector() {
        let to = [0xAB; 20];
        let amount = [0u8; 32];
        let data = encode_mint_calldata(&to, &amount);
        assert_eq!(&data[..4], &[0x40, 0xc1, 0x0f, 0x19]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn burn_calldata_selector() {
        let amount = [0u8; 32];
        let data = encode_burn_calldata(&amount);
        assert_eq!(&data[..4], &[0x42, 0x96, 0x6c, 0x68]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn pause_calldata() {
        let data = encode_pause_calldata();
        assert_eq!(data.len(), 4);
        assert_eq!(&data, &[0x84, 0x56, 0xcb, 0x59]);
    }

    #[test]
    fn unpause_calldata() {
        let data = encode_unpause_calldata();
        assert_eq!(data.len(), 4);
        assert_eq!(&data, &[0x3f, 0x4b, 0xa8, 0x3a]);
    }

    #[test]
    fn constructor_args_non_empty() {
        let args = erc20_constructor_args("Test Token", "TEST", 18, dec!(1000000));
        assert!(!args.is_empty());
        assert!(args.len() > 100); // should have substantial encoding
    }

    #[test]
    fn token_operation_serialization() {
        let op = TokenOperation::Mint { to: "0xabc".into(), amount: dec!(1000) };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("mint"));
        assert!(json.contains("0xabc"));

        let op = TokenOperation::Pause;
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("pause"));
    }

    #[test]
    fn deploy_request_structure() {
        let req = DeployTokenRequest {
            vault_id: Uuid::nil(), chain: Chain::Ethereum,
            name: "USD Stablecoin".into(), symbol: "USDX".into(),
            decimals: 6, initial_supply: dec!(1000000000),
            mintable: true, burnable: true, pausable: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("USDX"));
        assert!(json.contains("mintable"));
    }
}
