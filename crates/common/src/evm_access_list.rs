//! EVM Access Lists (EIP-2930) — pre-declare storage slots for gas savings.

use serde::{Deserialize, Serialize};

/// An access list entry: address + storage keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessListEntry {
    pub address: String,
    pub storage_keys: Vec<String>,
}

/// Build eth_createAccessList RPC request.
pub fn build_create_access_list_request(
    from: &str, to: &str, data: &str, value: &str,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "eth_createAccessList",
        "params": [{"from": from, "to": to, "data": data, "value": value}, "latest"]
    })
}

/// Estimate gas savings from using an access list.
/// Cold SLOAD without AL = 2600 gas, with AL = 2100 gas (saves 500 per slot).
/// Cold account access without AL = 2600 gas, with AL = 2400 gas (saves 200).
/// But AL itself costs: 2400 per address + 1900 per storage key.
pub fn estimate_access_list_savings(entries: &[AccessListEntry]) -> AccessListGas {
    let mut al_cost: u64 = 0;
    let mut savings: u64 = 0;

    for entry in entries {
        al_cost += 2400; // per address
        savings += 200;  // cold account access savings
        for _ in &entry.storage_keys {
            al_cost += 1900;  // per storage key in AL
            savings += 500;   // cold SLOAD savings
        }
    }

    let net = if savings > al_cost { savings - al_cost } else { 0 };

    AccessListGas {
        access_list_cost: al_cost,
        execution_savings: savings,
        net_savings: net,
        worth_it: net > 0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessListGas {
    pub access_list_cost: u64,
    pub execution_savings: u64,
    pub net_savings: u64,
    pub worth_it: bool,
}

/// Common storage slots for known contracts.
pub mod known_slots {
    /// ERC-20 balanceOf mapping slot (slot 0 in most implementations).
    pub fn erc20_balance_slot(holder: &[u8; 20]) -> String {
        // keccak256(abi.encode(holder, uint256(0)))
        use sha3::{Digest, Keccak256};
        let mut data = [0u8; 64];
        data[12..32].copy_from_slice(holder);
        // slot 0 for balances mapping
        let hash = Keccak256::digest(&data);
        format!("0x{}", hex::encode(hash))
    }

    /// ERC-20 allowance mapping slot (slot 1 in most implementations).
    pub fn erc20_allowance_slot(owner: &[u8; 20], spender: &[u8; 20]) -> String {
        use sha3::{Digest, Keccak256};
        // keccak256(abi.encode(spender, keccak256(abi.encode(owner, uint256(1)))))
        let mut inner = [0u8; 64];
        inner[12..32].copy_from_slice(owner);
        inner[63] = 1; // slot 1
        let inner_hash = Keccak256::digest(&inner);

        let mut outer = [0u8; 64];
        outer[12..32].copy_from_slice(spender);
        outer[32..64].copy_from_slice(&inner_hash);
        let hash = Keccak256::digest(&outer);
        format!("0x{}", hex::encode(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_access_list_request() {
        let req = build_create_access_list_request("0x1", "0x2", "0x", "0x0");
        assert_eq!(req["method"], "eth_createAccessList");
    }

    #[test]
    fn access_list_savings_single_address() {
        let entries = vec![AccessListEntry {
            address: "0x1".into(), storage_keys: vec![],
        }];
        let gas = estimate_access_list_savings(&entries);
        assert_eq!(gas.access_list_cost, 2400);
        assert_eq!(gas.execution_savings, 200);
        assert!(!gas.worth_it); // 200 < 2400
    }

    #[test]
    fn access_list_savings_many_slots() {
        let entries = vec![AccessListEntry {
            address: "0x1".into(),
            storage_keys: vec!["0xa".into(), "0xb".into(), "0xc".into(), "0xd".into(), "0xe".into(),
                              "0xf".into(), "0x10".into(), "0x11".into(), "0x12".into(), "0x13".into()],
        }];
        let gas = estimate_access_list_savings(&entries);
        // cost: 2400 + 10*1900 = 21400
        // savings: 200 + 10*500 = 5200
        assert!(!gas.worth_it); // still not worth it for most cases
    }

    #[test]
    fn balance_slot_deterministic() {
        let holder = [0xAA; 20];
        let s1 = known_slots::erc20_balance_slot(&holder);
        let s2 = known_slots::erc20_balance_slot(&holder);
        assert_eq!(s1, s2);
        assert!(s1.starts_with("0x"));
        assert_eq!(s1.len(), 66); // 0x + 64 hex chars
    }

    #[test]
    fn allowance_slot_different_from_balance() {
        let addr = [0xBB; 20];
        let bal = known_slots::erc20_balance_slot(&addr);
        let allow = known_slots::erc20_allowance_slot(&addr, &addr);
        assert_ne!(bal, allow);
    }

    #[test]
    fn gas_savings_serialization() {
        let gas = AccessListGas {
            access_list_cost: 2400, execution_savings: 5200,
            net_savings: 2800, worth_it: true,
        };
        let json = serde_json::to_string(&gas).unwrap();
        assert!(json.contains("worth_it"));
    }
}
