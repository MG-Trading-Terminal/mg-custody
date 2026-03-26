//! Solana instruction builders — SPL Token, Stake, System, ATA.

use serde::{Deserialize, Serialize};

/// Known Solana program IDs.
pub mod programs {
    pub const SYSTEM: &str = "11111111111111111111111111111111";
    pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    pub const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
    pub const ATA: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
    pub const STAKE: &str = "Stake11111111111111111111111111111111111111";
    pub const VOTE: &str = "Vote111111111111111111111111111111111111111";
    pub const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
    pub const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";
    pub const NAME_SERVICE: &str = "namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX";
}

/// SystemProgram::Transfer instruction data.
pub fn system_transfer_data(lamports: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&2u32.to_le_bytes()); // Transfer = instruction index 2
    data.extend_from_slice(&lamports.to_le_bytes());
    data
}

/// SystemProgram::CreateAccount instruction data.
pub fn system_create_account_data(lamports: u64, space: u64, owner: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(52);
    data.extend_from_slice(&0u32.to_le_bytes()); // CreateAccount = instruction index 0
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner);
    data
}

/// SPL Token::Transfer instruction data.
pub fn spl_token_transfer_data(amount: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(9);
    data.push(3); // Transfer = instruction index 3
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

/// SPL Token::Approve instruction data.
pub fn spl_token_approve_data(amount: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(9);
    data.push(4); // Approve = instruction index 4
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

/// SPL Token::MintTo instruction data.
pub fn spl_token_mint_to_data(amount: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(9);
    data.push(7); // MintTo = instruction index 7
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

/// SPL Token::Burn instruction data.
pub fn spl_token_burn_data(amount: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(9);
    data.push(8); // Burn = instruction index 8
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

/// SPL Token::CloseAccount instruction data.
pub fn spl_token_close_account_data() -> Vec<u8> {
    vec![9] // CloseAccount = instruction index 9
}

/// Stake::Initialize instruction data.
pub fn stake_initialize_data(authorized_staker: &[u8; 32], authorized_withdrawer: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(68);
    data.extend_from_slice(&0u32.to_le_bytes()); // Initialize = 0
    data.extend_from_slice(authorized_staker);
    data.extend_from_slice(authorized_withdrawer);
    data
}

/// Stake::Delegate instruction data.
pub fn stake_delegate_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(4);
    data.extend_from_slice(&2u32.to_le_bytes()); // DelegateStake = 2
    data
}

/// Stake::Deactivate instruction data.
pub fn stake_deactivate_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(4);
    data.extend_from_slice(&5u32.to_le_bytes()); // Deactivate = 5
    data
}

/// Stake::Withdraw instruction data.
pub fn stake_withdraw_data(lamports: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&4u32.to_le_bytes()); // Withdraw = 4
    data.extend_from_slice(&lamports.to_le_bytes());
    data
}

/// Memo instruction.
pub fn memo_data(message: &str) -> Vec<u8> {
    message.as_bytes().to_vec()
}

/// A serializable Solana instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaInstruction {
    pub program_id: String,
    pub accounts: Vec<SolanaAccountMeta>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaAccountMeta {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_transfer() {
        let data = system_transfer_data(1_000_000_000); // 1 SOL
        assert_eq!(data.len(), 12);
        let idx = u32::from_le_bytes(data[..4].try_into().unwrap());
        assert_eq!(idx, 2);
        let lamports = u64::from_le_bytes(data[4..12].try_into().unwrap());
        assert_eq!(lamports, 1_000_000_000);
    }

    #[test]
    fn system_create_account() {
        let owner = [0u8; 32];
        let data = system_create_account_data(1_000_000, 165, &owner);
        assert_eq!(data.len(), 52);
        let idx = u32::from_le_bytes(data[..4].try_into().unwrap());
        assert_eq!(idx, 0);
    }

    #[test]
    fn spl_transfer() {
        let data = spl_token_transfer_data(1_000_000);
        assert_eq!(data[0], 3);
        assert_eq!(data.len(), 9);
        let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
        assert_eq!(amount, 1_000_000);
    }

    #[test]
    fn spl_approve() {
        let data = spl_token_approve_data(500_000);
        assert_eq!(data[0], 4);
    }

    #[test]
    fn spl_mint_to() {
        let data = spl_token_mint_to_data(1_000_000);
        assert_eq!(data[0], 7);
    }

    #[test]
    fn spl_burn() {
        let data = spl_token_burn_data(500_000);
        assert_eq!(data[0], 8);
    }

    #[test]
    fn spl_close_account() {
        let data = spl_token_close_account_data();
        assert_eq!(data, vec![9]);
    }

    #[test]
    fn stake_initialize() {
        let staker = [0xAA; 32];
        let withdrawer = [0xBB; 32];
        let data = stake_initialize_data(&staker, &withdrawer);
        assert_eq!(data.len(), 68);
        assert_eq!(&data[4..36], &[0xAA; 32]);
    }

    #[test]
    fn stake_delegate() {
        let data = stake_delegate_data();
        assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()), 2);
    }

    #[test]
    fn stake_deactivate() {
        let data = stake_deactivate_data();
        assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()), 5);
    }

    #[test]
    fn stake_withdraw() {
        let data = stake_withdraw_data(2_000_000_000);
        assert_eq!(data.len(), 12);
        let lamports = u64::from_le_bytes(data[4..12].try_into().unwrap());
        assert_eq!(lamports, 2_000_000_000);
    }

    #[test]
    fn memo_instruction() {
        let data = memo_data("custody transfer #123");
        assert_eq!(data, b"custody transfer #123");
    }

    #[test]
    fn program_ids_valid() {
        assert_eq!(programs::SYSTEM.len(), 32);
        assert!(programs::TOKEN.len() >= 32);
        assert!(programs::STAKE.len() >= 32);
    }

    #[test]
    fn solana_instruction_serialization() {
        let ix = SolanaInstruction {
            program_id: programs::SYSTEM.into(),
            accounts: vec![
                SolanaAccountMeta { pubkey: "from".into(), is_signer: true, is_writable: true },
                SolanaAccountMeta { pubkey: "to".into(), is_signer: false, is_writable: true },
            ],
            data: system_transfer_data(100),
        };
        let json = serde_json::to_string(&ix).unwrap();
        assert!(json.contains("is_signer"));
    }
}
