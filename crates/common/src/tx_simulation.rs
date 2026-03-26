//! Transaction simulation — dry-run before broadcast to catch errors early.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use crate::Chain;

/// Result of simulating a transaction before broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub chain: Chain,
    pub success: bool,
    pub gas_used: Option<u64>,
    pub gas_limit: Option<u64>,
    pub error: Option<String>,
    pub revert_reason: Option<String>,
    pub state_changes: Vec<StateChange>,
    pub logs: Vec<SimulatedLog>,
    pub value_transferred: Decimal,
}

/// A state change detected during simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub address: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

/// A log emitted during simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
}

/// Build eth_call simulation request.
pub fn build_eth_call_request(
    from: &str, to: &str, data: &str, value: &str, block: &str,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "eth_call",
        "params": [{
            "from": from, "to": to,
            "data": data, "value": value,
        }, block]
    })
}

/// Build eth_estimateGas request.
pub fn build_estimate_gas_request(
    from: &str, to: &str, data: &str, value: &str,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "eth_estimateGas",
        "params": [{"from": from, "to": to, "data": data, "value": value}]
    })
}

/// Build Solana simulateTransaction request.
pub fn build_sol_simulate_request(signed_tx_base64: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "simulateTransaction",
        "params": [signed_tx_base64, {
            "encoding": "base64",
            "commitment": "confirmed",
            "replaceRecentBlockhash": true,
        }]
    })
}

/// Build Tron triggerConstantContract request (simulation).
pub fn build_tron_simulate_request(
    owner: &str, contract: &str, function_selector: &str, parameter: &str,
) -> serde_json::Value {
    serde_json::json!({
        "owner_address": owner,
        "contract_address": contract,
        "function_selector": function_selector,
        "parameter": parameter,
        "visible": true,
    })
}

/// Decode EVM revert reason from hex data.
pub fn decode_revert_reason(data: &str) -> Option<String> {
    let hex = data.trim_start_matches("0x");
    if hex.len() < 8 { return None; }

    // Error(string) selector = 0x08c379a0
    if &hex[..8] == "08c379a0" {
        // Skip selector(4) + offset(32) + length(32) = 68 bytes = 136 hex chars
        if hex.len() < 136 + 8 { return None; }
        let len_hex = &hex[72 + 8..72 + 8 + 64];
        let len = u64::from_str_radix(len_hex.trim_start_matches('0'), 16).ok()?;
        let msg_start = 136 + 8;
        let msg_end = msg_start + (len as usize * 2);
        if hex.len() < msg_end { return None; }
        let msg_bytes = hex::decode(&hex[msg_start..msg_end]).ok()?;
        return String::from_utf8(msg_bytes).ok();
    }

    // Panic(uint256) selector = 0x4e487b71
    if &hex[..8] == "4e487b71" {
        if hex.len() >= 72 {
            let code = u64::from_str_radix(hex[8..72].trim_start_matches('0'), 16).unwrap_or(0);
            let reason = match code {
                0x01 => "Assertion failed",
                0x11 => "Arithmetic overflow/underflow",
                0x12 => "Division by zero",
                0x21 => "Enum conversion out of range",
                0x31 => "Pop from empty array",
                0x32 => "Array index out of bounds",
                _ => "Unknown panic code",
            };
            return Some(format!("Panic({code:#x}): {reason}"));
        }
    }

    None
}

/// Pre-flight checks before broadcasting a transaction.
pub fn preflight_check(chain: Chain, signed_tx: &[u8]) -> Vec<PreflightWarning> {
    let mut warnings = Vec::new();

    if signed_tx.is_empty() {
        warnings.push(PreflightWarning::EmptyTransaction);
        return warnings;
    }

    match chain {
        Chain::Ethereum => {
            if signed_tx[0] != 0x02 && signed_tx[0] < 0xc0 {
                warnings.push(PreflightWarning::NotEip1559);
            }
        }
        Chain::Solana => {
            if signed_tx.len() > 1232 {
                warnings.push(PreflightWarning::TransactionTooLarge(signed_tx.len()));
            }
        }
        _ => {}
    }

    if signed_tx.len() > 100_000 {
        warnings.push(PreflightWarning::TransactionTooLarge(signed_tx.len()));
    }

    warnings
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightWarning {
    EmptyTransaction,
    NotEip1559,
    TransactionTooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eth_call_request() {
        let req = build_eth_call_request("0x1", "0x2", "0xdeadbeef", "0x0", "latest");
        assert_eq!(req["method"], "eth_call");
    }

    #[test]
    fn estimate_gas_request() {
        let req = build_estimate_gas_request("0x1", "0x2", "0x", "0x0");
        assert_eq!(req["method"], "eth_estimateGas");
    }

    #[test]
    fn sol_simulate_request() {
        let req = build_sol_simulate_request("base64data==");
        assert_eq!(req["method"], "simulateTransaction");
        assert!(req["params"][1]["replaceRecentBlockhash"].as_bool().unwrap());
    }

    #[test]
    fn tron_simulate_request() {
        let req = build_tron_simulate_request("T1", "T2", "transfer(address,uint256)", "");
        assert!(req["visible"].as_bool().unwrap());
    }

    #[test]
    fn decode_revert_panic() {
        // Test panic decoding works
        let mut hex = String::from("4e487b71");
        hex.push_str(&"0".repeat(62));
        hex.push_str("12"); // 0x12 = division by zero
        let result = decode_revert_reason(&hex).unwrap();
        assert!(result.contains("Division by zero"));
    }

    #[test]
    fn decode_panic_overflow() {
        let mut hex = String::from("4e487b71");
        hex.push_str(&"0".repeat(62));
        hex.push_str("11"); // 0x11 = overflow
        let result = decode_revert_reason(&hex).unwrap();
        assert!(result.contains("overflow"));
    }

    #[test]
    fn decode_unknown_revert() {
        assert_eq!(decode_revert_reason("0xdeadbeef"), None);
    }

    #[test]
    fn preflight_empty_tx() {
        let warnings = preflight_check(Chain::Ethereum, &[]);
        assert!(warnings.contains(&PreflightWarning::EmptyTransaction));
    }

    #[test]
    fn preflight_eip1559_ok() {
        let warnings = preflight_check(Chain::Ethereum, &[0x02, 0xf8, 0x70]);
        assert!(!warnings.contains(&PreflightWarning::NotEip1559));
    }

    #[test]
    fn preflight_solana_too_large() {
        let big_tx = vec![0u8; 2000];
        let warnings = preflight_check(Chain::Solana, &big_tx);
        assert!(warnings.iter().any(|w| matches!(w, PreflightWarning::TransactionTooLarge(_))));
    }

    #[test]
    fn simulation_result_structure() {
        let result = SimulationResult {
            chain: Chain::Ethereum, success: true,
            gas_used: Some(21000), gas_limit: Some(21000),
            error: None, revert_reason: None,
            state_changes: vec![StateChange {
                address: "0x1".into(), field: "balance".into(),
                old_value: "100".into(), new_value: "99".into(),
            }],
            logs: vec![],
            value_transferred: Decimal::ONE,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("state_changes"));
    }

    #[test]
    fn preflight_warning_serialization() {
        let json = serde_json::to_string(&PreflightWarning::NotEip1559).unwrap();
        assert!(json.contains("not_eip1559"));
    }
}
