//! EVM RLP encoding/decoding — transaction serialization, receipt parsing.

use serde::{Deserialize, Serialize};

/// RLP-encode a single byte.
pub fn rlp_encode_byte(b: u8) -> Vec<u8> {
    if b == 0 { vec![0x80] }
    else if b < 0x80 { vec![b] }
    else { vec![0x81, b] }
}

/// RLP-encode a byte slice.
pub fn rlp_encode_bytes(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![0x80];
    }
    if data.len() == 1 && data[0] < 0x80 {
        return vec![data[0]];
    }
    if data.len() <= 55 {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(0x80 + data.len() as u8);
        out.extend_from_slice(data);
        return out;
    }
    // Long string
    let len_bytes = encode_length(data.len());
    let mut out = Vec::with_capacity(1 + len_bytes.len() + data.len());
    out.push(0xb7 + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out.extend_from_slice(data);
    out
}

/// RLP-encode a list of already-encoded items.
pub fn rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let total_len: usize = items.iter().map(|i| i.len()).sum();

    if total_len <= 55 {
        let mut out = Vec::with_capacity(1 + total_len);
        out.push(0xc0 + total_len as u8);
        for item in items {
            out.extend_from_slice(item);
        }
        out
    } else {
        let len_bytes = encode_length(total_len);
        let mut out = Vec::with_capacity(1 + len_bytes.len() + total_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
        for item in items {
            out.extend_from_slice(item);
        }
        out
    }
}

/// RLP-encode a u64 value.
pub fn rlp_encode_u64(value: u64) -> Vec<u8> {
    if value == 0 { return vec![0x80]; }
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(7);
    rlp_encode_bytes(&bytes[start..])
}

/// RLP-encode a u128 value.
pub fn rlp_encode_u128(value: u128) -> Vec<u8> {
    if value == 0 { return vec![0x80]; }
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(15);
    rlp_encode_bytes(&bytes[start..])
}

fn encode_length(len: usize) -> Vec<u8> {
    if len < 256 { vec![len as u8] }
    else if len < 65536 { vec![(len >> 8) as u8, len as u8] }
    else { vec![(len >> 16) as u8, (len >> 8) as u8, len as u8] }
}

