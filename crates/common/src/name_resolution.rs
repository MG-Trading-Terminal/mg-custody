//! Name resolution — ENS, Unstoppable Domains, Solana Name Service.

use serde::{Deserialize, Serialize};
use crate::Chain;

/// A resolved name → address mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedName {
    pub name: String,
    pub address: String,
    pub chain: Chain,
    pub resolver: NameResolver,
    pub is_verified: bool,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameResolver {
    Ens,
    UnstoppableDomains,
    SolanaNameService,
    SpaceId,
}

/// ENS registry contract.
pub const ENS_REGISTRY: &str = "0x00000000000C2E074eC69A0dFb2997BA6C7d2e1e";
/// ENS public resolver.
pub const ENS_PUBLIC_RESOLVER: &str = "0x231b0Ee14048e9dCcD1d247744d114a4EB5E8E63";

/// Encode ENS `resolver(bytes32)` calldata.
/// Selector: 0x0178b8bf
pub fn encode_ens_resolver(namehash: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0x01, 0x78, 0xb8, 0xbf]);
    data.extend_from_slice(namehash);
    data
}

/// Encode ENS `addr(bytes32)` calldata on resolver.
/// Selector: 0x3b3b57de
pub fn encode_ens_addr(namehash: &[u8; 32]) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&[0x3b, 0x3b, 0x57, 0xde]);
    data.extend_from_slice(namehash);
    data
}

/// Compute ENS namehash for a domain.
/// namehash("") = 0x0
/// namehash("eth") = keccak256(namehash("") + keccak256("eth"))
pub fn ens_namehash(name: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut node = [0u8; 32];
    if name.is_empty() { return node; }
    for label in name.rsplit('.') {
        let label_hash = Keccak256::digest(label.as_bytes());
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&node);
        combined.extend_from_slice(&label_hash);
        let result = Keccak256::digest(&combined);
        node.copy_from_slice(&result);
    }
    node
}

/// Supported TLDs per resolver.
pub fn supported_tlds() -> Vec<(&'static str, NameResolver)> {
    vec![
        (".eth", NameResolver::Ens),
        (".xyz", NameResolver::Ens),
        (".crypto", NameResolver::UnstoppableDomains),
        (".nft", NameResolver::UnstoppableDomains),
        (".wallet", NameResolver::UnstoppableDomains),
        (".blockchain", NameResolver::UnstoppableDomains),
        (".sol", NameResolver::SolanaNameService),
        (".bnb", NameResolver::SpaceId),
    ]
}

/// Detect which resolver to use for a name.
pub fn detect_resolver(name: &str) -> Option<NameResolver> {
    supported_tlds().into_iter()
        .find(|(tld, _)| name.ends_with(tld))
        .map(|(_, resolver)| resolver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ens_namehash_empty() {
        assert_eq!(ens_namehash(""), [0u8; 32]);
    }

    #[test]
    fn ens_namehash_eth() {
        let hash = ens_namehash("eth");
        assert_ne!(hash, [0u8; 32]);
        assert_eq!(hex::encode(hash), "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae");
    }

    #[test]
    fn ens_namehash_vitalik() {
        let hash = ens_namehash("vitalik.eth");
        assert_ne!(hash, [0u8; 32]);
        // Known hash for vitalik.eth
        assert_eq!(hex::encode(hash), "ee6c4522aab0003e8d14cd40a6af439055fd2577951148c14b6cea9a53475835");
    }

    #[test]
    fn ens_resolver_calldata() {
        let data = encode_ens_resolver(&[0u8; 32]);
        assert_eq!(&data[..4], &[0x01, 0x78, 0xb8, 0xbf]);
        assert_eq!(data.len(), 36);
    }

    #[test]
    fn ens_addr_calldata() {
        let data = encode_ens_addr(&[0u8; 32]);
        assert_eq!(&data[..4], &[0x3b, 0x3b, 0x57, 0xde]);
    }

    #[test]
    fn detect_ens() {
        assert_eq!(detect_resolver("vitalik.eth"), Some(NameResolver::Ens));
    }

    #[test]
    fn detect_unstoppable() {
        assert_eq!(detect_resolver("user.crypto"), Some(NameResolver::UnstoppableDomains));
        assert_eq!(detect_resolver("user.wallet"), Some(NameResolver::UnstoppableDomains));
    }

    #[test]
    fn detect_sns() {
        assert_eq!(detect_resolver("user.sol"), Some(NameResolver::SolanaNameService));
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_resolver("user.com"), None);
    }

    #[test]
    fn resolved_name_structure() {
        let resolved = ResolvedName {
            name: "vitalik.eth".into(), address: "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045".into(),
            chain: Chain::Ethereum, resolver: NameResolver::Ens,
            is_verified: true, avatar_url: Some("https://metadata.ens.domains/mainnet/avatar/vitalik.eth".into()),
        };
        let json = serde_json::to_string(&resolved).unwrap();
        assert!(json.contains("vitalik.eth"));
    }
}
