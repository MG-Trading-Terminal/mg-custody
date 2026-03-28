//! Staking contract calldata — Lido, Rocket Pool, Marinade, native staking.
//!
//! Also provides calldata builders for:
//! - ETH beacon chain deposit contract (`build_eth_deposit_calldata_full`)
//! - Solana native stake program instructions (`build_sol_stake_instructions`)
//!
//! References:
//! - ETH deposit contract ABI: https://etherscan.io/address/0x00000000219ab540356cBB839Cbe05303d7705Fa
//! - Solana stake program: https://docs.solana.com/staking/stake-programming

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

// ---------------------------------------------------------------------------
// ETH Beacon Chain deposit contract
// ---------------------------------------------------------------------------

/// ETH Beacon Chain deposit contract address (mainnet).
///
/// Reference: https://etherscan.io/address/0x00000000219ab540356cBB839Cbe05303d7705Fa
pub const ETH_DEPOSIT_CONTRACT: &str = "0x00000000219ab540356cBB839Cbe05303d7705Fa";

/// Build ABI-encoded calldata for the deposit contract's `deposit()` function.
///
/// Function selector: 0x22895118
/// Signature: `deposit(bytes pubkey, bytes withdrawal_credentials, bytes signature, bytes32 deposit_data_root)`
///
/// Reference: https://github.com/ethereum/consensus-specs/blob/dev/solidity_deposit_contract/deposit_contract.sol
pub fn build_eth_deposit_calldata_full(
    pubkey: &[u8; 48],
    withdrawal_credentials: &[u8; 32],
    signature: &[u8; 96],
    deposit_data_root: &[u8; 32],
) -> Vec<u8> {
    // deposit(bytes, bytes, bytes, bytes32)
    // Selector: 0x22895118
    let selector: [u8; 4] = [0x22, 0x89, 0x51, 0x18];

    let mut calldata = Vec::with_capacity(4 + 32 * 14);
    calldata.extend_from_slice(&selector);

    // ABI head section: offsets to dynamic args, then inline bytes32
    // offset to pubkey data  = 128 (4 * 32)
    calldata.extend_from_slice(&encode_u256(128));
    // offset to withdrawal_credentials data = 192 (6 * 32)
    calldata.extend_from_slice(&encode_u256(192));
    // offset to signature data = 288 (9 * 32)
    calldata.extend_from_slice(&encode_u256(288));
    // deposit_data_root is bytes32 — inline
    calldata.extend_from_slice(deposit_data_root);

    // pubkey: length=48, data padded to multiple of 32 (64 bytes)
    calldata.extend_from_slice(&encode_u256(48));
    calldata.extend_from_slice(pubkey);
    calldata.extend_from_slice(&[0u8; 16]); // pad 48 → 64

    // withdrawal_credentials: length=32, already 32 bytes (no padding needed)
    calldata.extend_from_slice(&encode_u256(32));
    calldata.extend_from_slice(withdrawal_credentials);

    // signature: length=96, padded to 128 bytes
    calldata.extend_from_slice(&encode_u256(96));
    calldata.extend_from_slice(signature);
    calldata.extend_from_slice(&[0u8; 32]); // pad 96 → 128

    calldata
}

fn encode_u256(value: u64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[24..32].copy_from_slice(&value.to_be_bytes());
    bytes
}

// ---------------------------------------------------------------------------
// Solana native stake program instructions
// ---------------------------------------------------------------------------

