//! EVM-compatible chain registry.
//!
//! EVM L2 chains share Ethereum's address derivation, signing, and transaction format.
//! They differ only in: chain_id, RPC URL, block time, and confirmation requirements.
//!
//! This registry enables adding new EVM chains via configuration without code changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An EVM-compatible chain definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmChainDef {
    /// Unique chain identifier (e.g., "arbitrum", "optimism", "base").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// EIP-155 chain ID.
    pub chain_id: u64,
    /// Primary RPC endpoint.
    pub rpc_url: String,
    /// Fallback RPC endpoints.
    pub fallback_rpc_urls: Vec<String>,
    /// Block explorer URL (for tx links).
    pub explorer_url: Option<String>,
    /// Required confirmations for deposits.
    pub required_confirmations: u64,
    /// Average block time in seconds.
    pub block_time_secs: u64,
    /// Native token symbol (e.g., "ETH", "MATIC").
    pub native_token: String,
    /// Native token decimals (18 for most EVM chains).
    pub native_decimals: u8,
    /// ERC-20 tokens on this chain.
    pub tokens: HashMap<String, EvmTokenDef>,
    /// Whether this chain is enabled.
    pub enabled: bool,
    /// Whether this is a testnet.
    pub testnet: bool,
}

/// An ERC-20 token definition on an EVM chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmTokenDef {
    /// Token symbol (e.g., "USDT", "USDC").
    pub symbol: String,
    /// Contract address (checksummed).
    pub contract_address: String,
    /// Token decimals.
    pub decimals: u8,
}

/// Registry of all known EVM chains.
#[derive(Debug, Clone)]
pub struct EvmChainRegistry {
    chains: HashMap<String, EvmChainDef>,
}

impl EvmChainRegistry {
    pub fn new() -> Self {
        Self {
            chains: HashMap::new(),
        }
    }

    /// Create a registry with all standard EVM chains pre-configured.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        for chain in default_evm_chains() {
            registry.register(chain);
        }
        registry
    }

    /// Register a new chain.
    pub fn register(&mut self, chain: EvmChainDef) {
        self.chains.insert(chain.id.clone(), chain);
    }

    /// Get a chain by ID.
    pub fn get(&self, id: &str) -> Option<&EvmChainDef> {
        self.chains.get(id)
    }

    /// Get a chain by EIP-155 chain ID.
    pub fn get_by_chain_id(&self, chain_id: u64) -> Option<&EvmChainDef> {
        self.chains.values().find(|c| c.chain_id == chain_id)
    }

    /// List all enabled chains.
    pub fn enabled_chains(&self) -> Vec<&EvmChainDef> {
        self.chains.values().filter(|c| c.enabled).collect()
    }

    /// List all enabled mainnet chains.
    pub fn mainnet_chains(&self) -> Vec<&EvmChainDef> {
        self.chains
            .values()
            .filter(|c| c.enabled && !c.testnet)
            .collect()
    }

    /// Total number of registered chains.
    pub fn len(&self) -> usize {
        self.chains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }
}

