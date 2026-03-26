//! Gas optimization — Multicall, batch operations, ERC-20 Permit, flashloans.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Multicall (batch multiple calls in one tx)
// ---------------------------------------------------------------------------

/// Multicall3 contract (deployed on all EVM chains at same address).
pub const MULTICALL3_ADDRESS: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

/// A single call in a multicall batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulticallCall {
    pub target: String,
    pub allow_failure: bool,
    pub calldata: Vec<u8>,
}

/// Encode Multicall3 `aggregate3` calldata.
///
/// Selector: 0x82ad56cb
pub fn encode_multicall3(calls: &[MulticallCall]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100 + calls.len() * 100);
    data.extend_from_slice(&[0x82, 0xad, 0x56, 0xcb]);

    // offset to calls array
    let mut offset = [0u8; 32];
    offset[31] = 0x20;
    data.extend_from_slice(&offset);

    // array length
    let mut len = [0u8; 32];
    len[28..32].copy_from_slice(&(calls.len() as u32).to_be_bytes());
    data.extend_from_slice(&len);

    // Each call struct is (address, bool, bytes)
    // For simplicity, encode the count — real ABI encoding is more complex
    for call in calls {
        let target_bytes = hex::decode(call.target.trim_start_matches("0x")).unwrap_or_default();
        let mut target_slot = [0u8; 32];
        if target_bytes.len() == 20 {
            target_slot[12..].copy_from_slice(&target_bytes);
        }
        data.extend_from_slice(&target_slot);

        let mut bool_slot = [0u8; 32];
        bool_slot[31] = call.allow_failure as u8;
        data.extend_from_slice(&bool_slot);

        // calldata length + data
        let mut cd_len = [0u8; 32];
        cd_len[28..32].copy_from_slice(&(call.calldata.len() as u32).to_be_bytes());
        data.extend_from_slice(&cd_len);
        data.extend_from_slice(&call.calldata);
        // pad to 32-byte boundary
        let pad = (32 - (call.calldata.len() % 32)) % 32;
        data.extend(vec![0u8; pad]);
    }
    data
}

// ---------------------------------------------------------------------------
// ERC-20 Permit (gasless approvals — EIP-2612)
// ---------------------------------------------------------------------------

/// ERC-20 Permit parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermitParams {
    pub owner: [u8; 20],
    pub spender: [u8; 20],
    pub value: [u8; 32],
    pub deadline: [u8; 32],
    pub v: u8,
    pub r: [u8; 32],
    pub s: [u8; 32],
}

/// Encode ERC-20 `permit(address,address,uint256,uint256,uint8,bytes32,bytes32)`.
///
/// Selector: 0xd505accf
pub fn encode_permit(params: &PermitParams) -> Vec<u8> {
    let mut data = Vec::with_capacity(228);
    data.extend_from_slice(&[0xd5, 0x05, 0xac, 0xcf]);

    // owner
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(&params.owner);
    data.extend_from_slice(&slot);
    // spender
    slot = [0u8; 32];
    slot[12..].copy_from_slice(&params.spender);
    data.extend_from_slice(&slot);
    // value
    data.extend_from_slice(&params.value);
    // deadline
    data.extend_from_slice(&params.deadline);
    // v
    slot = [0u8; 32];
    slot[31] = params.v;
    data.extend_from_slice(&slot);
    // r
    data.extend_from_slice(&params.r);
    // s
    data.extend_from_slice(&params.s);
    data
}

/// Tokens supporting EIP-2612 Permit.
pub fn permit_supported_tokens() -> Vec<(&'static str, &'static str)> {
    vec![
        ("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"),
        ("DAI", "0x6B175474E89094C44Da98b954EedeAC495271d0F"),
        ("UNI", "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984"),
        ("AAVE", "0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9"),
        ("stETH", "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84"),
        ("rETH", "0xae78736Cd615f374D3085123A210448E74Fc6393"),
        ("COMP", "0xc00e94Cb662C3520282E6f5717214004A7f26888"),
        ("ARB", "0xB50721BCf8d664c30412Cfbc6cf7a15145234ad1"),
    ]
}

// ---------------------------------------------------------------------------
// Flash loans (Aave V3)
// ---------------------------------------------------------------------------

/// Aave V3 Pool contract.
pub const AAVE_V3_POOL: &str = "0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2";

/// Flash loan parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashLoanParams {
    pub receiver: String,
    pub assets: Vec<String>,
    pub amounts: Vec<Decimal>,
    pub modes: Vec<u8>, // 0 = no debt, 1 = stable, 2 = variable
    pub on_behalf_of: String,
    pub params: String, // encoded callback data
}

