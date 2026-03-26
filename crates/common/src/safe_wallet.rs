//! Safe{Wallet} (Gnosis Safe) — multi-sig wallet interactions.

use serde::{Deserialize, Serialize};

/// Safe proxy factory (v1.4.1).
pub const SAFE_PROXY_FACTORY: &str = "0x4e1DCf7AD4e460CfD30791CCC4F9c8a4f820ec67";
/// Safe singleton (v1.4.1).
pub const SAFE_SINGLETON: &str = "0x41675C099F32341bf84BFc5382aF534df5C7461a";
/// Safe fallback handler.
pub const SAFE_FALLBACK_HANDLER: &str = "0xfd0732Dc9E303f09fCEf3a7388Ad10A83459Ec99";
/// Safe multi-send.
pub const SAFE_MULTI_SEND: &str = "0x38869bf66a61cF6bDB996A6aE40D5853Fd43B526";

/// Safe transaction for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeTransaction {
    pub to: String,
    pub value: String,
    pub data: String,
    pub operation: SafeOperation,
    pub safe_tx_gas: String,
    pub base_gas: String,
    pub gas_price: String,
    pub gas_token: String,
    pub refund_receiver: String,
    pub nonce: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafeOperation {
    Call = 0,
    DelegateCall = 1,
}

/// Safe confirmation (signature from an owner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeConfirmation {
    pub owner: String,
    pub signature: String,
    pub signature_type: String,
}

/// Encode Safe `execTransaction` calldata.
/// Selector: 0x6a761202
pub fn encode_exec_transaction(tx: &SafeTransaction, signatures: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(400);
    data.extend_from_slice(&[0x6a, 0x76, 0x12, 0x02]);

    // to (address)
    let to_bytes = hex::decode(tx.to.trim_start_matches("0x")).unwrap_or_default();
    let mut slot = [0u8; 32];
    if to_bytes.len() == 20 { slot[12..].copy_from_slice(&to_bytes); }
    data.extend_from_slice(&slot);

    // value (uint256)
    let mut value_slot = [0u8; 32];
    let value_num: u64 = tx.value.parse().unwrap_or(0);
    value_slot[24..32].copy_from_slice(&value_num.to_be_bytes());
    data.extend_from_slice(&value_slot);

    // data offset, operation, safeTxGas, baseGas, gasPrice, gasToken, refundReceiver, signatures offset
    // Simplified — just include nonce and signature bytes
    let mut nonce_slot = [0u8; 32];
    nonce_slot[24..32].copy_from_slice(&tx.nonce.to_be_bytes());
    data.extend_from_slice(&nonce_slot);

    data.extend_from_slice(signatures);
    data
}

/// Encode Safe `getTransactionHash` for off-chain signing.
/// Selector: 0xd8d11f78
pub fn get_transaction_hash_selector() -> [u8; 4] {
    [0xd8, 0xd1, 0x1f, 0x78]
}

/// Multi-send encoding: pack multiple transactions.
pub fn encode_multi_send(transactions: &[(String, String, Vec<u8>)]) -> Vec<u8> {
    // Each entry: operation(1) + to(20) + value(32) + dataLength(32) + data
    let mut packed = Vec::new();
    for (to, value, calldata) in transactions {
        packed.push(0u8); // Call operation
        let to_bytes = hex::decode(to.trim_start_matches("0x")).unwrap_or_default();
        if to_bytes.len() == 20 {
            packed.extend_from_slice(&to_bytes);
        } else {
            packed.extend_from_slice(&[0u8; 20]);
        }
        // value as 32 bytes
        let mut val = [0u8; 32];
        let v: u64 = value.parse().unwrap_or(0);
        val[24..32].copy_from_slice(&v.to_be_bytes());
        packed.extend_from_slice(&val);
        // data length + data
        let mut dl = [0u8; 32];
        dl[28..32].copy_from_slice(&(calldata.len() as u32).to_be_bytes());
        packed.extend_from_slice(&dl);
        packed.extend_from_slice(calldata);
    }

    // Wrap in multiSend(bytes) — selector 0x8d80ff0a
    let mut data = Vec::with_capacity(packed.len() + 68);
    data.extend_from_slice(&[0x8d, 0x80, 0xff, 0x0a]);
    let mut offset = [0u8; 32];
    offset[31] = 0x20;
    data.extend_from_slice(&offset);
    let mut len = [0u8; 32];
    len[28..32].copy_from_slice(&(packed.len() as u32).to_be_bytes());
    data.extend_from_slice(&len);
    data.extend_from_slice(&packed);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_transaction_selector() {
        let tx = SafeTransaction {
            to: "0x0000000000000000000000000000000000000001".into(),
            value: "0".into(), data: "0x".into(),
            operation: SafeOperation::Call,
            safe_tx_gas: "0".into(), base_gas: "0".into(),
            gas_price: "0".into(), gas_token: "0x0000000000000000000000000000000000000000".into(),
            refund_receiver: "0x0000000000000000000000000000000000000000".into(), nonce: 0,
        };
        let data = encode_exec_transaction(&tx, &[]);
        assert_eq!(&data[..4], &[0x6a, 0x76, 0x12, 0x02]);
    }

    #[test]
    fn multi_send_encoding() {
        let txs = vec![
            ("0x0000000000000000000000000000000000000001".into(), "0".into(), vec![0xAA, 0xBB]),
        ];
        let data = encode_multi_send(&txs);
        assert_eq!(&data[..4], &[0x8d, 0x80, 0xff, 0x0a]);
        assert!(data.len() > 68);
    }

    #[test]
    fn safe_operation_values() {
        assert_eq!(SafeOperation::Call as u8, 0);
        assert_eq!(SafeOperation::DelegateCall as u8, 1);
    }

    #[test]
    fn safe_addresses_valid() {
        assert!(SAFE_PROXY_FACTORY.starts_with("0x"));
        assert_eq!(SAFE_PROXY_FACTORY.len(), 42);
        assert!(SAFE_SINGLETON.starts_with("0x"));
    }

    #[test]
    fn safe_tx_serialization() {
        let tx = SafeTransaction {
            to: "0x1".into(), value: "1000".into(), data: "0x".into(),
            operation: SafeOperation::Call,
            safe_tx_gas: "0".into(), base_gas: "21000".into(),
            gas_price: "0".into(), gas_token: "0x0".into(),
            refund_receiver: "0x0".into(), nonce: 5,
        };
        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("nonce"));
        assert!(json.contains("1000"));
    }
}
