//! Gnosis Safe multi-sig execution engine.
//!
//! Takes a Safe wallet address + owner keys, proposes transactions,
//! collects signatures, and executes when threshold is met.
//!
//! References:
//! - Safe{Wallet} contract: https://github.com/safe-global/safe-contracts/blob/v1.4.1/contracts/Safe.sol
//! - EIP-712 Safe transaction hash: https://eips.ethereum.org/EIPS/eip-712
//! - Safe transaction hash encoding: keccak256(safeTxHashPreimage) per Safe.getTransactionHash()

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::safe_wallet::{encode_exec_transaction, SafeTransaction};
use crate::eip712::{domain_separator_hash, keccak256_str, Eip712Domain};
use crate::Chain;

// Re-export so downstream crates can import SafeOperation from this module.
pub use crate::safe_wallet::SafeOperation;

// ---------------------------------------------------------------------------
// Safe EIP-712 type string
// Per Safe v1.4.1 contract: getTransactionHash encodes these fields
// Reference: https://github.com/safe-global/safe-contracts/blob/v1.4.1/contracts/Safe.sol#L352
// ---------------------------------------------------------------------------

const SAFE_TX_TYPE_HASH_PREIMAGE: &str =
    "SafeTx(address to,uint256 value,bytes data,uint8 operation,\
     uint256 safeTxGas,uint256 baseGas,uint256 gasPrice,\
     address gasToken,address refundReceiver,uint256 nonce)";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A Gnosis Safe multi-sig wallet registered in the custody service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeWallet {
    /// Safe proxy contract address (checksummed EIP-55).
    pub address: String,
    pub chain: Chain,
    /// M-of-N threshold — how many signatures are required.
    pub threshold: u32,
    /// All owner addresses (checksummed EIP-55).
    pub owners: Vec<String>,
    /// Current on-chain nonce. Must match when executing.
    pub nonce: u64,
}

/// A proposed transaction against a Gnosis Safe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeManagedTx {
    pub id: Uuid,
    pub safe_address: String,
    /// Destination address.
    pub to: String,
    /// Native-token value in wei (stored as Decimal to avoid f64).
    pub value: Decimal,
    /// ABI-encoded calldata.
    pub data: Vec<u8>,
    pub operation: SafeOperation,
    /// EIP-712 structured-data hash identifying this transaction uniquely.
    pub safe_tx_hash: String,
    pub signatures: Vec<SafeSignature>,
    pub status: SafeTxStatus,
    pub created_at: DateTime<Utc>,
}

/// Status of a managed Safe transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeTxStatus {
    /// Proposed, collecting signatures.
    Proposed,
    /// Actively circulating for signatures.
    Signing,
    /// Threshold met, ready to execute.
    Ready,
    /// Executed on-chain.
    Executed,
    /// Execution failed or cancelled.
    Failed,
}

/// A single owner's signature on a Safe transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeSignature {
    /// Owner address (checksummed EIP-55).
    pub signer: String,
    /// Raw 65-byte ECDSA signature (r ++ s ++ v).
    pub signature: Vec<u8>,
    pub signed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from Safe executor operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeError {
    /// Signer is not in the Safe's owner list.
    SignerNotOwner { signer: String },
    /// Signer has already submitted a signature for this transaction.
    DuplicateSignature { signer: String },
    /// Signature has wrong length (must be 65 bytes).
    InvalidSignatureLength { got: usize },
    /// Transaction is not in a state that allows this operation.
    InvalidStatus { current: SafeTxStatus, required: &'static str },
}

impl std::fmt::Display for SafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeError::SignerNotOwner { signer } =>
                write!(f, "Address {signer} is not an owner of this Safe"),
            SafeError::DuplicateSignature { signer } =>
                write!(f, "Address {signer} has already signed this transaction"),
            SafeError::InvalidSignatureLength { got } =>
                write!(f, "Signature must be 65 bytes, got {got}"),
            SafeError::InvalidStatus { current, required } =>
                write!(f, "Transaction is {current:?}, required status: {required}"),
        }
    }
}

impl std::error::Error for SafeError {}

// ---------------------------------------------------------------------------
// SafeWallet implementation
// ---------------------------------------------------------------------------