/// Build serialized instruction bytes for creating, initializing, and delegating
/// a native Solana stake account.
///
/// Encodes three instructions:
/// 1. SystemProgram::CreateAccount (create stake account with rent-exempt balance)
/// 2. StakeProgram::Initialize (set staker + withdrawer authority)
/// 3. StakeProgram::DelegateStake (delegate to vote account)
///
/// Reference: https://docs.solana.com/staking/stake-programming
pub fn build_sol_stake_instructions(
    from_pubkey: &[u8; 32],
    stake_account: &[u8; 32],
    vote_account: &[u8; 32],
    lamports: u64,
) -> Vec<u8> {
    // Stake program ID bytes (Stake11111111111111111111111111111111111111)
    let stake_program: [u8; 32] = {
        let decoded = bs58::decode("Stake11111111111111111111111111111111111111")
            .into_vec()
            .unwrap_or_else(|_| vec![0u8; 32]);
        let mut arr = [0u8; 32];
        let len = decoded.len().min(32);
        arr[..len].copy_from_slice(&decoded[..len]);
        arr
    };

    let mut instructions = Vec::new();

    // Instruction 1: SystemProgram::CreateAccount
    // data layout: [instruction_index(4)] [lamports(8)] [space(8)] [owner(32)]
    instructions.extend_from_slice(&0u32.to_le_bytes()); // CreateAccount = 0
    instructions.extend_from_slice(&lamports.to_le_bytes());
    instructions.extend_from_slice(&200u64.to_le_bytes()); // stake account space = 200 bytes
    instructions.extend_from_slice(&stake_program);

    // Instruction 2: StakeProgram::Initialize
    // data layout: [instruction_index(4)] [staker_pubkey(32)] [withdrawer_pubkey(32)]
    instructions.extend_from_slice(&0u32.to_le_bytes()); // Initialize = 0
    instructions.extend_from_slice(from_pubkey); // staker authority = from (hot wallet)
    instructions.extend_from_slice(from_pubkey); // withdrawer authority = from (hot wallet)

    // Instruction 3: StakeProgram::DelegateStake
    // data layout: [instruction_index(4)] [stake_account(32)] [vote_account(32)]
    instructions.extend_from_slice(&2u32.to_le_bytes()); // DelegateStake = 2
    instructions.extend_from_slice(stake_account);
    instructions.extend_from_slice(vote_account);

    instructions
}

// ---------------------------------------------------------------------------
// Lido (Ethereum liquid staking)
// ---------------------------------------------------------------------------

/// Lido stETH contract on Ethereum mainnet.
pub const LIDO_STETH_ADDRESS: &str = "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84";

/// Encode Lido `submit(address)` calldata — stake ETH, get stETH.
///
/// Selector: 0xa1903eab
/// The referral address is where Lido sends referral rewards.
pub fn encode_lido_submit(referral: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0xa1, 0x90, 0x3e, 0xab]);
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(referral);
    data.extend_from_slice(&slot);
    data
}

/// Encode Lido withdrawal `requestWithdrawals(uint256[],address)` calldata.
///
/// Selector: 0xd669d55a (Lido WithdrawalQueue)
pub fn encode_lido_request_withdrawal(amounts: &[[u8; 32]], owner: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100 + amounts.len() * 32);
    data.extend_from_slice(&[0xd6, 0x69, 0xd5, 0x5a]);

    // offset to amounts array (64 = 2 * 32)
    let mut offset_slot = [0u8; 32];
    offset_slot[31] = 0x40;
    data.extend_from_slice(&offset_slot);

    // owner
    let mut owner_slot = [0u8; 32];
    owner_slot[12..].copy_from_slice(owner);
    data.extend_from_slice(&owner_slot);

    // array length
    let mut len_slot = [0u8; 32];
    len_slot[28..32].copy_from_slice(&(amounts.len() as u32).to_be_bytes());
    data.extend_from_slice(&len_slot);

    // array elements
    for amount in amounts {
        data.extend_from_slice(amount);
    }

    data
}

/// Lido WithdrawalQueue contract.
pub const LIDO_WITHDRAWAL_QUEUE: &str = "0x889edC2eDab5f40e902b864aD4d7AdE8E412F9B1";

// ---------------------------------------------------------------------------
// Rocket Pool (Ethereum)
// ---------------------------------------------------------------------------

pub const ROCKET_POOL_DEPOSIT: &str = "0xDD9bc35aE942dF0cD11BbC5D1cd1f42886d5ae86";

/// Encode Rocket Pool `deposit()` — stake ETH, get rETH.
///
/// Selector: 0xd0e30db0
pub fn encode_rocket_pool_deposit() -> Vec<u8> {
    vec![0xd0, 0xe3, 0x0d, 0xb0]
}

/// Encode rETH `burn(uint256)` — unstake rETH back to ETH.
///
/// Selector: 0x42966c68
pub fn encode_reth_burn(amount: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0x42, 0x96, 0x6c, 0x68]);
    data.extend_from_slice(amount);
    data
}

pub const RETH_TOKEN: &str = "0xae78736Cd615f374D3085123A210448E74Fc6393";

// ---------------------------------------------------------------------------
// Marinade (Solana liquid staking)
// ---------------------------------------------------------------------------

/// Marinade Finance program ID on Solana.
pub const MARINADE_PROGRAM: &str = "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD";
pub const MSOL_MINT: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";

/// Marinade stake instruction data.
/// Discriminator: [0xc4, 0x9f, 0x8f, 0x34, 0x3e, 0x4a, 0x41, 0x45]
pub fn marinade_deposit_discriminator() -> [u8; 8] {
    [0xc4, 0x9f, 0x8f, 0x34, 0x3e, 0x4a, 0x41, 0x45]
}

