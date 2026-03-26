//! Chain-specific utility types — UTXO analysis, EVM gas, deposit forwarding,
//! transaction categories, multi-sig wallets.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Chain;

// ---------------------------------------------------------------------------
// UTXO analysis (Bitcoin)
// ---------------------------------------------------------------------------

/// Statistics about the UTXO set for a wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoAnalysis {
    pub total_utxos: u32,
    pub total_value: Decimal,
    pub largest_utxo: Decimal,
    pub smallest_utxo: Decimal,
    pub dust_utxos: u32,
    pub dust_threshold: Decimal,
    pub consolidation_recommended: bool,
    /// Estimated fee to consolidate all UTXOs into one.
    pub consolidation_fee_estimate: Decimal,
    pub analyzed_at: DateTime<Utc>,
}

/// Dust threshold for Bitcoin (546 satoshis for P2WPKH).
pub const BTC_DUST_LIMIT_SATS: u64 = 546;

/// Analyze a set of UTXO values.
pub fn analyze_utxos(values: &[Decimal], fee_rate_sat_per_vb: Decimal) -> UtxoAnalysis {
    let total = values.len() as u32;
    let total_value: Decimal = values.iter().copied().sum();
    let dust_threshold = Decimal::new(BTC_DUST_LIMIT_SATS as i64, 8); // in BTC

    let largest = values.iter().copied().max().unwrap_or(Decimal::ZERO);
    let smallest = values.iter().copied().min().unwrap_or(Decimal::ZERO);
    let dust_count = values.iter().filter(|v| **v <= dust_threshold).count() as u32;

    // Consolidation: each input ~68 vB, one output ~31 vB, overhead ~10 vB
    let consolidation_vb = total as i64 * 68 + 31 + 10;
    let consolidation_fee = fee_rate_sat_per_vb * Decimal::new(consolidation_vb, 0)
        / Decimal::new(100_000_000, 0); // sat to BTC

    UtxoAnalysis {
        total_utxos: total,
        total_value,
        largest_utxo: largest,
        smallest_utxo: smallest,
        dust_utxos: dust_count,
        dust_threshold,
        consolidation_recommended: total > 50 || dust_count > 10,
        consolidation_fee_estimate: consolidation_fee,
        analyzed_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// EVM gas utilities
// ---------------------------------------------------------------------------

/// EIP-1559 gas calculation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip1559GasParams {
    pub base_fee_gwei: Decimal,
    pub priority_fee_gwei: Decimal,
    pub max_fee_gwei: Decimal,
    pub gas_limit: u64,
    pub estimated_cost_eth: Decimal,
}

/// Calculate EIP-1559 gas parameters.
///
/// `max_fee = 2 * base_fee + priority_fee` (handles 2 consecutive full blocks).
pub fn calculate_eip1559_gas(
    base_fee_gwei: Decimal,
    priority_fee_gwei: Decimal,
    gas_limit: u64,
) -> Eip1559GasParams {
    let max_fee_gwei = base_fee_gwei * Decimal::new(2, 0) + priority_fee_gwei;
    // Cost in ETH = max_fee_gwei * gas_limit / 1e9
    let gwei_to_eth = Decimal::new(1_000_000_000, 0);
    let estimated_cost = max_fee_gwei * Decimal::new(gas_limit as i64, 0) / gwei_to_eth;

    Eip1559GasParams {
        base_fee_gwei,
        priority_fee_gwei,
        max_fee_gwei,
        gas_limit,
        estimated_cost_eth: estimated_cost,
    }
}

/// Standard gas limits for common operations.
pub const GAS_LIMIT_ETH_TRANSFER: u64 = 21_000;
pub const GAS_LIMIT_ERC20_TRANSFER: u64 = 65_000;
pub const GAS_LIMIT_ERC20_APPROVE: u64 = 46_000;
pub const GAS_LIMIT_CONTRACT_DEPLOY: u64 = 1_000_000;
pub const GAS_LIMIT_SWAP: u64 = 200_000;

/// Apply a safety buffer to a gas estimate.
pub fn gas_with_buffer(estimated_gas: u64, buffer_percent: u32) -> u64 {
    estimated_gas + (estimated_gas * buffer_percent as u64) / 100
}

// ---------------------------------------------------------------------------
// Deposit auto-forwarding
// ---------------------------------------------------------------------------

/// Rule for automatically forwarding deposits to a consolidation address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardingRule {
    pub id: Uuid,
    pub chain: Chain,
    pub asset: String,
    /// Minimum amount to trigger forwarding (below this, accumulate).
    pub min_amount: Decimal,
    /// Destination vault or address for forwarded funds.
    pub destination_vault_id: Uuid,
    /// Whether to deduct gas from the forwarded amount.
    pub deduct_gas: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateForwardingRuleRequest {
    pub chain: Chain,
    pub asset: String,
    pub min_amount: Decimal,
    pub destination_vault_id: Uuid,
    #[serde(default = "default_true")]
    pub deduct_gas: bool,
}

fn default_true() -> bool { true }

// ---------------------------------------------------------------------------
// Transaction categories / tags
// ---------------------------------------------------------------------------

/// Predefined transaction categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionCategory {
    Deposit,
    Withdrawal,
    InternalTransfer,
    Consolidation,
    GasRefuel,
    StakingDeposit,
    StakingReward,
    StakingWithdrawal,
    Fee,
    Sweep,
    Unknown,
}