/// Decode RLP length prefix.
pub fn rlp_decode_length(data: &[u8]) -> Option<(RlpType, usize, usize)> {
    if data.is_empty() { return None; }
    let prefix = data[0];

    if prefix < 0x80 {
        Some((RlpType::String, 0, 1))
    } else if prefix <= 0xb7 {
        let len = (prefix - 0x80) as usize;
        Some((RlpType::String, 1, len))
    } else if prefix <= 0xbf {
        let len_len = (prefix - 0xb7) as usize;
        let mut len = 0usize;
        for i in 0..len_len {
            len = (len << 8) | data[1 + i] as usize;
        }
        Some((RlpType::String, 1 + len_len, len))
    } else if prefix <= 0xf7 {
        let len = (prefix - 0xc0) as usize;
        Some((RlpType::List, 1, len))
    } else {
        let len_len = (prefix - 0xf7) as usize;
        let mut len = 0usize;
        for i in 0..len_len {
            len = (len << 8) | data[1 + i] as usize;
        }
        Some((RlpType::List, 1 + len_len, len))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RlpType { String, List }

/// EVM transaction receipt fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxReceipt {
    pub tx_hash: String,
    pub block_number: u64,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub status: bool,
    pub logs_count: u32,
    pub contract_address: Option<String>,
}

/// Decode a transaction hash from RLP-encoded raw transaction.
pub fn tx_type_from_raw(raw_tx: &[u8]) -> TxType {
    if raw_tx.is_empty() { return TxType::Unknown; }
    match raw_tx[0] {
        0x01 => TxType::Eip2930,
        0x02 => TxType::Eip1559,
        0x03 => TxType::Eip4844,
        b if b >= 0xc0 => TxType::Legacy,
        _ => TxType::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxType { Legacy, Eip2930, Eip1559, Eip4844, Unknown }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rlp_encode_empty_bytes() {
        assert_eq!(rlp_encode_bytes(&[]), vec![0x80]);
    }

    #[test]
    fn rlp_encode_single_byte() {
        assert_eq!(rlp_encode_bytes(&[0x42]), vec![0x42]);
        assert_eq!(rlp_encode_bytes(&[0x80]), vec![0x81, 0x80]);
    }

    #[test]
    fn rlp_encode_short_string() {
        let data = b"dog";
        let encoded = rlp_encode_bytes(data);
        assert_eq!(encoded, vec![0x83, b'd', b'o', b'g']);
    }

    #[test]
    fn rlp_encode_empty_list() {
        assert_eq!(rlp_encode_list(&[]), vec![0xc0]);
    }

    #[test]
    fn rlp_encode_string_list() {
        let items = vec![
            rlp_encode_bytes(b"cat"),
            rlp_encode_bytes(b"dog"),
        ];
        let encoded = rlp_encode_list(&items);
        assert_eq!(encoded[0], 0xc8); // 0xc0 + 8 (3+1 + 3+1)
    }

    #[test]
    fn rlp_encode_u64_zero() {
        assert_eq!(rlp_encode_u64(0), vec![0x80]);
    }

    #[test]
    fn rlp_encode_u64_small() {
        assert_eq!(rlp_encode_u64(1), vec![0x01]);
        assert_eq!(rlp_encode_u64(127), vec![0x7f]);
    }

    #[test]
    fn rlp_encode_u64_large() {
        let encoded = rlp_encode_u64(1024);
        assert_eq!(encoded, vec![0x82, 0x04, 0x00]);
    }

    #[test]
    fn rlp_encode_u128_value() {
        let encoded = rlp_encode_u128(30_000_000_000); // 30 gwei
        assert!(encoded.len() <= 6);
    }

    #[test]
    fn rlp_decode_single_byte() {
        let (rtype, offset, len) = rlp_decode_length(&[0x42]).unwrap();
        assert_eq!(rtype, RlpType::String);
        assert_eq!(offset, 0);
        assert_eq!(len, 1);
    }

    #[test]
    fn rlp_decode_short_string() {
        let (rtype, offset, len) = rlp_decode_length(&[0x83, b'd', b'o', b'g']).unwrap();
        assert_eq!(rtype, RlpType::String);
        assert_eq!(offset, 1);
        assert_eq!(len, 3);
    }

    #[test]
    fn rlp_decode_list() {
        let (rtype, offset, len) = rlp_decode_length(&[0xc8]).unwrap();
        assert_eq!(rtype, RlpType::List);
        assert_eq!(offset, 1);
        assert_eq!(len, 8);
    }

    #[test]
    fn tx_type_detection() {
        assert_eq!(tx_type_from_raw(&[0x02, 0xf8]), TxType::Eip1559);
        assert_eq!(tx_type_from_raw(&[0x01, 0xf8]), TxType::Eip2930);
        assert_eq!(tx_type_from_raw(&[0x03, 0xf8]), TxType::Eip4844);
        assert_eq!(tx_type_from_raw(&[0xf8, 0x70]), TxType::Legacy);
    }

    #[test]
    fn tx_type_unknown() {
        assert_eq!(tx_type_from_raw(&[]), TxType::Unknown);
        assert_eq!(tx_type_from_raw(&[0x05]), TxType::Unknown);
    }

    #[test]
    fn tx_type_serialization() {
        assert_eq!(serde_json::to_string(&TxType::Eip1559).unwrap(), r#""eip1559""#);
        assert_eq!(serde_json::to_string(&TxType::Eip4844).unwrap(), r#""eip4844""#);
    }

    #[test]
    fn tx_receipt_structure() {
        let receipt = TxReceipt {
            tx_hash: "0xabc".into(), block_number: 19500000,
            gas_used: 21000, effective_gas_price: 30_000_000_000,
            status: true, logs_count: 0, contract_address: None,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(json.contains("19500000"));
    }

    #[test]
    fn rlp_encode_byte_values() {
        assert_eq!(rlp_encode_byte(0), vec![0x80]);
        assert_eq!(rlp_encode_byte(1), vec![0x01]);
        assert_eq!(rlp_encode_byte(0x80), vec![0x81, 0x80]);
    }

    #[test]
    fn rlp_roundtrip_concept() {
        // Encode and verify prefix matches expected type
        let encoded = rlp_encode_bytes(b"hello world");
        let (rtype, _, len) = rlp_decode_length(&encoded).unwrap();
        assert_eq!(rtype, RlpType::String);
        assert_eq!(len, 11);
    }
}
