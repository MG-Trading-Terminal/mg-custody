//! Bitcoin Script — opcodes, script building, P2SH/P2WSH redeem scripts.

use serde::{Deserialize, Serialize};

/// Bitcoin Script opcodes (commonly used).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Op0 = 0x00,
    OpPushBytes20 = 0x14,
    OpPushBytes32 = 0x20,
    OpPushBytes33 = 0x21,
    OpDup = 0x76,
    OpEqual = 0x87,
    OpEqualVerify = 0x88,
    OpHash160 = 0xa9,
    OpCheckSig = 0xac,
    OpCheckMultiSig = 0xae,
    Op1 = 0x51,
    Op2 = 0x52,
    Op3 = 0x53,
    Op4 = 0x54,
    Op5 = 0x55,
    OpCheckSequenceVerify = 0xb2,
    OpDrop = 0x75,
    OpIf = 0x63,
    OpElse = 0x67,
    OpEndIf = 0x68,
    OpReturn = 0x6a,
}

/// Build P2PKH script: OP_DUP OP_HASH160 <pubKeyHash> OP_EQUALVERIFY OP_CHECKSIG
pub fn build_p2pkh_script(pubkey_hash: &[u8; 20]) -> Vec<u8> {
    let mut script = Vec::with_capacity(25);
    script.push(OpCode::OpDup as u8);
    script.push(OpCode::OpHash160 as u8);
    script.push(OpCode::OpPushBytes20 as u8);
    script.extend_from_slice(pubkey_hash);
    script.push(OpCode::OpEqualVerify as u8);
    script.push(OpCode::OpCheckSig as u8);
    script
}

/// Build P2WPKH witness script (scriptPubKey): OP_0 <20-byte-hash>
pub fn build_p2wpkh_script(pubkey_hash: &[u8; 20]) -> Vec<u8> {
    let mut script = Vec::with_capacity(22);
    script.push(OpCode::Op0 as u8);
    script.push(OpCode::OpPushBytes20 as u8);
    script.extend_from_slice(pubkey_hash);
    script
}

/// Build P2WSH script (scriptPubKey): OP_0 <32-byte-hash>
pub fn build_p2wsh_script(script_hash: &[u8; 32]) -> Vec<u8> {
    let mut script = Vec::with_capacity(34);
    script.push(OpCode::Op0 as u8);
    script.push(OpCode::OpPushBytes32 as u8);
    script.extend_from_slice(script_hash);
    script
}

/// Build M-of-N multisig redeem script.
pub fn build_multisig_redeem_script(m: u8, pubkeys: &[Vec<u8>]) -> Vec<u8> {
    let n = pubkeys.len() as u8;
    assert!(m >= 1 && m <= n && n <= 15);

    let mut script = Vec::with_capacity(1 + pubkeys.len() * 34 + 2);
    script.push(0x50 + m); // OP_M
    for pk in pubkeys {
        script.push(pk.len() as u8);
        script.extend_from_slice(pk);
    }
    script.push(0x50 + n); // OP_N
    script.push(OpCode::OpCheckMultiSig as u8);
    script
}

/// Build OP_RETURN data output script.
pub fn build_op_return_script(data: &[u8]) -> Vec<u8> {
    assert!(data.len() <= 80, "OP_RETURN data max 80 bytes");
    let mut script = Vec::with_capacity(2 + data.len());
    script.push(OpCode::OpReturn as u8);
    script.push(data.len() as u8);
    script.extend_from_slice(data);
    script
}

/// Build HTLC (Hash Time-Locked Contract) redeem script.
pub fn build_htlc_script(
    recipient_hash: &[u8; 20],
    refund_hash: &[u8; 20],
    secret_hash: &[u8; 32],
    locktime: u32,
) -> Vec<u8> {
    let mut script = Vec::with_capacity(100);
    script.push(OpCode::OpIf as u8);
    script.push(OpCode::OpHash160 as u8);
    script.push(0x14); // push 20 bytes
    script.extend_from_slice(&secret_hash[..20]); // simplified
    script.push(OpCode::OpEqualVerify as u8);
    script.push(OpCode::OpDup as u8);
    script.push(OpCode::OpHash160 as u8);
    script.push(0x14);
    script.extend_from_slice(recipient_hash);
    script.push(OpCode::OpElse as u8);
    // locktime as 4 bytes LE
    script.push(4);
    script.extend_from_slice(&locktime.to_le_bytes());
    script.push(OpCode::OpCheckSequenceVerify as u8);
    script.push(OpCode::OpDrop as u8);
    script.push(OpCode::OpDup as u8);
    script.push(OpCode::OpHash160 as u8);
    script.push(0x14);
    script.extend_from_slice(refund_hash);
    script.push(OpCode::OpEndIf as u8);
    script.push(OpCode::OpEqualVerify as u8);
    script.push(OpCode::OpCheckSig as u8);
    script
}