impl Default for EvmChainRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Default EVM chain configurations.
pub fn default_evm_chains() -> Vec<EvmChainDef> {
    vec![
        EvmChainDef {
            id: "ethereum".into(),
            name: "Ethereum Mainnet".into(),
            chain_id: 1,
            rpc_url: "http://localhost:8545".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://etherscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 12,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xdAC17F958D2ee523a2206206994597C13D831ec7".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".into(), decimals: 6,
                }),
                ("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "arbitrum".into(),
            name: "Arbitrum One".into(),
            chain_id: 42161,
            rpc_url: "https://arb1.arbitrum.io/rpc".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://arbiscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 1, // Arbitrum has ~250ms blocks
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9".into(), decimals: 6,
                }),
                ("0xaf88d065e77c8cC2239327C5EDb3A432268e5831".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "optimism".into(),
            name: "Optimism".into(),
            chain_id: 10,
            rpc_url: "https://mainnet.optimism.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://optimistic.etherscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x94b008aA00579c1307B0EF2c499aD98a8ce58e58".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x94b008aA00579c1307B0EF2c499aD98a8ce58e58".into(), decimals: 6,
                }),
                ("0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "base".into(),
            name: "Base".into(),
            chain_id: 8453,
            rpc_url: "https://mainnet.base.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://basescan.org".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "polygon".into(),
            name: "Polygon PoS".into(),
            chain_id: 137,
            rpc_url: "https://polygon-rpc.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://polygonscan.com".into()),
            required_confirmations: 128,
            block_time_secs: 2,
            native_token: "MATIC".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xc2132D05D31c914a87C6611C10748AEb04B58e8F".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xc2132D05D31c914a87C6611C10748AEb04B58e8F".into(), decimals: 6,
                }),
                ("0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "avalanche".into(),
            name: "Avalanche C-Chain".into(),
            chain_id: 43114,
            rpc_url: "https://api.avax.network/ext/bc/C/rpc".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://snowtrace.io".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "AVAX".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7".into(), decimals: 6,
                }),
                ("0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "bsc".into(),
            name: "BNB Smart Chain".into(),
            chain_id: 56,
            rpc_url: "https://bsc-dataseed.binance.org".into(),
            fallback_rpc_urls: vec!["https://bsc-dataseed1.defibit.io".into()],
            explorer_url: Some("https://bscscan.com".into()),
            required_confirmations: 15,
            block_time_secs: 3,
            native_token: "BNB".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x55d398326f99059fF775485246999027B3197955".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x55d398326f99059fF775485246999027B3197955".into(), decimals: 18,
                }),
                ("0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "zksync".into(),
            name: "zkSync Era".into(),
            chain_id: 324,
            rpc_url: "https://mainnet.era.zksync.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.zksync.io".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "linea".into(),
            name: "Linea".into(),
            chain_id: 59144,
            rpc_url: "https://rpc.linea.build".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://lineascan.build".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "scroll".into(),
            name: "Scroll".into(),
            chain_id: 534352,
            rpc_url: "https://rpc.scroll.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://scrollscan.com".into()),
            required_confirmations: 12,
            block_time_secs: 3,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "mantle".into(),
            name: "Mantle".into(),
            chain_id: 5000,
            rpc_url: "https://rpc.mantle.xyz".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.mantle.xyz".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "MNT".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        // Testnets
        EvmChainDef {
            id: "sepolia".into(),
            name: "Sepolia Testnet".into(),
            chain_id: 11155111,
            rpc_url: "https://rpc.sepolia.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://sepolia.etherscan.io".into()),
            required_confirmations: 3,
            block_time_secs: 12,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_chains() {
        let registry = EvmChainRegistry::with_defaults();
        assert!(registry.len() >= 8); // At least 8 default chains
    }

    #[test]
    fn get_by_id() {
        let registry = EvmChainRegistry::with_defaults();
        let arb = registry.get("arbitrum").unwrap();
        assert_eq!(arb.chain_id, 42161);
        assert_eq!(arb.name, "Arbitrum One");
    }

    #[test]
    fn get_by_chain_id() {
        let registry = EvmChainRegistry::with_defaults();
        let base = registry.get_by_chain_id(8453).unwrap();
        assert_eq!(base.id, "base");
    }

    #[test]
    fn mainnet_chains_exclude_testnets() {
        let registry = EvmChainRegistry::with_defaults();
        let mainnets = registry.mainnet_chains();
        assert!(mainnets.iter().all(|c| !c.testnet));
        assert!(!mainnets.is_empty());
    }

    #[test]
    fn all_chains_have_correct_native_decimals() {
        let registry = EvmChainRegistry::with_defaults();
        for chain in registry.enabled_chains() {
            assert_eq!(chain.native_decimals, 18, "EVM chains should have 18 decimals for native token: {}", chain.id);
        }
    }

    #[test]
    fn usdt_decimals_correct_per_chain() {
        let registry = EvmChainRegistry::with_defaults();
        for chain in registry.enabled_chains() {
            for token in chain.tokens.values() {
                if token.symbol == "USDT" {
                    // BSC USDT is 18 decimals, all others are 6
                    if chain.id == "bsc" {
                        assert_eq!(token.decimals, 18, "BSC USDT should be 18 decimals");
                    } else {
                        assert_eq!(token.decimals, 6, "USDT should be 6 decimals on {}", chain.id);
                    }
                }
            }
        }
    }

    #[test]
    fn register_custom_chain() {
        let mut registry = EvmChainRegistry::new();
        assert_eq!(registry.len(), 0);

        registry.register(EvmChainDef {
            id: "custom".into(),
            name: "Custom L2".into(),
            chain_id: 99999,
            rpc_url: "http://localhost:8545".into(),
            fallback_rpc_urls: vec![],
            explorer_url: None,
            required_confirmations: 5,
            block_time_secs: 1,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        });

        assert_eq!(registry.len(), 1);
        assert!(registry.get("custom").is_some());
    }

    #[test]
    fn chain_ids_are_unique() {
        let registry = EvmChainRegistry::with_defaults();
        let mut chain_ids: Vec<u64> = registry.enabled_chains().iter().map(|c| c.chain_id).collect();
        let len_before = chain_ids.len();
        chain_ids.sort();
        chain_ids.dedup();
        assert_eq!(chain_ids.len(), len_before, "Chain IDs must be unique");
    }

    #[test]
    fn serialization_roundtrip() {
        let chain = &default_evm_chains()[1]; // Arbitrum
        let json = serde_json::to_string(chain).unwrap();
        let parsed: EvmChainDef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.chain_id, chain.chain_id);
        assert_eq!(parsed.id, chain.id);
    }
}
