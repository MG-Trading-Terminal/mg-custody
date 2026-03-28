//! Staking execution — wires existing calldata builders into an execution flow.
//!
//! Provides `StakingExecutor` which constructs ready-to-submit `StakingOperation`
//! values for ETH beacon chain deposits and SOL validator delegation.
//!
//! All monetary amounts are `rust_decimal::Decimal`. Amount validation is strict:
//! - ETH: exactly 32 ETH per validator slot.
//! - SOL: minimum 0.01 SOL (10_000_000 lamports, rent-exempt).
//!
//! References:
//! - ETH deposit contract: https://etherscan.io/address/0x00000000219ab540356cBB839Cbe05303d7705Fa
//! - Solana stake program: https://docs.solana.com/staking

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Type of staking operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingOpType {
    /// Lock funds with a validator / deposit contract.
    Stake,
    /// Initiate withdrawal / unbonding.
    Unstake,
    /// Claim accumulated staking rewards.
    ClaimRewards,
}

/// A staking operation ready for submission to the signing pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingOperation {
    pub id: Uuid,
    pub chain: Chain,
    pub operation_type: StakingOpType,
    /// Amount in native display units (ETH, SOL — not wei or lamports).
    pub amount: Decimal,
    /// Validator address / vote account / deposit contract.
    pub validator: String,
    /// ABI-encoded or program-specific calldata ready to submit.
    pub calldata: Vec<u8>,
    pub status: StakingOpStatus,
    pub created_at: DateTime<Utc>,
}

/// Lifecycle status of a staking operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StakingOpStatus {
    /// Built locally, not yet submitted to signing.
    Prepared,
    /// Submitted to the signing pipeline.
    Submitted,
    /// Included in a block.
    Confirmed,
    /// Submission failed.
    Failed,
}

// ---------------------------------------------------------------------------
// StakingExecutor
// ---------------------------------------------------------------------------

/// Entry point for constructing staking operations.
///
/// Each method validates parameters, builds calldata, and returns a
/// `StakingOperation` ready to be handed to the signing pipeline.
/// No network calls are made — this is pure calldata construction.
pub struct StakingExecutor;

impl StakingExecutor {
    /// Construct an ETH beacon chain validator deposit operation.
    ///
    /// Validates that `amount` is exactly 32 ETH (the beacon chain minimum per
    /// validator slot). Delegates calldata construction to
    /// `custody_signing::staking::build_eth_deposit_calldata`.
    ///
    /// # Arguments
    /// - `amount` — must be exactly `dec!(32)` ETH.
    /// - `withdrawal_credentials` — 32-byte hex string (`0x` + 64 hex chars).
    /// - `validator_pubkey` — 48-byte hex BLS12-381 pubkey (`0x` + 96 hex chars).
    /// - `signature` — 96-byte hex BLS signature over deposit message.
    /// - `deposit_data_root` — 32-byte hex DepositData SSZ hash root.
    pub fn create_eth_stake(
        amount: Decimal,
        withdrawal_credentials: &str,
        validator_pubkey: &str,
        signature: &str,
        deposit_data_root: &str,
    ) -> Result<StakingOperation> {
        if amount != dec!(32) {
            bail!(
                "ETH validator deposit requires exactly 32 ETH, got {amount}. \
                 Each validator slot is a fixed 32 ETH deposit."
            );
        }

        let pubkey_bytes = decode_hex_fixed::<48>(validator_pubkey)
            .context("validator_pubkey must be 0x-prefixed 96-char hex (48 bytes)")?;
        let cred_bytes = decode_hex_fixed::<32>(withdrawal_credentials)
            .context("withdrawal_credentials must be 0x-prefixed 64-char hex (32 bytes)")?;
        let sig_bytes = decode_hex_fixed::<96>(signature)
            .context("signature must be 0x-prefixed 192-char hex (96 bytes)")?;
        let root_bytes = decode_hex_fixed::<32>(deposit_data_root)
            .context("deposit_data_root must be 0x-prefixed 64-char hex (32 bytes)")?;

        let calldata = crate::staking_contracts::build_eth_deposit_calldata_full(
            &pubkey_bytes,
            &cred_bytes,
            &sig_bytes,
            &root_bytes,
        );

        Ok(StakingOperation {
            id: Uuid::new_v4(),
            chain: Chain::Ethereum,
            operation_type: StakingOpType::Stake,
            amount,
            validator: crate::staking_contracts::ETH_DEPOSIT_CONTRACT.to_string(),
            calldata,
            status: StakingOpStatus::Prepared,
            created_at: Utc::now(),
        })
    }