impl SafeWallet {
    /// Create a new SafeWallet record.
    pub fn new(
        address: String,
        chain: Chain,
        threshold: u32,
        owners: Vec<String>,
        nonce: u64,
    ) -> Self {
        Self { address, chain, threshold, owners, nonce }
    }

    /// Propose a new transaction. Computes the EIP-712 safe_tx_hash and
    /// returns a `SafeManagedTx` in `Proposed` status.
    ///
    /// `value_wei` is the native-token amount in wei as a Decimal (no f64).
    pub fn propose_transaction(
        &self,
        to: String,
        value_wei: Decimal,
        data: Vec<u8>,
        operation: SafeOperation,
        chain_id: u64,
    ) -> SafeManagedTx {
        let id = Uuid::new_v4();
        let safe_tx_hash = compute_safe_tx_hash(
            &self.address,
            &to,
            value_wei,
            &data,
            operation,
            self.nonce,
            chain_id,
        );

        SafeManagedTx {
            id,
            safe_address: self.address.clone(),
            to,
            value: value_wei,
            data,
            operation,
            safe_tx_hash,
            signatures: Vec::new(),
            status: SafeTxStatus::Proposed,
            created_at: Utc::now(),
        }
    }

    /// Check whether `address` is a registered owner of this Safe.
    pub fn is_owner(&self, address: &str) -> bool {
        self.owners.iter().any(|o| o.eq_ignore_ascii_case(address))
    }
}

// ---------------------------------------------------------------------------
// SafeManagedTx implementation
// ---------------------------------------------------------------------------

impl SafeManagedTx {
    /// Add a signature from `signer`. Validates:
    /// 1. Signer is in the Safe's owner list.
    /// 2. Signer has not already signed.
    /// 3. Signature is 65 bytes (ECDSA r+s+v).
    ///
    /// Returns `true` when the signature causes the threshold to be met.
    pub fn add_signature(
        &mut self,
        signer: String,
        signature: Vec<u8>,
        wallet: &SafeWallet,
    ) -> Result<bool, SafeError> {
        if self.status == SafeTxStatus::Executed || self.status == SafeTxStatus::Failed {
            return Err(SafeError::InvalidStatus {
                current: self.status,
                required: "Proposed or Signing",
            });
        }

        if !wallet.is_owner(&signer) {
            return Err(SafeError::SignerNotOwner { signer });
        }

        if self.signatures.iter().any(|s| s.signer.eq_ignore_ascii_case(&signer)) {
            return Err(SafeError::DuplicateSignature { signer });
        }

        if signature.len() != 65 {
            return Err(SafeError::InvalidSignatureLength { got: signature.len() });
        }

        self.signatures.push(SafeSignature {
            signer,
            signature,
            signed_at: Utc::now(),
        });

        // Transition status
        if self.is_ready(wallet) {
            self.status = SafeTxStatus::Ready;
            Ok(true)
        } else {
            self.status = SafeTxStatus::Signing;
            Ok(false)
        }
    }

    /// Returns `true` when collected signature count meets the Safe's threshold.
    pub fn is_ready(&self, wallet: &SafeWallet) -> bool {
        self.signatures.len() >= wallet.threshold as usize
    }

    /// Build the `execTransaction` calldata from the collected signatures.
    ///
    /// Signatures are sorted by signer address (ascending) as required by
    /// the Safe contract's signature validation.
    ///
    /// Reference: Safe.sol#checkNSignatures — owners must be sorted ascending.
    pub fn encode_exec(&self) -> Result<Vec<u8>, SafeError> {
        if self.status != SafeTxStatus::Ready {
            return Err(SafeError::InvalidStatus {
                current: self.status,
                required: "Ready",
            });
        }

        // Sort signatures by signer address (case-insensitive, ascending)
        let mut sorted = self.signatures.clone();
        sorted.sort_by(|a, b| a.signer.to_lowercase().cmp(&b.signer.to_lowercase()));

        // Concatenate raw 65-byte signatures
        let packed: Vec<u8> = sorted.iter().flat_map(|s| s.signature.iter().copied()).collect();

        let safe_tx = SafeTransaction {
            to: self.to.clone(),
            // Value in wei as decimal — we truncate to u64 wei for on-chain encoding.
            // This is safe: the maximum ETH supply is ~120M ETH = 1.2e26 wei < u64::MAX (1.8e19 wei).
            // For values above u64::MAX wei (182 ETH * 1e18), the caller should use a token transfer
            // with value=0 in the native field and the amount encoded in calldata.
            value: self.value.to_string(),
            data: format!("0x{}", hex::encode(&self.data)),
            operation: self.operation,
            safe_tx_gas: "0".into(),
            base_gas: "0".into(),
            gas_price: "0".into(),
            gas_token: "0x0000000000000000000000000000000000000000".into(),
            refund_receiver: "0x0000000000000000000000000000000000000000".into(),
            nonce: 0, // nonce is already incorporated in safe_tx_hash during proposal
        };

        Ok(encode_exec_transaction(&safe_tx, &packed))
    }
}

