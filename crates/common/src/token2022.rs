//! Solana Token-2022 (Token Extensions) — transfer fees, confidential, hooks.

use serde::{Deserialize, Serialize};

/// Token-2022 program ID.
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
/// Original Token program ID.
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
/// Associated Token Account program.
pub const ATA_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

/// Token-2022 extension types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenExtension {
    TransferFee,
    ConfidentialTransfer,
    DefaultAccountState,
    ImmutableOwner,
    MemoRequired,
    NonTransferable,
    InterestBearing,
    PermanentDelegate,
    TransferHook,
    MetadataPointer,
    TokenMetadata,
    GroupPointer,
    GroupMember,
}

/// Transfer fee configuration for Token-2022.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferFeeConfig {
    /// Fee in basis points (e.g., 50 = 0.5%).
    pub fee_bps: u16,
    /// Maximum fee in token units.
    pub max_fee: u64,
    /// Authority that can update fees.
    pub fee_authority: String,
    /// Account that collects fees.
    pub withdrawal_authority: String,
}

/// Token-2022 mint metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token2022Mint {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub supply: u64,
    pub extensions: Vec<TokenExtension>,
    pub transfer_fee: Option<TransferFeeConfig>,
    pub is_confidential: bool,
}

/// Known Token-2022 tokens on Solana.
pub fn known_token2022_mints() -> Vec<Token2022Mint> {
    vec![
        Token2022Mint {
            address: "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo".into(),
            symbol: "PYUSD".into(), name: "PayPal USD".into(),
            decimals: 6, supply: 0,
            extensions: vec![TokenExtension::TransferFee, TokenExtension::MetadataPointer],
            transfer_fee: Some(TransferFeeConfig {
                fee_bps: 0, max_fee: 0,
                fee_authority: "".into(), withdrawal_authority: "".into(),
            }),
            is_confidential: false,
        },
        Token2022Mint {
            address: "CXk2AMBfi3TwaEL2468s6zP8xq9NxTXjp9gjMgzeUynM".into(),
            symbol: "BERN".into(), name: "BonkEarn".into(),
            decimals: 5, supply: 0,
            extensions: vec![TokenExtension::TransferFee],
            transfer_fee: Some(TransferFeeConfig {
                fee_bps: 500, max_fee: u64::MAX,
                fee_authority: "".into(), withdrawal_authority: "".into(),
            }),
            is_confidential: false,
        },
    ]
}

/// Calculate transfer fee for a Token-2022 transfer.
pub fn calculate_transfer_fee(amount: u64, fee_bps: u16, max_fee: u64) -> u64 {
    let fee = (amount as u128 * fee_bps as u128) / 10_000;
    (fee as u64).min(max_fee)
}

/// Check if a token uses Token-2022 program (vs legacy Token program).
pub fn is_token_2022(program_id: &str) -> bool {
    program_id == TOKEN_2022_PROGRAM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_fee_calculation() {
        assert_eq!(calculate_transfer_fee(1_000_000, 50, u64::MAX), 5_000); // 0.5%
        assert_eq!(calculate_transfer_fee(1_000_000, 100, u64::MAX), 10_000); // 1%
        assert_eq!(calculate_transfer_fee(1_000_000, 500, 1_000), 1_000); // capped
    }

    #[test]
    fn transfer_fee_zero() {
        assert_eq!(calculate_transfer_fee(1_000_000, 0, u64::MAX), 0);
    }

    #[test]
    fn is_token_2022_check() {
        assert!(is_token_2022(TOKEN_2022_PROGRAM));
        assert!(!is_token_2022(TOKEN_PROGRAM));
    }

    #[test]
    fn known_mints() {
        let mints = known_token2022_mints();
        assert!(mints.len() >= 2);
        assert!(mints.iter().any(|m| m.symbol == "PYUSD"));
    }

    #[test]
    fn extension_serialization() {
        assert_eq!(serde_json::to_string(&TokenExtension::TransferFee).unwrap(), r#""transfer_fee""#);
        assert_eq!(serde_json::to_string(&TokenExtension::ConfidentialTransfer).unwrap(), r#""confidential_transfer""#);
    }

    #[test]
    fn all_extensions_serialize() {
        let exts = vec![
            TokenExtension::TransferFee, TokenExtension::ConfidentialTransfer,
            TokenExtension::ImmutableOwner, TokenExtension::MemoRequired,
            TokenExtension::NonTransferable, TokenExtension::InterestBearing,
            TokenExtension::TransferHook, TokenExtension::TokenMetadata,
        ];
        for ext in exts {
            let json = serde_json::to_string(&ext).unwrap();
            let parsed: TokenExtension = serde_json::from_str(&json).unwrap();
            assert_eq!(ext, parsed);
        }
    }
}