    /// Construct a SOL validator delegation operation.
    ///
    /// Validates that `amount` is at least 0.01 SOL (rent-exempt minimum for a
    /// stake account). Delegates instruction encoding to the signing crate.
    ///
    /// # Arguments
    /// - `amount` — in SOL (not lamports). Minimum 0.01 SOL.
    /// - `validator` — base58-encoded vote account address (32 bytes).
    /// - `from_pubkey` — 32-byte payer / staker pubkey (hex or base58).
    /// - `stake_account` — fresh stake account keypair pubkey (hex or base58).
    pub fn create_sol_delegation(
        amount: Decimal,
        validator: &str,
        from_pubkey_bytes: &[u8; 32],
        stake_account_bytes: &[u8; 32],
    ) -> Result<StakingOperation> {
        let min_sol = dec!(0.01);
        if amount < min_sol {
            bail!(
                "SOL delegation minimum is 0.01 SOL, got {amount}. \
                 Stake accounts require a rent-exempt balance."
            );
        }

        // Convert SOL → lamports (1 SOL = 1_000_000_000 lamports)
        let lamports_decimal = amount * dec!(1_000_000_000);
        let lamports = lamports_decimal
            .to_string()
            .split('.')
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .context("SOL amount too large to represent as lamports")?;

        // Decode vote account
        let vote_bytes = bs58_decode_32(validator)
            .with_context(|| format!("validator must be a valid base58 vote account: {validator}"))?;

        let calldata = crate::staking_contracts::build_sol_stake_instructions(
            from_pubkey_bytes,
            stake_account_bytes,
            &vote_bytes,
            lamports,
        );

        Ok(StakingOperation {
            id: Uuid::new_v4(),
            chain: Chain::Solana,
            operation_type: StakingOpType::Stake,
            amount,
            validator: validator.to_string(),
            calldata,
            status: StakingOpStatus::Prepared,
            created_at: Utc::now(),
        })
    }

    /// Construct an unstake operation for an existing staking position.
    ///
    /// Returns a `StakingOperation` with empty calldata — the actual
    /// chain-specific unstake instruction is built by the withdrawal pipeline
    /// once the position record is retrieved from the database.
    pub fn create_unstake(position_id: Uuid, chain: Chain) -> Result<StakingOperation> {
        validate_chain_supports_staking(chain)?;

        Ok(StakingOperation {
            id: Uuid::new_v4(),
            chain,
            operation_type: StakingOpType::Unstake,
            amount: Decimal::ZERO, // resolved from position record
            validator: position_id.to_string(),
            calldata: Vec::new(),
            status: StakingOpStatus::Prepared,
            created_at: Utc::now(),
        })
    }

