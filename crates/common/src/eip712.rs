//! EIP-712 Typed Structured Data — domain separator, type hashing, encoding.

use serde::{Deserialize, Serialize};

/// EIP-712 domain separator parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip712Domain {
    pub name: String,
    pub version: String,
    pub chain_id: u64,
    pub verifying_contract: String,
    pub salt: Option<String>,
}

/// A typed data field definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip712Field {
    pub name: String,
    pub field_type: String,
}

/// Full EIP-712 typed data structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip712TypedData {
    pub types: std::collections::HashMap<String, Vec<Eip712Field>>,
    pub primary_type: String,
    pub domain: Eip712Domain,
    pub message: serde_json::Value,
}

/// Compute the type hash for a struct.
/// `typeHash = keccak256(encodeType(typeName))`
pub fn encode_type(type_name: &str, fields: &[Eip712Field]) -> String {
    let fields_str: Vec<String> = fields.iter()
        .map(|f| format!("{} {}", f.field_type, f.name))
        .collect();
    format!("{}({})", type_name, fields_str.join(","))
}

/// Compute keccak256 hash of a string.
pub fn keccak256_str(input: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute the domain separator hash.
pub fn domain_separator_hash(domain: &Eip712Domain) -> [u8; 32] {
    let type_hash = keccak256_str(
        "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
    );
    let name_hash = keccak256_str(&domain.name);
    let version_hash = keccak256_str(&domain.version);

    let mut encoded = Vec::with_capacity(160);
    encoded.extend_from_slice(&type_hash);
    encoded.extend_from_slice(&name_hash);
    encoded.extend_from_slice(&version_hash);
    // chainId as uint256
    let mut chain_id = [0u8; 32];
    chain_id[24..32].copy_from_slice(&domain.chain_id.to_be_bytes());
    encoded.extend_from_slice(&chain_id);
    // verifying contract as address (20 bytes in 32-byte slot)
    let addr_bytes = hex::decode(domain.verifying_contract.trim_start_matches("0x")).unwrap_or_default();
    let mut addr_slot = [0u8; 32];
    if addr_bytes.len() == 20 {
        addr_slot[12..].copy_from_slice(&addr_bytes);
    }
    encoded.extend_from_slice(&addr_slot);

    keccak256_bytes(&encoded)
}

fn keccak256_bytes(input: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let result = Keccak256::digest(input);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Common EIP-712 message types for custody operations.
pub mod custody_types {
    use super::*;

    pub fn permit_type() -> Vec<Eip712Field> {
        vec![
            Eip712Field { name: "owner".into(), field_type: "address".into() },
            Eip712Field { name: "spender".into(), field_type: "address".into() },
            Eip712Field { name: "value".into(), field_type: "uint256".into() },
            Eip712Field { name: "nonce".into(), field_type: "uint256".into() },
            Eip712Field { name: "deadline".into(), field_type: "uint256".into() },
        ]
    }

    pub fn withdrawal_approval_type() -> Vec<Eip712Field> {
        vec![
            Eip712Field { name: "withdrawalId".into(), field_type: "bytes32".into() },
            Eip712Field { name: "destination".into(), field_type: "address".into() },
            Eip712Field { name: "amount".into(), field_type: "uint256".into() },
            Eip712Field { name: "chain".into(), field_type: "uint256".into() },
            Eip712Field { name: "nonce".into(), field_type: "uint256".into() },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_type_permit() {
        let encoded = encode_type("Permit", &custody_types::permit_type());
        assert_eq!(encoded, "Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");
    }

    #[test]
    fn keccak256_empty() {
        let hash = keccak256_str("");
        assert_eq!(hex::encode(hash), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    }

    #[test]
    fn keccak256_eip712domain() {
        let hash = keccak256_str("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
        assert_eq!(hex::encode(hash), "8b73c3c69bb8fe3d512ecc4cf759cc79239f7b179b0ffacaa9a75d522b39400f");
    }

    #[test]
    fn domain_separator_deterministic() {
        let domain = Eip712Domain {
            name: "Test".into(), version: "1".into(),
            chain_id: 1, verifying_contract: "0x0000000000000000000000000000000000000001".into(),
            salt: None,
        };
        let h1 = domain_separator_hash(&domain);
        let h2 = domain_separator_hash(&domain);
        assert_eq!(h1, h2);
    }

    #[test]
    fn domain_separator_different_chains() {
        let d1 = Eip712Domain { name: "T".into(), version: "1".into(), chain_id: 1, verifying_contract: "0x0000000000000000000000000000000000000001".into(), salt: None };
        let d2 = Eip712Domain { name: "T".into(), version: "1".into(), chain_id: 42161, verifying_contract: "0x0000000000000000000000000000000000000001".into(), salt: None };
        assert_ne!(domain_separator_hash(&d1), domain_separator_hash(&d2));
    }

    #[test]
    fn withdrawal_approval_type_fields() {
        let fields = custody_types::withdrawal_approval_type();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0].name, "withdrawalId");
    }

    #[test]
    fn typed_data_serialization() {
        let mut types = std::collections::HashMap::new();
        types.insert("Permit".into(), custody_types::permit_type());
        let td = Eip712TypedData {
            types, primary_type: "Permit".into(),
            domain: Eip712Domain { name: "USDC".into(), version: "2".into(), chain_id: 1, verifying_contract: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into(), salt: None },
            message: serde_json::json!({"owner": "0x1", "spender": "0x2", "value": "1000000", "nonce": "0", "deadline": "999999999"}),
        };
        let json = serde_json::to_string(&td).unwrap();
        assert!(json.contains("Permit"));
        assert!(json.contains("USDC"));
    }
}
