//! ABI decoder — decode EVM calldata and event logs back to structured data.

use serde::{Deserialize, Serialize};

/// Known function selectors (first 4 bytes of keccak256(signature)).
pub fn known_selectors() -> Vec<(&'static str, &'static str)> {
    vec![
        ("0xa9059cbb", "transfer(address,uint256)"),
        ("0x23b872dd", "transferFrom(address,address,uint256)"),
        ("0x095ea7b3", "approve(address,uint256)"),
        ("0x70a08231", "balanceOf(address)"),
        ("0xdd62ed3e", "allowance(address,address)"),
        ("0x18160ddd", "totalSupply()"),
        ("0x313ce567", "decimals()"),
        ("0x06fdde03", "name()"),
        ("0x95d89b41", "symbol()"),
        ("0xd505accf", "permit(address,address,uint256,uint256,uint8,bytes32,bytes32)"),
        ("0x414bf389", "exactInputSingle(...)"),
        ("0x6e553f65", "deposit(uint256,address)"),
        ("0xb460af94", "withdraw(uint256,address,address)"),
        ("0x6a761202", "execTransaction(...)"),
        ("0x8d80ff0a", "multiSend(bytes)"),
        ("0x40c10f19", "mint(address,uint256)"),
        ("0x42966c68", "burn(uint256)"),
        ("0x8456cb59", "pause()"),
        ("0x3f4ba83a", "unpause()"),
        ("0xa1903eab", "submit(address)"),           // Lido
        ("0xd0e30db0", "deposit()"),                  // WETH/RocketPool
        ("0x82ad56cb", "aggregate3(Call3[])"),         // Multicall3
        ("0x1fad948c", "handleOps(UserOperation[],address)"), // ERC-4337
        ("0xf242432a", "safeTransferFrom(address,address,uint256,uint256,bytes)"), // ERC-1155
        ("0x3df02124", "exchange(int128,int128,uint256,uint256)"), // Curve
        ("0xab9c4b5d", "flashLoan(...)"),             // Aave
    ]
}

/// Decode a function selector to its name.
pub fn decode_selector(calldata: &[u8]) -> Option<&'static str> {
    if calldata.len() < 4 { return None; }
    let selector = format!("0x{}", hex::encode(&calldata[..4]));
    known_selectors().into_iter()
        .find(|(s, _)| *s == selector)
        .map(|(_, name)| name)
}

/// Decoded EVM event log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedEvent {
    pub name: String,
    pub signature: String,
    pub params: Vec<DecodedParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParam {
    pub name: String,
    pub param_type: String,
    pub value: String,
    pub indexed: bool,
}

/// Known event topic0 hashes.
pub fn known_event_topics() -> Vec<(&'static str, &'static str)> {
    vec![
        ("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef", "Transfer(address,address,uint256)"),
        ("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925", "Approval(address,address,uint256)"),
        ("0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822", "Swap(address,uint256,uint256,uint256,uint256,address)"),
        ("0x1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1", "Sync(uint112,uint112)"),
        ("0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67", "Swap(address,address,int256,int256,uint160,uint128,int24)"),
        ("0x7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65", "Withdrawal(address,uint256)"),
        ("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c", "Deposit(address,uint256)"),
    ]
}

/// Decode an event topic0 to its name.
pub fn decode_event_topic(topic0: &str) -> Option<&'static str> {
    known_event_topics().into_iter()
        .find(|(t, _)| *t == topic0)
        .map(|(_, name)| name)
}

/// Decode an address from a 32-byte ABI-encoded slot.
pub fn decode_address(data: &[u8]) -> Option<String> {
    if data.len() < 32 { return None; }
    // Address is in the last 20 bytes of a 32-byte slot
    Some(format!("0x{}", hex::encode(&data[12..32])))
}

/// Decode a uint256 from a 32-byte slot.
pub fn decode_uint256(data: &[u8]) -> Option<String> {
    if data.len() < 32 { return None; }
    // Strip leading zeros for readability
    let s = hex::encode(data);
    let trimmed = s.trim_start_matches('0');
    if trimmed.is_empty() { Some("0".into()) } else { Some(format!("0x{trimmed}")) }
}

/// Decode ERC-20 Transfer event from log data.
pub fn decode_transfer_event(topics: &[&str], data: &[u8]) -> Option<DecodedEvent> {
    if topics.len() < 3 { return None; }
    if topics[0] != "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef" {
        return None;
    }

    let from = format!("0x{}", &topics[1][26..]); // last 20 bytes
    let to = format!("0x{}", &topics[2][26..]);
    let value = decode_uint256(data).unwrap_or_default();

    Some(DecodedEvent {
        name: "Transfer".into(),
        signature: "Transfer(address,address,uint256)".into(),
        params: vec![
            DecodedParam { name: "from".into(), param_type: "address".into(), value: from, indexed: true },
            DecodedParam { name: "to".into(), param_type: "address".into(), value: to, indexed: true },
            DecodedParam { name: "value".into(), param_type: "uint256".into(), value, indexed: false },
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_known_selector_transfer() {
        let calldata = hex::decode("a9059cbb000000000000000000000000").unwrap();
        assert_eq!(decode_selector(&calldata), Some("transfer(address,uint256)"));
    }

    #[test]
    fn decode_known_selector_approve() {
        let calldata = hex::decode("095ea7b3").unwrap();
        assert_eq!(decode_selector(&calldata), Some("approve(address,uint256)"));
    }

    #[test]
    fn decode_unknown_selector() {
        let calldata = hex::decode("deadbeef").unwrap();
        assert_eq!(decode_selector(&calldata), None);
    }

    #[test]
    fn decode_too_short() {
        assert_eq!(decode_selector(&[0xAA, 0xBB]), None);
    }

    #[test]
    fn known_selectors_count() {
        assert!(known_selectors().len() >= 25);
    }

    #[test]
    fn decode_transfer_event_full() {
        let topics = &[
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
            "0x000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "0x000000000000000000000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ];
        let mut data = [0u8; 32];
        data[31] = 100; // value = 100

        let event = decode_transfer_event(topics, &data).unwrap();
        assert_eq!(event.name, "Transfer");
        assert_eq!(event.params.len(), 3);
        assert!(event.params[0].value.contains("aaaa"));
        assert!(event.params[1].value.contains("bbbb"));
    }

    #[test]
    fn decode_event_topic_transfer() {
        assert_eq!(
            decode_event_topic("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"),
            Some("Transfer(address,address,uint256)")
        );
    }

    #[test]
    fn decode_event_topic_approval() {
        assert_eq!(
            decode_event_topic("0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925"),
            Some("Approval(address,address,uint256)")
        );
    }

    #[test]
    fn decode_address_from_slot() {
        let mut slot = [0u8; 32];
        slot[12..].copy_from_slice(&[0xAA; 20]);
        let addr = decode_address(&slot).unwrap();
        assert!(addr.starts_with("0x"));
        assert!(addr.contains("aaaa"));
    }

    #[test]
    fn decode_uint256_zero() {
        let slot = [0u8; 32];
        assert_eq!(decode_uint256(&slot), Some("0".into()));
    }

    #[test]
    fn decode_uint256_nonzero() {
        let mut slot = [0u8; 32];
        slot[31] = 0xFF;
        let val = decode_uint256(&slot).unwrap();
        assert_eq!(val, "0xff");
    }

    #[test]
    fn event_topics_count() {
        assert!(known_event_topics().len() >= 7);
    }
}
