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

/// Build ABI-encoded arguments for `constructor(string,string,uint8,uint256)`.
///
/// Solidity ABI layout for this signature is head/tail:
///
/// ```text
/// head (4 × 32 bytes):
///   [0x00]  offset of `name`   tail = 0x80   (uint256)
///   [0x20]  offset of `symbol` tail = dynamic (uint256)
///   [0x40]  decimals                         (uint256, right-aligned)
///   [0x60]  initial_supply * 10^decimals     (uint256, right-aligned)
/// tail:
///   name:   uint256 length || UTF-8 bytes || zero-pad to 32-byte boundary
///   symbol: uint256 length || UTF-8 bytes || zero-pad to 32-byte boundary
/// ```
///
/// The old implementation put the length in a u32 at an arbitrary
/// offset and encoded `initial_supply` as its ASCII-digit bytes with
/// the decimal point stripped — both of which produce bytes a
/// Solidity constructor will decode into garbage, not "1000000.5 USDX".
///
/// `initial_supply` is scaled by `10^decimals` to match the on-chain
/// uint256 representation. Returns `Err` if the scaled amount does
/// not fit in a `u128` (about 3.4 × 10^38), which covers every
/// reasonable ERC-20 supply; assets with supplies beyond that should
/// use a specialised deploy path.
pub fn erc20_constructor_args(
    name: &str,
    symbol: &str,
    decimals: u8,
    initial_supply: Decimal,
) -> anyhow::Result<Vec<u8>> {
    if initial_supply.is_sign_negative() {
        return Err(anyhow::anyhow!("initial_supply must be non-negative"));
    }
    if name.len() > 255 || symbol.len() > 32 {
        return Err(anyhow::anyhow!(
            "name/symbol too long: name.len={}, symbol.len={} (caps: 255/32)",
            name.len(), symbol.len()
        ));
    }

    // Scale supply: (supply * 10^decimals) rounded, then fit into u128.
    // rust_decimal holds up to 28 significant digits, so decimals > 28
    // would lose precision silently; reject that early.
    if decimals > 28 {
        return Err(anyhow::anyhow!(
            "decimals {decimals} exceeds rust_decimal's 28-digit precision"
        ));
    }
    let scale = Decimal::from(10u64.pow(decimals as u32));
    let scaled = initial_supply.checked_mul(scale)
        .ok_or_else(|| anyhow::anyhow!("initial_supply × 10^decimals overflows"))?;
    let scaled = scaled.round();
    let supply_u128: u128 = scaled.to_string().parse()
        .map_err(|_| anyhow::anyhow!("scaled supply does not fit in u128"))?;

    let mut data = Vec::with_capacity(4 * 32 + aligned_tail_len(name) + aligned_tail_len(symbol));

    // --- head ---
    let head_size: u64 = 4 * 32;
    let name_offset: u64 = head_size; // 0x80
    let symbol_offset: u64 = head_size + aligned_tail_len(name) as u64;

    data.extend_from_slice(&u256_be(name_offset as u128));
    data.extend_from_slice(&u256_be(symbol_offset as u128));
    data.extend_from_slice(&u256_be(decimals as u128));
    data.extend_from_slice(&u256_be(supply_u128));

    // --- tail ---
    data.extend_from_slice(&encode_abi_string(name));
    data.extend_from_slice(&encode_abi_string(symbol));

    Ok(data)
}

/// Length in bytes of a dynamic `string` in the ABI tail:
///   32 bytes for the length prefix + content padded to a 32-byte boundary.
fn aligned_tail_len(s: &str) -> usize {
    let content = s.len();
    let padded = content.div_ceil(32) * 32;
    32 + padded
}

/// ABI-encode a dynamic string: uint256 length || bytes || zero-pad to 32.
fn encode_abi_string(s: &str) -> Vec<u8> {
    let len = s.len();
    let padded = len.div_ceil(32) * 32;
    let mut out = Vec::with_capacity(32 + padded);
    out.extend_from_slice(&u256_be(len as u128));
    out.extend_from_slice(s.as_bytes());
    out.extend(std::iter::repeat_n(0u8, padded - len));
    out
}

/// Left-zero-pad a u128 into a 32-byte uint256 in big-endian.
fn u256_be(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
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
    fn constructor_args_layout_is_head_plus_tails() {
        // name = "Test Token" (10 bytes), symbol = "TEST" (4 bytes).
        // head  = 4 × 32 = 128 bytes
        // name  tail: 32 (length) + 32 (padded content) = 64 bytes
        // sym   tail: 32 (length) + 32 (padded content) = 64 bytes
        // total = 256 bytes.
        let args = erc20_constructor_args("Test Token", "TEST", 18, dec!(1_000_000)).unwrap();
        assert_eq!(args.len(), 256);

        // Head slot 0 = offset of name tail = 0x80 (128)
        let name_off = u128::from_be_bytes(args[16..32].try_into().unwrap());
        assert_eq!(name_off, 128);
        // Head slot 1 = offset of symbol tail = 128 + 64 = 192
        let sym_off = u128::from_be_bytes(args[48..64].try_into().unwrap());
        assert_eq!(sym_off, 192);
        // Head slot 2 = decimals = 18
        assert_eq!(args[64 + 31], 18);
        // Head slot 3 = supply scaled by 10^18 = 1_000_000 * 10^18
        let supply = u128::from_be_bytes(args[112..128].try_into().unwrap());
        assert_eq!(supply, 1_000_000u128 * 10u128.pow(18));
    }

    #[test]
    fn constructor_args_reproduce_name_bytes() {
        let args = erc20_constructor_args("USDX", "USDX", 6, dec!(100)).unwrap();
        // name tail begins at offset 128: 32-byte length, then 4-byte content.
        let name_len = u128::from_be_bytes(args[128 + 16..128 + 32].try_into().unwrap());
        assert_eq!(name_len, 4);
        assert_eq!(&args[128 + 32..128 + 36], b"USDX");
    }

    #[test]
    fn constructor_args_fractional_supply_scales_correctly() {
        // 1.5 tokens at 6 decimals = 1_500_000 atomic units.
        let args = erc20_constructor_args("X", "X", 6, dec!(1.5)).unwrap();
        let supply = u128::from_be_bytes(args[112..128].try_into().unwrap());
        assert_eq!(supply, 1_500_000);
    }

    #[test]
    fn constructor_args_rejects_negative_supply() {
        let err = erc20_constructor_args("X", "X", 18, dec!(-1)).unwrap_err();
        assert!(err.to_string().contains("non-negative"));
    }

    #[test]
    fn constructor_args_rejects_oversized_name() {
        let long = "a".repeat(256);
        assert!(erc20_constructor_args(&long, "X", 18, dec!(1)).is_err());
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
