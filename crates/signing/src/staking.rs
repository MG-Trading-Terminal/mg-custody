//! Staking transaction construction.
//!
//! Builds staking transactions for ETH (beacon chain deposit) and SOL (delegation).

use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// ETH Beacon Chain deposit contract address (mainnet).
pub const BEACON_DEPOSIT_CONTRACT: &str = "0x00000000219ab540356cBB839Cbe05303d7705Fa";

/// Build an ETH beacon chain deposit transaction.
///
/// Constructs calldata for the deposit contract's `deposit()` function.
/// Requires exactly 32 ETH.
///
/// Reference: https://etherscan.io/address/0x00000000219ab540356cBB839Cbe05303d7705Fa
pub fn build_eth_deposit_calldata(
    pubkey: &[u8; 48],           // BLS12-381 validator pubkey
    withdrawal_credentials: &[u8; 32],
    signature: &[u8; 96],        // BLS signature over deposit message
    deposit_data_root: &[u8; 32],
) -> Vec<u8> {
    // deposit(bytes pubkey, bytes withdrawal_credentials, bytes signature, bytes32 deposit_data_root)
    // function selector: 0x22895118
    let selector: [u8; 4] = [0x22, 0x89, 0x51, 0x18];

    let mut calldata = Vec::with_capacity(4 + 32 * 10);
    calldata.extend_from_slice(&selector);

    // ABI encoding with dynamic types (pubkey, withdrawal_credentials, signature are bytes)
    // Offsets: pubkey=128, withdrawal_credentials=192, signature=256, deposit_data_root=inline
    calldata.extend_from_slice(&encode_u256(128)); // offset to pubkey
    calldata.extend_from_slice(&encode_u256(192)); // offset to withdrawal_credentials
    calldata.extend_from_slice(&encode_u256(288)); // offset to signature
    calldata.extend_from_slice(deposit_data_root); // bytes32, inline

    // pubkey (48 bytes, padded to 64)
    calldata.extend_from_slice(&encode_u256(48)); // length
    calldata.extend_from_slice(pubkey);
    calldata.extend_from_slice(&[0u8; 16]); // pad to 64

    // withdrawal_credentials (32 bytes)
    calldata.extend_from_slice(&encode_u256(32)); // length
    calldata.extend_from_slice(withdrawal_credentials);

    // signature (96 bytes, padded to 128)
    calldata.extend_from_slice(&encode_u256(96)); // length
    calldata.extend_from_slice(signature);
    calldata.extend_from_slice(&[0u8; 32]); // pad to 128

    calldata
}

/// Validate ETH staking amount (must be exactly 32 ETH).
pub fn validate_eth_stake_amount(amount: Decimal) -> Result<()> {
    if amount != dec!(32) {
        anyhow::bail!("ETH validator stake must be exactly 32 ETH, got {amount}");
    }
    Ok(())
}

/// Build a SOL stake account creation + delegation instruction set.
///
/// Returns the serialized instruction data for:
/// 1. SystemProgram::CreateAccount (rent-exempt stake account)
/// 2. StakeProgram::Initialize (set staker + withdrawer authority)
/// 3. StakeProgram::DelegateStake (delegate to vote account)
pub fn build_sol_delegation_instructions(
    from_pubkey: &[u8; 32],
    stake_account: &[u8; 32],
    vote_account: &[u8; 32],
    lamports: u64,
) -> Vec<u8> {
    // Solana stake program ID: Stake11111111111111111111111111111111111111
    let stake_program = bs58::decode("Stake11111111111111111111111111111111111111")
        .into_vec().unwrap_or_else(|_| vec![0u8; 32]);

    let mut instructions = Vec::new();

    // Instruction 1: CreateAccount (SystemProgram)
    // program_id: 11111111111111111111111111111111 (System)
    // accounts: [from (signer, writable), stake_account (signer, writable)]
    // data: CreateAccount { lamports, space: 200, owner: Stake program }
    instructions.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CreateAccount instruction index
    instructions.extend_from_slice(&lamports.to_le_bytes());
    instructions.extend_from_slice(&200u64.to_le_bytes()); // stake account space
    instructions.extend_from_slice(&stake_program[..32.min(stake_program.len())]);

    // Instruction 2: Initialize
    // data: Initialize { authorized: Authorized { staker, withdrawer } }
    instructions.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Initialize
    instructions.extend_from_slice(from_pubkey); // staker = our hot wallet
    instructions.extend_from_slice(from_pubkey); // withdrawer = our hot wallet

    // Instruction 3: DelegateStake
    // accounts: [stake_account, vote_account, clock, stake_history, config, staker]
    instructions.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]); // DelegateStake
    instructions.extend_from_slice(stake_account);
    instructions.extend_from_slice(vote_account);

    instructions
}

/// Validate SOL delegation amount.
pub fn validate_sol_delegation(lamports: u64) -> Result<()> {
    // Minimum stake on Solana is ~0.01 SOL (rent-exempt minimum)
    let min_lamports = 10_000_000; // 0.01 SOL
    if lamports < min_lamports {
        anyhow::bail!("SOL delegation minimum is 0.01 SOL, got {} lamports", lamports);
    }
    Ok(())
}

/// Encode a u256 as 32 bytes (big-endian, left-padded).
fn encode_u256(value: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..32].copy_from_slice(&value.to_be_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eth_deposit_calldata_structure() {
        let pubkey = [0xAA; 48];
        let withdrawal_creds = [0xBB; 32];
        let signature = [0xCC; 96];
        let data_root = [0xDD; 32];

        let calldata = build_eth_deposit_calldata(&pubkey, &withdrawal_creds, &signature, &data_root);

        // Starts with function selector
        assert_eq!(&calldata[0..4], &[0x22, 0x89, 0x51, 0x18]);
        // Has reasonable length
        assert!(calldata.len() > 200);
    }

    #[test]
    fn validate_eth_stake_32() {
        assert!(validate_eth_stake_amount(dec!(32)).is_ok());
        assert!(validate_eth_stake_amount(dec!(31)).is_err());
        assert!(validate_eth_stake_amount(dec!(33)).is_err());
        assert!(validate_eth_stake_amount(dec!(0)).is_err());
    }

    #[test]
    fn sol_delegation_instructions() {
        let from = [1u8; 32];
        let stake = [2u8; 32];
        let vote = [3u8; 32];

        let instructions = build_sol_delegation_instructions(&from, &stake, &vote, 1_000_000_000);
        assert!(!instructions.is_empty());
    }

    #[test]
    fn validate_sol_minimum() {
        assert!(validate_sol_delegation(10_000_000).is_ok()); // 0.01 SOL
        assert!(validate_sol_delegation(1_000_000_000).is_ok()); // 1 SOL
        assert!(validate_sol_delegation(1_000).is_err()); // too small
    }

    #[test]
    fn encode_u256_values() {
        let zero = encode_u256(0);
        assert_eq!(zero, [0u8; 32]);

        let one = encode_u256(1);
        assert_eq!(one[31], 1);
        assert_eq!(one[30], 0);
    }
}