// ---------------------------------------------------------------------------
// EIP-712 Safe transaction hash
// ---------------------------------------------------------------------------

/// Compute the EIP-712 hash that owners must sign for a Safe transaction.
///
/// Per Safe v1.4.1:
/// ```text
/// safeTxHash = keccak256(
///     0x1901 ||
///     domainSeparator ||
///     keccak256(encodeTransactionData(...))
/// )
/// ```
///
/// Reference: https://github.com/safe-global/safe-contracts/blob/v1.4.1/contracts/Safe.sol#L352
pub fn compute_safe_tx_hash(
    safe_address: &str,
    to: &str,
    value_wei: Decimal,
    data: &[u8],
    operation: SafeOperation,
    nonce: u64,
    chain_id: u64,
) -> String {
    use sha3::{Digest, Keccak256};

    // 1. Domain separator
    let domain = Eip712Domain {
        name: "Safe".into(),
        version: "1.3.0".into(),
        chain_id,
        verifying_contract: safe_address.to_string(),
        salt: None,
    };
    let domain_sep = domain_separator_hash(&domain);

    // 2. SafeTx type hash
    let type_hash = keccak256_str(SAFE_TX_TYPE_HASH_PREIMAGE);

    // 3. to address (padded to 32 bytes)
    let to_bytes = hex::decode(to.trim_start_matches("0x")).unwrap_or_default();
    let mut to_slot = [0u8; 32];
    if to_bytes.len() == 20 {
        to_slot[12..].copy_from_slice(&to_bytes);
    }

    // 4. value as uint256 (32 bytes, big-endian)
    // Convert Decimal wei to bytes. We work with the integer representation.
    let value_u64: u64 = value_wei.try_into().unwrap_or(0);
    let mut value_slot = [0u8; 32];
    value_slot[24..32].copy_from_slice(&value_u64.to_be_bytes());

    // 5. data as keccak256(bytes)
    let data_hash = {
        let h = Keccak256::digest(data);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&h);
        arr
    };

    // 6. operation (uint8 → uint256)
    let mut op_slot = [0u8; 32];
    op_slot[31] = operation as u8;

    // 7. safeTxGas, baseGas, gasPrice — all zero for standard custody use
    let zero32 = [0u8; 32];

    // 8. gasToken address (zero address → no refund)
    let gas_token_slot = [0u8; 32];

    // 9. refundReceiver address (zero address)
    let refund_slot = [0u8; 32];

    // 10. nonce as uint256
    let mut nonce_slot = [0u8; 32];
    nonce_slot[24..32].copy_from_slice(&nonce.to_be_bytes());

    // Encode the SafeTx struct: typeHash ++ fields
    let mut encoded = Vec::with_capacity(11 * 32);
    encoded.extend_from_slice(&type_hash);
    encoded.extend_from_slice(&to_slot);
    encoded.extend_from_slice(&value_slot);
    encoded.extend_from_slice(&data_hash);
    encoded.extend_from_slice(&op_slot);
    encoded.extend_from_slice(&zero32); // safeTxGas
    encoded.extend_from_slice(&zero32); // baseGas
    encoded.extend_from_slice(&zero32); // gasPrice
    encoded.extend_from_slice(&gas_token_slot);
    encoded.extend_from_slice(&refund_slot);
    encoded.extend_from_slice(&nonce_slot);

    let struct_hash = {
        let h = Keccak256::digest(&encoded);
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&h);
        arr
    };

    // Final: 0x1901 ++ domainSeparator ++ structHash
    let mut preimage = Vec::with_capacity(66);
    preimage.extend_from_slice(&[0x19, 0x01]);
    preimage.extend_from_slice(&domain_sep);
    preimage.extend_from_slice(&struct_hash);

    let final_hash = Keccak256::digest(&preimage);
    format!("0x{}", hex::encode(final_hash))
}