/// Parse script to identify type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    P2pkh,
    P2sh,
    P2wpkh,
    P2wsh,
    P2tr,
    OpReturn,
    Multisig,
    Unknown,
}

pub fn identify_script(script: &[u8]) -> ScriptType {
    if script.len() == 25 && script[0] == 0x76 && script[1] == 0xa9 { return ScriptType::P2pkh; }
    if script.len() == 23 && script[0] == 0xa9 { return ScriptType::P2sh; }
    if script.len() == 22 && script[0] == 0x00 && script[1] == 0x14 { return ScriptType::P2wpkh; }
    if script.len() == 34 && script[0] == 0x00 && script[1] == 0x20 { return ScriptType::P2wsh; }
    if script.len() == 34 && script[0] == 0x51 && script[1] == 0x20 { return ScriptType::P2tr; }
    if !script.is_empty() && script[0] == 0x6a { return ScriptType::OpReturn; }
    if !script.is_empty() && script.last() == Some(&0xae) { return ScriptType::Multisig; }
    ScriptType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p2pkh_script() {
        let script = build_p2pkh_script(&[0xAA; 20]);
        assert_eq!(script.len(), 25);
        assert_eq!(script[0], OpCode::OpDup as u8);
        assert_eq!(script[24], OpCode::OpCheckSig as u8);
        assert_eq!(identify_script(&script), ScriptType::P2pkh);
    }

    #[test]
    fn p2wpkh_script() {
        let script = build_p2wpkh_script(&[0xBB; 20]);
        assert_eq!(script.len(), 22);
        assert_eq!(script[0], 0x00);
        assert_eq!(script[1], 0x14);
        assert_eq!(identify_script(&script), ScriptType::P2wpkh);
    }

    #[test]
    fn p2wsh_script() {
        let script = build_p2wsh_script(&[0xCC; 32]);
        assert_eq!(script.len(), 34);
        assert_eq!(script[0], 0x00);
        assert_eq!(script[1], 0x20);
        assert_eq!(identify_script(&script), ScriptType::P2wsh);
    }

    #[test]
    fn multisig_2_of_3() {
        let pks = vec![vec![0xAA; 33], vec![0xBB; 33], vec![0xCC; 33]];
        let script = build_multisig_redeem_script(2, &pks);
        assert_eq!(script[0], 0x52); // OP_2
        assert_eq!(*script.last().unwrap(), OpCode::OpCheckMultiSig as u8);
        assert_eq!(identify_script(&script), ScriptType::Multisig);
    }

    #[test]
    fn op_return_script() {
        let data = b"Hello Bitcoin";
        let script = build_op_return_script(data);
        assert_eq!(script[0], OpCode::OpReturn as u8);
        assert_eq!(script[1], data.len() as u8);
        assert_eq!(identify_script(&script), ScriptType::OpReturn);
    }

    #[test]
    fn htlc_script() {
        let script = build_htlc_script(&[0xAA; 20], &[0xBB; 20], &[0xCC; 32], 144);
        assert!(script.len() > 50);
        assert_eq!(script[0], OpCode::OpIf as u8);
    }

    #[test]
    fn identify_p2tr() {
        let mut script = vec![0x51, 0x20]; // OP_1, PUSH32
        script.extend_from_slice(&[0xDD; 32]);
        assert_eq!(identify_script(&script), ScriptType::P2tr);
    }

    #[test]
    fn identify_unknown() {
        assert_eq!(identify_script(&[0xFF, 0xFF]), ScriptType::Unknown);
    }

    #[test]
    fn script_type_serialization() {
        assert_eq!(serde_json::to_string(&ScriptType::P2wpkh).unwrap(), r#""p2wpkh""#);
        assert_eq!(serde_json::to_string(&ScriptType::P2tr).unwrap(), r#""p2tr""#);
    }

    #[test]
    fn opcode_values() {
        assert_eq!(OpCode::OpDup as u8, 0x76);
        assert_eq!(OpCode::OpCheckSig as u8, 0xac);
        assert_eq!(OpCode::OpCheckMultiSig as u8, 0xae);
        assert_eq!(OpCode::OpReturn as u8, 0x6a);
    }

    #[test]
    fn op_return_max_80() {
        let data = vec![0u8; 80];
        let script = build_op_return_script(&data);
        assert_eq!(script.len(), 82); // 1 + 1 + 80
    }

    #[test]
    #[should_panic]
    fn op_return_too_large() {
        build_op_return_script(&[0u8; 81]);
    }

    #[test]
    fn multisig_1_of_1() {
        let pks = vec![vec![0xAA; 33]];
        let script = build_multisig_redeem_script(1, &pks);
        assert_eq!(script[0], 0x51); // OP_1
    }
}