    /// Construct a claim-rewards operation for an existing staking position.
    pub fn create_claim_rewards(position_id: Uuid, chain: Chain) -> Result<StakingOperation> {
        validate_chain_supports_staking(chain)?;

        Ok(StakingOperation {
            id: Uuid::new_v4(),
            chain,
            operation_type: StakingOpType::ClaimRewards,
            amount: Decimal::ZERO, // resolved at claim time
            validator: position_id.to_string(),
            calldata: Vec::new(),
            status: StakingOpStatus::Prepared,
            created_at: Utc::now(),
        })
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn validate_chain_supports_staking(chain: Chain) -> Result<()> {
    match chain {
        Chain::Ethereum | Chain::Solana => Ok(()),
        other => bail!("staking is not supported for chain={other}"),
    }
}

/// Decode a 0x-prefixed hex string into a fixed-size byte array.
fn decode_hex_fixed<const N: usize>(hex: &str) -> Result<[u8; N]> {
    let stripped = hex.strip_prefix("0x").unwrap_or(hex);
    if stripped.len() != N * 2 {
        bail!(
            "expected {} hex chars ({}  bytes), got {} chars",
            N * 2,
            N,
            stripped.len()
        );
    }
    let bytes = (0..N)
        .map(|i| {
            u8::from_str_radix(&stripped[i * 2..i * 2 + 2], 16)
                .map_err(|_| anyhow::anyhow!("invalid hex at position {}", i * 2))
        })
        .collect::<Result<Vec<u8>>>()?;
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Decode a base58-encoded string into a 32-byte array.
fn bs58_decode_32(s: &str) -> Result<[u8; 32]> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| anyhow::anyhow!("base58 decode failed: {e}"))?;
    if bytes.len() != 32 {
        bail!("expected 32 bytes from base58 decode, got {}", bytes.len());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // Test vectors: all hex strings are zero-filled for reproducibility.

    fn eth_pubkey_hex() -> String {
        format!("0x{}", "aa".repeat(48))
    }
    fn eth_cred_hex() -> String {
        format!("0x{}", "bb".repeat(32))
    }
    fn eth_sig_hex() -> String {
        format!("0x{}", "cc".repeat(96))
    }
    fn eth_root_hex() -> String {
        format!("0x{}", "dd".repeat(32))
    }

    // --- ETH staking ---

    #[test]
    fn eth_stake_32_eth_succeeds() {
        let op = StakingExecutor::create_eth_stake(
            dec!(32),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap();
        assert_eq!(op.chain, Chain::Ethereum);
        assert_eq!(op.operation_type, StakingOpType::Stake);
        assert_eq!(op.amount, dec!(32));
        assert_eq!(op.status, StakingOpStatus::Prepared);
        // Calldata must start with deposit() function selector 0x22895118
        assert_eq!(&op.calldata[0..4], &[0x22, 0x89, 0x51, 0x18]);
    }

    #[test]
    fn eth_stake_not_32_rejected() {
        let err = StakingExecutor::create_eth_stake(
            dec!(31),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("32 ETH"), "error: {err}");
    }

    #[test]
    fn eth_stake_33_rejected() {
        let err = StakingExecutor::create_eth_stake(
            dec!(33),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("32 ETH"), "error: {err}");
    }

    #[test]
    fn eth_stake_zero_rejected() {
        let err = StakingExecutor::create_eth_stake(
            dec!(0),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("32 ETH"), "error: {err}");
    }

    #[test]
    fn eth_stake_bad_pubkey_hex_rejected() {
        let err = StakingExecutor::create_eth_stake(
            dec!(32),
            &eth_cred_hex(),
            "0xdeadbeef", // too short
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("validator_pubkey"),
            "error: {err}"
        );
    }

    #[test]
    fn eth_stake_calldata_length() {
        let op = StakingExecutor::create_eth_stake(
            dec!(32),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap();
        // Minimal: 4 (selector) + 4*32 (offsets+root) + 32+64 (pubkey) + 32+32 (creds) + 32+128 (sig) = 420
        assert!(
            op.calldata.len() >= 420,
            "calldata too short: {}",
            op.calldata.len()
        );
    }

    #[test]
    fn eth_stake_unique_ids() {
        let op1 = StakingExecutor::create_eth_stake(
            dec!(32),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap();
        let op2 = StakingExecutor::create_eth_stake(
            dec!(32),
            &eth_cred_hex(),
            &eth_pubkey_hex(),
            &eth_sig_hex(),
            &eth_root_hex(),
        )
        .unwrap();
        assert_ne!(op1.id, op2.id, "each operation must have a unique ID");
    }

    // --- SOL delegation ---

    // Solana system program pubkey (all zeros) — just for test
    fn test_from() -> [u8; 32] { [0x01u8; 32] }
    fn test_stake_account() -> [u8; 32] { [0x02u8; 32] }
    // Marinade vote account (valid base58, 32 bytes)
    const MARINADE_VOTE: &str = "B1aLzaNMeFVAyQ6f3XbbUyKcH2YPHu2fqiEagmiF23VR";

    #[test]
    fn sol_delegation_1_sol_succeeds() {
        let op = StakingExecutor::create_sol_delegation(
            dec!(1),
            MARINADE_VOTE,
            &test_from(),
            &test_stake_account(),
        )
        .unwrap();
        assert_eq!(op.chain, Chain::Solana);
        assert_eq!(op.operation_type, StakingOpType::Stake);
        assert_eq!(op.amount, dec!(1));
        assert_eq!(op.validator, MARINADE_VOTE);
        assert!(!op.calldata.is_empty());
    }

    #[test]
    fn sol_delegation_minimum_0_01_sol_succeeds() {
        let op = StakingExecutor::create_sol_delegation(
            dec!(0.01),
            MARINADE_VOTE,
            &test_from(),
            &test_stake_account(),
        )
        .unwrap();
        assert_eq!(op.amount, dec!(0.01));
    }

    #[test]
    fn sol_delegation_below_minimum_rejected() {
        let err = StakingExecutor::create_sol_delegation(
            dec!(0.001),
            MARINADE_VOTE,
            &test_from(),
            &test_stake_account(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("0.01 SOL"), "error: {err}");
    }

    #[test]
    fn sol_delegation_zero_rejected() {
        let err = StakingExecutor::create_sol_delegation(
            dec!(0),
            MARINADE_VOTE,
            &test_from(),
            &test_stake_account(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("0.01 SOL"), "error: {err}");
    }

    // --- Unstake ---

    #[test]
    fn create_unstake_ethereum() {
        let pid = Uuid::new_v4();
        let op = StakingExecutor::create_unstake(pid, Chain::Ethereum).unwrap();
        assert_eq!(op.operation_type, StakingOpType::Unstake);
        assert_eq!(op.chain, Chain::Ethereum);
        assert_eq!(op.validator, pid.to_string());
    }

    #[test]
    fn create_unstake_solana() {
        let pid = Uuid::new_v4();
        let op = StakingExecutor::create_unstake(pid, Chain::Solana).unwrap();
        assert_eq!(op.chain, Chain::Solana);
    }

    #[test]
    fn create_unstake_unsupported_chain_rejected() {
        let err = StakingExecutor::create_unstake(Uuid::new_v4(), Chain::Bitcoin).unwrap_err();
        assert!(err.to_string().contains("bitcoin"), "error: {err}");
    }

    // --- Claim rewards ---

    #[test]
    fn create_claim_rewards_ethereum() {
        let pid = Uuid::new_v4();
        let op = StakingExecutor::create_claim_rewards(pid, Chain::Ethereum).unwrap();
        assert_eq!(op.operation_type, StakingOpType::ClaimRewards);
        assert_eq!(op.chain, Chain::Ethereum);
    }

    #[test]
    fn create_claim_rewards_tron_rejected() {
        let err =
            StakingExecutor::create_claim_rewards(Uuid::new_v4(), Chain::Tron).unwrap_err();
        assert!(err.to_string().contains("tron"), "error: {err}");
    }

    // --- Hex decoder ---

    #[test]
    fn decode_hex_fixed_correct() {
        let result = decode_hex_fixed::<4>("0xdeadbeef").unwrap();
        assert_eq!(result, [0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn decode_hex_fixed_wrong_length() {
        assert!(decode_hex_fixed::<32>("0xdeadbeef").is_err());
    }

    #[test]
    fn decode_hex_without_prefix() {
        let result = decode_hex_fixed::<2>("aabb").unwrap();
        assert_eq!(result, [0xaa, 0xbb]);
    }
}