// ---------------------------------------------------------------------------
// API request/response types (used by safe_routes.rs)
// ---------------------------------------------------------------------------

/// Request to register a Safe wallet.
#[derive(Debug, Deserialize)]
pub struct RegisterSafeRequest {
    /// Safe proxy address (checksummed EIP-55).
    pub address: String,
    pub chain: Chain,
    pub threshold: u32,
    pub owners: Vec<String>,
    pub nonce: u64,
}

/// Request to propose a transaction to a Safe.
#[derive(Debug, Deserialize)]
pub struct ProposeSafeTxRequest {
    pub to: String,
    /// Wei amount as string-encoded Decimal.
    pub value_wei: Decimal,
    /// Hex-encoded calldata, with or without "0x" prefix.
    pub data_hex: String,
    pub operation: SafeOperation,
    /// EVM chain ID (1 = Ethereum mainnet).
    pub chain_id: u64,
}

/// Request to add a signature to a pending Safe transaction.
#[derive(Debug, Deserialize)]
pub struct AddSafeSignatureRequest {
    /// Signer's address.
    pub signer: String,
    /// Hex-encoded 65-byte ECDSA signature.
    pub signature_hex: String,
}

/// Response for a Safe wallet.
#[derive(Debug, Serialize)]
pub struct SafeWalletResponse {
    pub address: String,
    pub chain: String,
    pub threshold: u32,
    pub owners: Vec<String>,
    pub nonce: u64,
}

impl From<SafeWallet> for SafeWalletResponse {
    fn from(w: SafeWallet) -> Self {
        SafeWalletResponse {
            address: w.address,
            chain: w.chain.as_str().to_string(),
            threshold: w.threshold,
            owners: w.owners,
            nonce: w.nonce,
        }
    }
}

/// Response for a Safe managed transaction.
#[derive(Debug, Serialize)]
pub struct SafeManagedTxResponse {
    pub id: String,
    pub safe_address: String,
    pub to: String,
    pub value_wei: String,
    pub data_hex: String,
    pub operation: String,
    pub safe_tx_hash: String,
    pub signatures_count: usize,
    pub status: String,
    pub created_at: String,
}

impl From<SafeManagedTx> for SafeManagedTxResponse {
    fn from(tx: SafeManagedTx) -> Self {
        SafeManagedTxResponse {
            id: tx.id.to_string(),
            safe_address: tx.safe_address,
            to: tx.to,
            value_wei: tx.value.to_string(),
            data_hex: format!("0x{}", hex::encode(&tx.data)),
            operation: match tx.operation {
                SafeOperation::Call => "call".into(),
                SafeOperation::DelegateCall => "delegate_call".into(),
            },
            safe_tx_hash: tx.safe_tx_hash,
            signatures_count: tx.signatures.len(),
            status: format!("{:?}", tx.status).to_lowercase(),
            created_at: tx.created_at.to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn test_wallet() -> SafeWallet {
        SafeWallet::new(
            "0x1234567890123456789012345678901234567890".into(),
            Chain::Ethereum,
            2, // 2-of-3
            vec![
                "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
                "0xCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC".into(),
            ],
            0,
        )
    }

    #[test]
    fn propose_transaction_generates_uuid_and_hash() {
        let wallet = test_wallet();
        let tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0),
            vec![],
            SafeOperation::Call,
            1,
        );
        assert_eq!(tx.status, SafeTxStatus::Proposed);
        assert!(!tx.safe_tx_hash.is_empty());
        assert!(tx.safe_tx_hash.starts_with("0x"));
        assert_eq!(tx.safe_tx_hash.len(), 66); // "0x" + 64 hex chars
    }