// ---------------------------------------------------------------------------
// Staking validators (known good validators)
// ---------------------------------------------------------------------------

/// A known staking validator/pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub name: String,
    pub chain: Chain,
    pub address: String,
    pub protocol: StakingProtocol,
    pub fee_percent: Decimal,
    pub min_stake: Decimal,
    pub apy_estimate: Decimal,
    pub is_liquid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingProtocol {
    LidoV2,
    RocketPool,
    Marinade,
    NativeEth,
    NativeSol,
    Jito,
    CoinbaseStake,
}

/// Known validators/pools.
pub fn known_validators() -> Vec<ValidatorInfo> {
    use rust_decimal_macros::dec;
    vec![
        ValidatorInfo {
            name: "Lido".into(), chain: Chain::Ethereum,
            address: LIDO_STETH_ADDRESS.into(), protocol: StakingProtocol::LidoV2,
            fee_percent: dec!(10), min_stake: dec!(0.01), apy_estimate: dec!(3.5),
            is_liquid: true,
        },
        ValidatorInfo {
            name: "Rocket Pool".into(), chain: Chain::Ethereum,
            address: ROCKET_POOL_DEPOSIT.into(), protocol: StakingProtocol::RocketPool,
            fee_percent: dec!(15), min_stake: dec!(0.01), apy_estimate: dec!(3.3),
            is_liquid: true,
        },
        ValidatorInfo {
            name: "Coinbase Staked ETH".into(), chain: Chain::Ethereum,
            address: "0xBe9895146f7AF43049ca1c1AE358B0541Ea49704".into(),
            protocol: StakingProtocol::CoinbaseStake,
            fee_percent: dec!(25), min_stake: dec!(0.01), apy_estimate: dec!(3.0),
            is_liquid: true,
        },
        ValidatorInfo {
            name: "Marinade Finance".into(), chain: Chain::Solana,
            address: MARINADE_PROGRAM.into(), protocol: StakingProtocol::Marinade,
            fee_percent: dec!(6), min_stake: dec!(0.01), apy_estimate: dec!(7.0),
            is_liquid: true,
        },
        ValidatorInfo {
            name: "Jito".into(), chain: Chain::Solana,
            address: "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn".into(),
            protocol: StakingProtocol::Jito,
            fee_percent: dec!(4), min_stake: dec!(0.01), apy_estimate: dec!(7.5),
            is_liquid: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn lido_submit_calldata() {
        let referral = [0u8; 20];
        let data = encode_lido_submit(&referral);
        assert_eq!(&data[..4], &[0xa1, 0x90, 0x3e, 0xab]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn lido_withdrawal_calldata() {
        let amounts = vec![[0u8; 32]];
        let owner = [0xAA; 20];
        let data = encode_lido_request_withdrawal(&amounts, &owner);
        assert_eq!(&data[..4], &[0xd6, 0x69, 0xd5, 0x5a]);
        assert!(data.len() > 100);
    }

    #[test]
    fn rocket_pool_deposit_calldata() {
        let data = encode_rocket_pool_deposit();
        assert_eq!(data, &[0xd0, 0xe3, 0x0d, 0xb0]);
    }

    #[test]
    fn reth_burn_calldata() {
        let amount = [0u8; 32];
        let data = encode_reth_burn(&amount);
        assert_eq!(&data[..4], &[0x42, 0x96, 0x6c, 0x68]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn known_validators_count() {
        let validators = known_validators();
        assert!(validators.len() >= 5);
    }

    #[test]
    fn lido_validator_info() {
        let v = known_validators().into_iter().find(|v| v.name == "Lido").unwrap();
        assert_eq!(v.protocol, StakingProtocol::LidoV2);
        assert!(v.is_liquid);
        assert_eq!(v.fee_percent, dec!(10));
    }

    #[test]
    fn marinade_discriminator() {
        let disc = marinade_deposit_discriminator();
        assert_eq!(disc.len(), 8);
        assert_eq!(disc[0], 0xc4);
    }

    #[test]
    fn staking_protocol_serialization() {
        assert_eq!(serde_json::to_string(&StakingProtocol::LidoV2).unwrap(), r#""lido_v2""#);
        assert_eq!(serde_json::to_string(&StakingProtocol::Jito).unwrap(), r#""jito""#);
    }

    #[test]
    fn all_validators_have_apy() {
        for v in known_validators() {
            assert!(v.apy_estimate > Decimal::ZERO, "{} has zero APY", v.name);
        }
    }
}
