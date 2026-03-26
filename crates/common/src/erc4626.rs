//! ERC-4626 Tokenized Vault Standard — yield-bearing token interactions.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A known ERC-4626 vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc4626Vault {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub asset: String,
    pub asset_decimals: u8,
    pub protocol: String,
    pub apy_estimate: Decimal,
}

/// Known ERC-4626 vaults on Ethereum.
pub fn known_vaults() -> Vec<Erc4626Vault> {
    use rust_decimal_macros::dec;
    vec![
        Erc4626Vault {
            address: "0x83F20F44975D03b1b09e64809B757c47f942BEeA".into(),
            name: "Savings DAI".into(), symbol: "sDAI".into(),
            asset: "DAI".into(), asset_decimals: 18,
            protocol: "Maker/Spark".into(), apy_estimate: dec!(5.0),
        },
        Erc4626Vault {
            address: "0x7a364e8770418566e3eb2001A96116E6138Eb32F".into(),
            name: "Yearn USDC".into(), symbol: "yvUSDC".into(),
            asset: "USDC".into(), asset_decimals: 6,
            protocol: "Yearn V3".into(), apy_estimate: dec!(4.5),
        },
        Erc4626Vault {
            address: "0x4DEDf26112B3Ec8eC46e7E31EA5e123490B05B8B".into(),
            name: "Morpho USDC".into(), symbol: "maUSDC".into(),
            asset: "USDC".into(), asset_decimals: 6,
            protocol: "Morpho".into(), apy_estimate: dec!(6.0),
        },
    ]
}

/// Encode `deposit(uint256,address)` — deposit assets, receive shares.
/// Selector: 0x6e553f65
pub fn encode_deposit(assets: &[u8; 32], receiver: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&[0x6e, 0x55, 0x3f, 0x65]);
    data.extend_from_slice(assets);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(receiver);
    data.extend_from_slice(&slot);
    data
}

/// Encode `withdraw(uint256,address,address)` — withdraw assets by burning shares.
/// Selector: 0xb460af94
pub fn encode_withdraw(assets: &[u8; 32], receiver: &[u8; 20], owner: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100);
    data.extend_from_slice(&[0xb4, 0x60, 0xaf, 0x94]);
    data.extend_from_slice(assets);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(receiver);
    data.extend_from_slice(&slot);
    slot = [0u8; 32];
    slot[12..].copy_from_slice(owner);
    data.extend_from_slice(&slot);
    data
}

/// Encode `convertToShares(uint256)` — view function.
/// Selector: 0xc6e6f592
pub fn encode_convert_to_shares(assets: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0xc6, 0xe6, 0xf5, 0x92]);
    data.extend_from_slice(assets);
    data
}

/// Encode `convertToAssets(uint256)` — view function.
/// Selector: 0x07a2d13a
pub fn encode_convert_to_assets(shares: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0x07, 0xa2, 0xd1, 0x3a]);
    data.extend_from_slice(shares);
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn known_vaults_list() {
        let vaults = known_vaults();
        assert!(vaults.len() >= 3);
        assert!(vaults.iter().any(|v| v.symbol == "sDAI"));
    }

    #[test]
    fn deposit_calldata() {
        let data = encode_deposit(&[0u8; 32], &[0xAA; 20]);
        assert_eq!(&data[..4], &[0x6e, 0x55, 0x3f, 0x65]);
        assert_eq!(data.len(), 68);
    }

    #[test]
    fn withdraw_calldata() {
        let data = encode_withdraw(&[0u8; 32], &[0xAA; 20], &[0xBB; 20]);
        assert_eq!(&data[..4], &[0xb4, 0x60, 0xaf, 0x94]);
        assert_eq!(data.len(), 100);
    }

    #[test]
    fn convert_to_shares_calldata() {
        let data = encode_convert_to_shares(&[0u8; 32]);
        assert_eq!(&data[..4], &[0xc6, 0xe6, 0xf5, 0x92]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn convert_to_assets_calldata() {
        let data = encode_convert_to_assets(&[0u8; 32]);
        assert_eq!(&data[..4], &[0x07, 0xa2, 0xd1, 0x3a]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn sdai_vault_info() {
        let vaults = known_vaults();
        let sdai = vaults.iter().find(|v| v.symbol == "sDAI").unwrap();
        assert_eq!(sdai.asset, "DAI");
        assert_eq!(sdai.apy_estimate, dec!(5.0));
    }
}