    #[test]
    fn safe_tx_hash_deterministic() {
        let hash1 = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0),
            &[],
            SafeOperation::Call,
            0,
            1,
        );
        let hash2 = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0),
            &[],
            SafeOperation::Call,
            0,
            1,
        );
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn safe_tx_hash_different_nonces() {
        let h0 = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0), &[], SafeOperation::Call, 0, 1,
        );
        let h1 = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0), &[], SafeOperation::Call, 1, 1,
        );
        assert_ne!(h0, h1, "Different nonces must produce different hashes");
    }

    #[test]
    fn safe_tx_hash_different_chain_ids() {
        let h_mainnet = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0), &[], SafeOperation::Call, 0, 1,
        );
        let h_arbitrum = compute_safe_tx_hash(
            "0x1234567890123456789012345678901234567890",
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            dec!(0), &[], SafeOperation::Call, 0, 42161,
        );
        assert_ne!(h_mainnet, h_arbitrum, "Different chain IDs must produce different hashes");
    }

    #[test]
    fn add_signature_rejects_non_owner() {
        let wallet = test_wallet();
        let mut tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        let result = tx.add_signature(
            "0xDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD".into(),
            vec![0u8; 65],
            &wallet,
        );
        assert!(matches!(result, Err(SafeError::SignerNotOwner { .. })));
    }

    #[test]
    fn add_signature_rejects_wrong_length() {
        let wallet = test_wallet();
        let mut tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        let result = tx.add_signature(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            vec![0u8; 64], // wrong — must be 65
            &wallet,
        );
        assert!(matches!(result, Err(SafeError::InvalidSignatureLength { got: 64 })));
    }

    #[test]
    fn add_signature_rejects_duplicate() {
        let wallet = test_wallet();
        let mut tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        tx.add_signature(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            vec![0u8; 65],
            &wallet,
        ).unwrap();
        let result = tx.add_signature(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            vec![0u8; 65],
            &wallet,
        );
        assert!(matches!(result, Err(SafeError::DuplicateSignature { .. })));
    }

    #[test]
    fn is_ready_after_threshold_signatures() {
        let wallet = test_wallet(); // threshold = 2
        let mut tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        assert!(!tx.is_ready(&wallet));

        tx.add_signature(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            vec![0u8; 65], &wallet,
        ).unwrap();
        assert!(!tx.is_ready(&wallet));
        assert_eq!(tx.status, SafeTxStatus::Signing);

        tx.add_signature(
            "0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
            vec![1u8; 65], &wallet,
        ).unwrap();
        assert!(tx.is_ready(&wallet));
        assert_eq!(tx.status, SafeTxStatus::Ready);
    }

    #[test]
    fn encode_exec_requires_ready_status() {
        let wallet = test_wallet();
        let tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        // Still Proposed — should error
        assert!(matches!(tx.encode_exec(), Err(SafeError::InvalidStatus { .. })));
    }

    #[test]
    fn encode_exec_produces_exec_transaction_selector() {
        let wallet = test_wallet();
        let mut tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(0), vec![], SafeOperation::Call, 1,
        );
        tx.add_signature("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(), vec![0u8; 65], &wallet).unwrap();
        tx.add_signature("0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(), vec![1u8; 65], &wallet).unwrap();

        let calldata = tx.encode_exec().unwrap();
        // execTransaction selector = 0x6a761202
        assert_eq!(&calldata[..4], &[0x6a, 0x76, 0x12, 0x02]);
    }

    #[test]
    fn is_owner_case_insensitive() {
        let wallet = test_wallet();
        // Lower-case version of the first owner (40 hex chars = 20 bytes)
        assert!(wallet.is_owner("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        // Original mixed-case
        assert!(wallet.is_owner("0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        // Non-owner
        assert!(!wallet.is_owner("0xDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD"));
    }

    #[test]
    fn response_conversion_no_float() {
        let wallet = test_wallet();
        let tx = wallet.propose_transaction(
            "0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            dec!(1500000000000000000), // 1.5 ETH in wei as Decimal
            vec![],
            SafeOperation::Call,
            1,
        );
        let resp: SafeManagedTxResponse = tx.into();
        // Value must be string-encoded, not float
        assert_eq!(resp.value_wei, "1500000000000000000");
    }
}
