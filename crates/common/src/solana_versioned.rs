//! Solana versioned transactions — v0 with Address Lookup Tables.

use serde::{Deserialize, Serialize};

/// Solana transaction versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxVersion {
    Legacy,
    V0,
}

/// Address Lookup Table account reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressLookupTable {
    pub account_key: String,
    pub writable_indexes: Vec<u8>,
    pub readonly_indexes: Vec<u8>,
}

/// Versioned message header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedMessageHeader {
    pub num_required_signatures: u8,
    pub num_readonly_signed: u8,
    pub num_readonly_unsigned: u8,
}

/// Build a v0 transaction message prefix byte.
pub fn v0_prefix() -> u8 {
    0x80 // bit 7 set = versioned, version 0
}

/// Estimate transaction size for v0 with ALTs.
pub fn estimate_v0_tx_size(
    num_signatures: u8,
    num_account_keys: u8,
    num_instructions: u8,
    avg_instruction_data_len: u16,
    num_alt_accounts: u8,
) -> usize {
    // Signatures: 64 bytes each
    let sigs = num_signatures as usize * 64 + 1; // +1 for compact array len
    // Message: header(3) + account_keys(num*32) + recent_blockhash(32) + instructions
    let header = 3;
    let keys = 1 + num_account_keys as usize * 32;
    let blockhash = 32;
    let instructions = 1 + num_instructions as usize * (1 + 2 + 2 + avg_instruction_data_len as usize);
    // ALT: 1 byte count + per table (32 + 1 + indexes + 1 + indexes)
    let alt = 1 + num_alt_accounts as usize * 36;
    // Version prefix
    let version = 1;

    version + sigs + header + keys + blockhash + instructions + alt
}

/// Max transaction size on Solana.
pub const MAX_TX_SIZE: usize = 1232;

/// Check if a transaction fits within Solana's size limit.
pub fn fits_in_tx(size: usize) -> bool {
    size <= MAX_TX_SIZE
}

/// Solana transaction priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityLevel {
    None,
    Low,       // ~1,000 micro-lamports per CU
    Medium,    // ~10,000
    High,      // ~100,000
    VeryHigh,  // ~1,000,000
}

impl PriorityLevel {
    pub fn micro_lamports_per_cu(&self) -> u64 {
        match self {
            Self::None => 0,
            Self::Low => 1_000,
            Self::Medium => 10_000,
            Self::High => 100_000,
            Self::VeryHigh => 1_000_000,
        }
    }

    /// Estimate priority fee in SOL for given compute units.
    pub fn fee_sol(&self, compute_units: u32) -> f64 {
        let micro_lamports = self.micro_lamports_per_cu() as f64 * compute_units as f64;
        micro_lamports / 1_000_000.0 / 1_000_000_000.0 // micro-lamports -> lamports -> SOL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_prefix_byte() {
        assert_eq!(v0_prefix(), 0x80);
        assert!(v0_prefix() & 0x80 != 0); // bit 7 set
    }

    #[test]
    fn estimate_simple_transfer() {
        let size = estimate_v0_tx_size(1, 3, 1, 12, 0);
        assert!(size < MAX_TX_SIZE);
        assert!(size > 100);
    }

    #[test]
    fn estimate_complex_tx() {
        let size = estimate_v0_tx_size(1, 10, 5, 100, 2);
        assert!(size > 500);
    }

    #[test]
    fn fits_check() {
        assert!(fits_in_tx(500));
        assert!(fits_in_tx(1232));
        assert!(!fits_in_tx(1233));
    }

    #[test]
    fn priority_ordering() {
        assert!(PriorityLevel::None < PriorityLevel::Low);
        assert!(PriorityLevel::Low < PriorityLevel::Medium);
        assert!(PriorityLevel::High < PriorityLevel::VeryHigh);
    }

    #[test]
    fn priority_micro_lamports() {
        assert_eq!(PriorityLevel::None.micro_lamports_per_cu(), 0);
        assert_eq!(PriorityLevel::Low.micro_lamports_per_cu(), 1_000);
        assert_eq!(PriorityLevel::VeryHigh.micro_lamports_per_cu(), 1_000_000);
    }

    #[test]
    fn priority_fee_calculation() {
        let fee = PriorityLevel::Medium.fee_sol(200_000);
        assert!(fee > 0.0);
        assert!(fee < 0.01); // should be tiny
    }

    #[test]
    fn tx_version_serialization() {
        assert_eq!(serde_json::to_string(&TxVersion::V0).unwrap(), r#""v0""#);
        assert_eq!(serde_json::to_string(&TxVersion::Legacy).unwrap(), r#""legacy""#);
    }

    #[test]
    fn alt_structure() {
        let alt = AddressLookupTable {
            account_key: "ALTabc123".into(),
            writable_indexes: vec![0, 1, 2],
            readonly_indexes: vec![3, 4],
        };
        let json = serde_json::to_string(&alt).unwrap();
        assert!(json.contains("writable_indexes"));
    }

    #[test]
    fn message_header() {
        let header = VersionedMessageHeader {
            num_required_signatures: 1,
            num_readonly_signed: 0,
            num_readonly_unsigned: 1,
        };
        assert_eq!(header.num_required_signatures, 1);
    }

    #[test]
    fn priority_level_serialization() {
        assert_eq!(serde_json::to_string(&PriorityLevel::VeryHigh).unwrap(), r#""very_high""#);
    }
}