/// Aave V3 flash loan fee (0.05% = 5 bps).
pub const AAVE_FLASH_LOAN_FEE_BPS: u32 = 5;

/// Calculate flash loan fee.
pub fn flash_loan_fee(amount: Decimal) -> Decimal {
    amount * Decimal::new(AAVE_FLASH_LOAN_FEE_BPS as i64, 0) / Decimal::new(10_000, 0)
}

/// Encode `flashLoan` calldata.
///
/// Selector: 0xab9c4b5d
pub fn encode_flash_loan_selector() -> [u8; 4] {
    [0xab, 0x9c, 0x4b, 0x5d]
}

// ---------------------------------------------------------------------------
// Gas estimation helpers
// ---------------------------------------------------------------------------

/// Estimate gas savings from batching multiple operations.
pub fn batch_gas_savings(individual_gas: &[u64]) -> GasSavingsEstimate {
    let total_individual: u64 = individual_gas.iter().sum();
    // Multicall overhead: ~21000 base + ~2500 per call (vs 21000 per individual tx)
    let multicall_gas = 21_000 + individual_gas.iter().map(|g| g.saturating_sub(21_000) + 2_500).sum::<u64>();
    let saved = total_individual.saturating_sub(multicall_gas);
    let savings_pct = if total_individual > 0 {
        (saved as f64 / total_individual as f64 * 100.0) as u32
    } else { 0 };

    GasSavingsEstimate {
        individual_total_gas: total_individual,
        batched_gas: multicall_gas,
        gas_saved: saved,
        savings_percent: savings_pct,
        tx_count: individual_gas.len() as u32,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasSavingsEstimate {
    pub individual_total_gas: u64,
    pub batched_gas: u64,
    pub gas_saved: u64,
    pub savings_percent: u32,
    pub tx_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn multicall3_selector() {
        let data = encode_multicall3(&[]);
        assert_eq!(&data[..4], &[0x82, 0xad, 0x56, 0xcb]);
    }

    #[test]
    fn multicall3_with_calls() {
        let calls = vec![
            MulticallCall {
                target: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into(),
                allow_failure: false,
                calldata: vec![0x70, 0xa0, 0x82, 0x31], // balanceOf
            },
        ];
        let data = encode_multicall3(&calls);
        assert!(data.len() > 100);
    }

    #[test]
    fn permit_calldata() {
        let params = PermitParams {
            owner: [0xAA; 20], spender: [0xBB; 20],
            value: [0u8; 32], deadline: [0u8; 32],
            v: 27, r: [0u8; 32], s: [0u8; 32],
        };
        let data = encode_permit(&params);
        assert_eq!(&data[..4], &[0xd5, 0x05, 0xac, 0xcf]);
        assert_eq!(data.len(), 228);
    }

    #[test]
    fn permit_supported_list() {
        let tokens = permit_supported_tokens();
        assert!(tokens.len() >= 8);
        assert!(tokens.iter().any(|(s, _)| *s == "USDC"));
    }

    #[test]
    fn flash_loan_fee_calculation() {
        assert_eq!(flash_loan_fee(dec!(1000000)), dec!(500)); // 0.05% of 1M = 500
        assert_eq!(flash_loan_fee(dec!(100)), dec!(0.05));
    }

    #[test]
    fn flash_loan_selector() {
        assert_eq!(encode_flash_loan_selector(), [0xab, 0x9c, 0x4b, 0x5d]);
    }

    #[test]
    fn gas_savings_estimate() {
        let individual = vec![65_000, 65_000, 65_000]; // 3 ERC-20 transfers
        let est = batch_gas_savings(&individual);
        assert_eq!(est.tx_count, 3);
        assert!(est.gas_saved > 0);
        assert!(est.savings_percent > 0);
        assert!(est.batched_gas < est.individual_total_gas);
    }

    #[test]
    fn gas_savings_single_tx() {
        let individual = vec![21_000];
        let est = batch_gas_savings(&individual);
        assert_eq!(est.tx_count, 1);
        // Single tx can't really be batched effectively
    }

    #[test]
    fn gas_savings_many_txs() {
        let individual = vec![65_000; 10]; // 10 ERC-20 transfers
        let est = batch_gas_savings(&individual);
        assert!(est.savings_percent >= 20); // should save 20%+
    }

    #[test]
    fn multicall3_address_constant() {
        assert!(MULTICALL3_ADDRESS.starts_with("0x"));
        assert_eq!(MULTICALL3_ADDRESS.len(), 42);
    }
}