impl std::fmt::Display for TransactionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_default();
        f.write_str(s.trim_matches('"'))
    }
}

/// Custom tag on a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTag {
    pub transaction_id: Uuid,
    pub category: TransactionCategory,
    pub labels: Vec<String>,
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Multi-sig wallet support
// ---------------------------------------------------------------------------

/// Multi-sig wallet configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigConfig {
    pub wallet_id: Uuid,
    pub chain: Chain,
    pub threshold: u32,
    pub total_signers: u32,
    pub signers: Vec<MultisigSigner>,
    pub wallet_type: MultisigWalletType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultisigSigner {
    pub id: Uuid,
    pub name: String,
    /// Hex-encoded public key.
    pub public_key: String,
    pub weight: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultisigWalletType {
    /// Bitcoin P2SH-P2WSH (BIP-67 sorted multi-sig).
    BitcoinP2shP2wsh,
    /// Ethereum Gnosis Safe / multi-sig contract.
    EthereumGnosisSafe,
    /// Solana multi-sig (Squads protocol).
    SolanaSquads,
    /// MPC threshold signature (not actual multi-sig but equivalent).
    MpcThreshold,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // --- UTXO analysis ---

    #[test]
    fn utxo_analysis_basic() {
        let values = vec![dec!(0.1), dec!(0.5), dec!(1.0), dec!(0.00000100)];
        let analysis = analyze_utxos(&values, dec!(20));
        assert_eq!(analysis.total_utxos, 4);
        assert_eq!(analysis.total_value, dec!(1.60000100));
        assert_eq!(analysis.largest_utxo, dec!(1.0));
        assert_eq!(analysis.smallest_utxo, dec!(0.00000100));
        assert_eq!(analysis.dust_utxos, 1); // 100 sats < 546 sats
    }

    #[test]
    fn utxo_consolidation_recommended() {
        let values: Vec<Decimal> = (0..60).map(|_| dec!(0.001)).collect();
        let analysis = analyze_utxos(&values, dec!(10));
        assert!(analysis.consolidation_recommended);
    }

    #[test]
    fn utxo_analysis_empty() {
        let analysis = analyze_utxos(&[], dec!(20));
        assert_eq!(analysis.total_utxos, 0);
        assert_eq!(analysis.total_value, Decimal::ZERO);
    }

    // --- EVM gas ---

    #[test]
    fn eip1559_gas_calculation() {
        let params = calculate_eip1559_gas(dec!(30), dec!(2), GAS_LIMIT_ETH_TRANSFER);
        assert_eq!(params.max_fee_gwei, dec!(62)); // 2*30+2
        assert_eq!(params.gas_limit, 21_000);
        // Cost = 62 gwei * 21000 / 1e9 = 0.001302 ETH
        assert_eq!(params.estimated_cost_eth, dec!(0.001302));
    }

    #[test]
    fn eip1559_erc20_gas() {
        let params = calculate_eip1559_gas(dec!(50), dec!(3), GAS_LIMIT_ERC20_TRANSFER);
        assert_eq!(params.max_fee_gwei, dec!(103));
        assert_eq!(params.gas_limit, 65_000);
    }

    #[test]
    fn gas_buffer_20_percent() {
        assert_eq!(gas_with_buffer(21_000, 20), 25_200);
        assert_eq!(gas_with_buffer(65_000, 10), 71_500);
    }

    #[test]
    fn gas_buffer_zero() {
        assert_eq!(gas_with_buffer(21_000, 0), 21_000);
    }

    // --- Transaction categories ---

    #[test]
    fn category_display() {
        assert_eq!(TransactionCategory::Deposit.to_string(), "deposit");
        assert_eq!(TransactionCategory::GasRefuel.to_string(), "gas_refuel");
        assert_eq!(TransactionCategory::StakingReward.to_string(), "staking_reward");
    }

    #[test]
    fn category_serialization_roundtrip() {
        let cats = vec![
            TransactionCategory::Deposit, TransactionCategory::Withdrawal,
            TransactionCategory::Consolidation, TransactionCategory::Fee,
            TransactionCategory::Sweep,
        ];
        for cat in cats {
            let json = serde_json::to_string(&cat).unwrap();
            let parsed: TransactionCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(cat, parsed);
        }
    }

    // --- Multi-sig ---

    #[test]
    fn multisig_config_2_of_3() {
        let config = MultisigConfig {
            wallet_id: Uuid::nil(),
            chain: Chain::Bitcoin,
            threshold: 2,
            total_signers: 3,
            signers: vec![
                MultisigSigner { id: Uuid::nil(), name: "Alice".into(), public_key: "02aaa".into(), weight: 1 },
                MultisigSigner { id: Uuid::nil(), name: "Bob".into(), public_key: "02bbb".into(), weight: 1 },
                MultisigSigner { id: Uuid::nil(), name: "Charlie".into(), public_key: "02ccc".into(), weight: 1 },
            ],
            wallet_type: MultisigWalletType::BitcoinP2shP2wsh,
            created_at: Utc::now(),
        };
        assert_eq!(config.threshold, 2);
        assert_eq!(config.signers.len(), 3);
    }

    #[test]
    fn multisig_wallet_types() {
        let types = vec![
            MultisigWalletType::BitcoinP2shP2wsh,
            MultisigWalletType::EthereumGnosisSafe,
            MultisigWalletType::SolanaSquads,
            MultisigWalletType::MpcThreshold,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let parsed: MultisigWalletType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, parsed);
        }
    }

    // --- Forwarding rules ---

    #[test]
    fn forwarding_rule_serialization() {
        let rule = ForwardingRule {
            id: Uuid::nil(), chain: Chain::Ethereum, asset: "ETH".into(),
            min_amount: dec!(0.1), destination_vault_id: Uuid::nil(),
            deduct_gas: true, enabled: true, created_at: Utc::now(),
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("deduct_gas"));
    }

    #[test]
    fn forwarding_request_defaults() {
        let json = r#"{"chain":"ethereum","asset":"ETH","min_amount":"0.1","destination_vault_id":"00000000-0000-0000-0000-000000000000"}"#;
        let req: CreateForwardingRuleRequest = serde_json::from_str(json).unwrap();
        assert!(req.deduct_gas); // default true
    }

    // --- Transaction tags ---

    #[test]
    fn transaction_tag_with_metadata() {
        let tag = TransactionTag {
            transaction_id: Uuid::nil(),
            category: TransactionCategory::Consolidation,
            labels: vec!["auto".into(), "weekly".into()],
            metadata: serde_json::json!({"source_count": 15}),
        };
        let json = serde_json::to_string(&tag).unwrap();
        assert!(json.contains("consolidation"));
        assert!(json.contains("source_count"));
    }
}
