//! ERC-4337 Account Abstraction — UserOperation, Bundler, Paymaster.

use serde::{Deserialize, Serialize};

/// ERC-4337 UserOperation (v0.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOperation {
    pub sender: String,
    pub nonce: String,
    pub init_code: String,
    pub call_data: String,
    pub call_gas_limit: String,
    pub verification_gas_limit: String,
    pub pre_verification_gas: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
    pub paymaster_and_data: String,
    pub signature: String,
}

/// EntryPoint contract (v0.7).
pub const ENTRYPOINT_V07: &str = "0x0000000071727De22E5E9d8BAf0edAc6f37da032";
/// EntryPoint contract (v0.6).
pub const ENTRYPOINT_V06: &str = "0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789";

/// Known ERC-4337 bundler endpoints.
pub fn bundler_endpoints() -> Vec<BundlerEndpoint> {
    vec![
        BundlerEndpoint { name: "Pimlico".into(), url: "https://api.pimlico.io/v2".into(), supports_paymaster: true },
        BundlerEndpoint { name: "Stackup".into(), url: "https://api.stackup.sh/v1/node".into(), supports_paymaster: true },
        BundlerEndpoint { name: "Alchemy".into(), url: "https://eth-mainnet.g.alchemy.com/v2".into(), supports_paymaster: false },
        BundlerEndpoint { name: "Biconomy".into(), url: "https://bundler.biconomy.io/api/v2".into(), supports_paymaster: true },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerEndpoint {
    pub name: String,
    pub url: String,
    pub supports_paymaster: bool,
}

/// Encode `handleOps` calldata for EntryPoint.
///
/// Selector: 0x1fad948c
pub fn encode_handle_ops(ops: &[UserOperation], beneficiary: &[u8; 20]) -> Vec<u8> {
    let mut data = Vec::with_capacity(100);
    data.extend_from_slice(&[0x1f, 0xad, 0x94, 0x8c]);
    // Simplified — real encoding needs dynamic array ABI
    let mut slot = [0u8; 32];
    slot[12..].copy_from_slice(beneficiary);
    data.extend_from_slice(&slot);
    // ops count
    let mut count = [0u8; 32];
    count[28..32].copy_from_slice(&(ops.len() as u32).to_be_bytes());
    data.extend_from_slice(&count);
    data
}

/// Smart account factory addresses.
pub mod factories {
    pub const SAFE_4337_MODULE: &str = "0xa581c4A4DB7175302464fF3C06380BC3270b4037";
    pub const KERNEL_FACTORY: &str = "0x5de4839a76cf55d0c90e2061ef4386d962E15ae3";
    pub const SIMPLE_ACCOUNT_FACTORY: &str = "0x9406Cc6185a346906296840746125a0E44976454";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundler_endpoints_count() {
        assert!(bundler_endpoints().len() >= 4);
    }

    #[test]
    fn handle_ops_calldata() {
        let ops = vec![UserOperation {
            sender: "0x1".into(), nonce: "0x0".into(), init_code: "0x".into(),
            call_data: "0x".into(), call_gas_limit: "0x5000".into(),
            verification_gas_limit: "0x10000".into(), pre_verification_gas: "0x5000".into(),
            max_fee_per_gas: "0x1".into(), max_priority_fee_per_gas: "0x1".into(),
            paymaster_and_data: "0x".into(), signature: "0x".into(),
        }];
        let data = encode_handle_ops(&ops, &[0xBB; 20]);
        assert_eq!(&data[..4], &[0x1f, 0xad, 0x94, 0x8c]);
    }

    #[test]
    fn entrypoint_addresses() {
        assert!(ENTRYPOINT_V07.starts_with("0x"));
        assert!(ENTRYPOINT_V06.starts_with("0x"));
        assert_eq!(ENTRYPOINT_V07.len(), 42);
    }

    #[test]
    fn user_operation_serialization() {
        let op = UserOperation {
            sender: "0xabc".into(), nonce: "0x1".into(), init_code: "0x".into(),
            call_data: "0xdeadbeef".into(), call_gas_limit: "0x5000".into(),
            verification_gas_limit: "0x10000".into(), pre_verification_gas: "0x5000".into(),
            max_fee_per_gas: "0x3B9ACA00".into(), max_priority_fee_per_gas: "0x77359400".into(),
            paymaster_and_data: "0x".into(), signature: "0x".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("0xdeadbeef"));
    }
}
