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
            tokens: HashMap::from([
                ("0x493257fD37EDB34451f62EDf8D2a0C418852bA4C".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x493257fD37EDB34451f62EDf8D2a0C418852bA4C".into(), decimals: 6,
                }),
                ("0x1d17CBcF0D6D143135aE902365D2E5e2A16538D4".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x1d17CBcF0D6D143135aE902365D2E5e2A16538D4".into(), decimals: 6,
                }),
            ]),
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
            tokens: HashMap::from([
                ("0xA219439258ca9da29E9Cc4cE5596924745e12B93".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xA219439258ca9da29E9Cc4cE5596924745e12B93".into(), decimals: 6,
                }),
                ("0x176211869cA2b568f2A7D4EE941E073a821EE1ff".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x176211869cA2b568f2A7D4EE941E073a821EE1ff".into(), decimals: 6,
                }),
            ]),
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
            tokens: HashMap::from([
                ("0xf55BEC9cafDbE8a36eF67A3D7ee70C6Aa7D2Da26".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xf55BEC9cafDbE8a36eF67A3D7ee70C6Aa7D2Da26".into(), decimals: 6,
                }),
                ("0x06eFdBFf2a14a7c8E15944D1F4A48F9F95F663A4".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x06eFdBFf2a14a7c8E15944D1F4A48F9F95F663A4".into(), decimals: 6,
                }),
            ]),
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
            tokens: HashMap::from([
                ("0x201EBa5CC46D216Ce6DC03F6a759e8E766e956aE".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x201EBa5CC46D216Ce6DC03F6a759e8E766e956aE".into(), decimals: 6,
                }),
                ("0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        // ── Additional EVM Chains ────────────────────────────────
        EvmChainDef {
            id: "fantom".into(),
            name: "Fantom Opera".into(),
            chain_id: 250,
            rpc_url: "https://rpc.ftm.tools".into(),
            fallback_rpc_urls: vec!["https://rpc.ankr.com/fantom".into()],
            explorer_url: Some("https://ftmscan.com".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "FTM".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x049d68029688eAbF473097a2fC38ef61633A3C7A".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x049d68029688eAbF473097a2fC38ef61633A3C7A".into(), decimals: 6,
                }),
                ("0x04068DA6C83AFCFA0e13ba15A6696662335D5B75".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x04068DA6C83AFCFA0e13ba15A6696662335D5B75".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "cronos".into(),
            name: "Cronos".into(),
            chain_id: 25,
            rpc_url: "https://evm.cronos.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://cronoscan.com".into()),
            required_confirmations: 12,
            block_time_secs: 6,
            native_token: "CRO".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x66e428c3f67a68878562e79A0234c1F83c208770".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x66e428c3f67a68878562e79A0234c1F83c208770".into(), decimals: 6,
                }),
                ("0xc21223249CA28397B4B6541dfFaEcC539BfF0c59".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xc21223249CA28397B4B6541dfFaEcC539BfF0c59".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "gnosis".into(),
            name: "Gnosis Chain".into(),
            chain_id: 100,
            rpc_url: "https://rpc.gnosischain.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://gnosisscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 5,
            native_token: "xDAI".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x4ECaBa5870353805a9F068101A40E0f32ed605C6".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x4ECaBa5870353805a9F068101A40E0f32ed605C6".into(), decimals: 6,
                }),
                ("0xDDAfbb505ad214D7b80b1f830fcCc89B60fb7A83".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xDDAfbb505ad214D7b80b1f830fcCc89B60fb7A83".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "celo".into(),
            name: "Celo".into(),
            chain_id: 42220,
            rpc_url: "https://forno.celo.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://celoscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 5,
            native_token: "CELO".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x48065fbBE25f71C9282ddf5e1cD6D6A887483D5e".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x48065fbBE25f71C9282ddf5e1cD6D6A887483D5e".into(), decimals: 6,
                }),
                ("0xcebA9300f2b948710d2653dD7B07f33A8B32118C".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xcebA9300f2b948710d2653dD7B07f33A8B32118C".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "kava".into(),
            name: "Kava EVM".into(),
            chain_id: 2222,
            rpc_url: "https://evm.kava.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://kavascan.com".into()),
            required_confirmations: 12,
            block_time_secs: 6,
            native_token: "KAVA".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x919C1c267BC06a7039e03fcc2eF738525769109c".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x919C1c267BC06a7039e03fcc2eF738525769109c".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "moonbeam".into(),
            name: "Moonbeam".into(),
            chain_id: 1284,
            rpc_url: "https://rpc.api.moonbeam.network".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://moonscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 12,
            native_token: "GLMR".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xFFFFFFfFea09FB06d082fd1275CD48b191cbCD1d".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xFFFFFFfFea09FB06d082fd1275CD48b191cbCD1d".into(), decimals: 6,
                }),
                ("0x931715FEE2d06333043d11F658C8CE934aC61D0c".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x931715FEE2d06333043d11F658C8CE934aC61D0c".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "blast".into(),
            name: "Blast".into(),
            chain_id: 81457,
            rpc_url: "https://rpc.blast.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://blastscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x4300000000000000000000000000000000000003".into(), EvmTokenDef {
                    symbol: "USDB".into(), contract_address: "0x4300000000000000000000000000000000000003".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "metis".into(),
            name: "Metis Andromeda".into(),
            chain_id: 1088,
            rpc_url: "https://andromeda.metis.io/?owner=1088".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://andromeda-explorer.metis.io".into()),
            required_confirmations: 12,
            block_time_secs: 4,
            native_token: "METIS".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xbB06DCA3AE6887fAbF931640f67cab3e3a16F4dC".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xbB06DCA3AE6887fAbF931640f67cab3e3a16F4dC".into(), decimals: 6,
                }),
                ("0xEA32A96608495e54156Ae48931A7c0f0046d1e99".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xEA32A96608495e54156Ae48931A7c0f0046d1e99".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "manta".into(),
            name: "Manta Pacific".into(),
            chain_id: 169,
            rpc_url: "https://pacific-rpc.manta.network/http".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://pacific-explorer.manta.network".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xf417F5A458eC102B90352F697D6e2Ac3A3d2851f".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xf417F5A458eC102B90352F697D6e2Ac3A3d2851f".into(), decimals: 6,
                }),
                ("0xb73603C5d87fA094B7314C74ACE2e64D165016fb".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xb73603C5d87fA094B7314C74ACE2e64D165016fb".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "opbnb".into(),
            name: "opBNB".into(),
            chain_id: 204,
            rpc_url: "https://opbnb-mainnet-rpc.bnbchain.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://opbnbscan.com".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "BNB".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x9e5AAC1Ba1a2e6aEd6b32689DFcF62A509Ca96f3".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x9e5AAC1Ba1a2e6aEd6b32689DFcF62A509Ca96f3".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "polygon_zkevm".into(),
            name: "Polygon zkEVM".into(),
            chain_id: 1101,
            rpc_url: "https://zkevm-rpc.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://zkevm.polygonscan.com".into()),
            required_confirmations: 12,
            block_time_secs: 5,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x1E4a5963aBFD975d8c9021ce480b42188849D41d".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x1E4a5963aBFD975d8c9021ce480b42188849D41d".into(), decimals: 6,
                }),
                ("0xA8CE8aee21bC2A48a5EF670afCc9274C7bbbC035".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xA8CE8aee21bC2A48a5EF670afCc9274C7bbbC035".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "aurora".into(),
            name: "Aurora (NEAR EVM)".into(),
            chain_id: 1313161554,
            rpc_url: "https://mainnet.aurora.dev".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.aurora.dev".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x4988a896b1227218e4A686fdE5EabdcAbd91571f".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x4988a896b1227218e4A686fdE5EabdcAbd91571f".into(), decimals: 6,
                }),
                ("0xB12BFcA5A55806AaF64E99521918A4bf0fC40802".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xB12BFcA5A55806AaF64E99521918A4bf0fC40802".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "klaytn".into(),
            name: "Klaytn".into(),
            chain_id: 8217,
            rpc_url: "https://public-en-cypress.klaytn.net".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://klaytnscope.com".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "KLAY".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xceE8FAF64bB97a73bb51E115Aa89C17FfA8dD167".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xceE8FAF64bB97a73bb51E115Aa89C17FfA8dD167".into(), decimals: 6,
                }),
                ("0x6270B58BE569a7c0b8f47594F191631Ae5b2C86C".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x6270B58BE569a7c0b8f47594F191631Ae5b2C86C".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "filecoin".into(),
            name: "Filecoin EVM".into(),
            chain_id: 314,
            rpc_url: "https://api.node.glif.io/rpc/v1".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://filfox.info".into()),
            required_confirmations: 12,
            block_time_secs: 30,
            native_token: "FIL".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "mode".into(),
            name: "Mode".into(),
            chain_id: 34443,
            rpc_url: "https://mainnet.mode.network".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.mode.network".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xd988097fb8612cc24eeC14542bC03424c656005f".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xd988097fb8612cc24eeC14542bC03424c656005f".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "zetachain".into(),
            name: "ZetaChain".into(),
            chain_id: 7000,
            rpc_url: "https://zetachain-evm.blockpi.network/v1/rpc/public".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.zetachain.com".into()),
            required_confirmations: 12,
            block_time_secs: 5,
            native_token: "ZETA".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "core".into(),
            name: "Core".into(),
            chain_id: 1116,
            rpc_url: "https://rpc.coredao.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://scan.coredao.org".into()),
            required_confirmations: 12,
            block_time_secs: 3,
            native_token: "CORE".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x900101d06A7426441Ae63e9AB3B9b0F63Be145F1".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x900101d06A7426441Ae63e9AB3B9b0F63Be145F1".into(), decimals: 6,
                }),
                ("0xa4151B2B3e269645b82e46271060C0aBD22309E8".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0xa4151B2B3e269645b82e46271060C0aBD22309E8".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "flare".into(),
            name: "Flare".into(),
            chain_id: 14,
            rpc_url: "https://flare-api.flare.network/ext/C/rpc".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://flare-explorer.flare.network".into()),
            required_confirmations: 12,
            block_time_secs: 3,
            native_token: "FLR".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "rootstock".into(),
            name: "Rootstock (Bitcoin sidechain)".into(),
            chain_id: 30,
            rpc_url: "https://public-node.rsk.co".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.rsk.co".into()),
            required_confirmations: 12,
            block_time_secs: 30,
            native_token: "RBTC".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xEf213441a85DF4d7acBdAe0Cf78004E1e486BB96".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xEf213441a85DF4d7acBdAe0Cf78004E1e486BB96".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "sei".into(),
            name: "Sei".into(),
            chain_id: 1329,
            rpc_url: "https://evm-rpc.sei-apis.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://seitrace.com".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "SEI".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x3894085Ef7Ff0f0aeDf52E2A2704928d1Ec074F2".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x3894085Ef7Ff0f0aeDf52E2A2704928d1Ec074F2".into(), decimals: 6,
                }),
                ("0xB75D0B03c06A926e488e2659DF1A861F860bD3d1".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xB75D0B03c06A926e488e2659DF1A861F860bD3d1".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "taiko".into(),
            name: "Taiko".into(),
            chain_id: 167000,
            rpc_url: "https://rpc.mainnet.taiko.xyz".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://taikoscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 12,
            native_token: "ETH".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x2DEF195713CF4a606B49D07E520e22C17899a736".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x2DEF195713CF4a606B49D07E520e22C17899a736".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "pulsechain".into(),
            name: "PulseChain".into(),
            chain_id: 369,
            rpc_url: "https://rpc.pulsechain.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://otter.pulsechain.com".into()),
            required_confirmations: 12,
            block_time_secs: 10,
            native_token: "PLS".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x0Cb6F5a34ad42ec934882A05265A7d5F59b51A2f".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x0Cb6F5a34ad42ec934882A05265A7d5F59b51A2f".into(), decimals: 6,
                }),
                ("0x15D38573d2feeb82e7ad5187aB8c1D52810B1f07".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x15D38573d2feeb82e7ad5187aB8c1D52810B1f07".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "evmos".into(),
            name: "Evmos".into(),
            chain_id: 9001,
            rpc_url: "https://evmos-evm-rpc.publicnode.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://www.mintscan.io/evmos".into()),
            required_confirmations: 12,
            block_time_secs: 6,
            native_token: "EVMOS".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "telos".into(),
            name: "Telos EVM".into(),
            chain_id: 40,
            rpc_url: "https://mainnet.telos.net/evm".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://www.teloscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "TLOS".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xeFAeeE334F0Fd1712f9a8cc375f427D9Cdd40d73".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xeFAeeE334F0Fd1712f9a8cc375f427D9Cdd40d73".into(), decimals: 6,
                }),
                ("0x818ec0A7Fe18Ff94269904fCED6AE3DaE6d6dC0b".into(), EvmTokenDef {
                    symbol: "USDC".into(), contract_address: "0x818ec0A7Fe18Ff94269904fCED6AE3DaE6d6dC0b".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "iotex".into(),
            name: "IoTeX".into(),
            chain_id: 4689,
            rpc_url: "https://babel-api.mainnet.iotex.io".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://iotexscan.io".into()),
            required_confirmations: 12,
            block_time_secs: 5,
            native_token: "IOTX".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x6fbCdc1169B5130C59E72E51Ed68A84841C98cd1".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x6fbCdc1169B5130C59E72E51Ed68A84841C98cd1".into(), decimals: 6,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "okxchain".into(),
            name: "OKX Chain".into(),
            chain_id: 66,
            rpc_url: "https://exchainrpc.okex.org".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://www.oklink.com/okc".into()),
            required_confirmations: 12,
            block_time_secs: 4,
            native_token: "OKT".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0x382bB369d343125BfB2117af9c149795C6C65C50".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0x382bB369d343125BfB2117af9c149795C6C65C50".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "heco".into(),
            name: "Huobi ECO Chain".into(),
            chain_id: 128,
            rpc_url: "https://http-mainnet.hecochain.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://www.hecoinfo.com".into()),
            required_confirmations: 12,
            block_time_secs: 3,
            native_token: "HT".into(),
            native_decimals: 18,
            tokens: HashMap::from([
                ("0xa71EdC38d189767582C38A3145b5873052c3e47a".into(), EvmTokenDef {
                    symbol: "USDT".into(), contract_address: "0xa71EdC38d189767582C38A3145b5873052c3e47a".into(), decimals: 18,
                }),
            ]),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "wemix".into(),
            name: "WEMIX3.0".into(),
            chain_id: 1111,
            rpc_url: "https://api.wemix.com".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.wemix.com".into()),
            required_confirmations: 12,
            block_time_secs: 1,
            native_token: "WEMIX".into(),
            native_decimals: 18,
            tokens: HashMap::new(),
            enabled: true,
            testnet: false,
        },
        EvmChainDef {
            id: "harmony".into(),
            name: "Harmony".into(),
            chain_id: 1666600000,
            rpc_url: "https://api.harmony.one".into(),
            fallback_rpc_urls: vec![],
            explorer_url: Some("https://explorer.harmony.one".into()),
            required_confirmations: 12,
            block_time_secs: 2,
            native_token: "ONE".into(),
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
                    // BSC, opBNB, Rootstock, Heco, OKX USDT are 18 decimals, all others are 6
                    if matches!(chain.id.as_str(), "bsc" | "opbnb" | "rootstock" | "heco" | "okxchain") {
                        assert_eq!(token.decimals, 18, "BSC USDT should be 18 decimals");
                    } else {
                        assert_eq!(token.decimals, 6, "USDT should be 6 decimals on {}", chain.id);
                    }
                }
            }
        }
    }

    #[test]
    fn all_evm_chains_have_stablecoins() {
        let registry = EvmChainRegistry::with_defaults();
        let chains_needing_stables = ["ethereum", "arbitrum", "optimism", "base", "polygon", "avalanche", "bsc", "zksync", "linea", "scroll", "mantle", "fantom", "cronos", "gnosis", "celo", "metis", "manta"];

        for chain in registry.enabled_chains() {
            if !chains_needing_stables.contains(&chain.id.as_str()) { continue; }
            let has_usdt = chain.tokens.values().any(|t| t.symbol == "USDT");
            let has_usdc = chain.tokens.values().any(|t| t.symbol == "USDC");
            // Base doesn't have official USDT
            if chain.id != "base" {
                assert!(has_usdt, "Chain {} missing USDT", chain.id);
            }
            assert!(has_usdc, "Chain {} missing USDC", chain.id);
        }

        // Verify all target chains exist
        let chain_ids: Vec<&str> = registry.enabled_chains().iter().map(|c| c.id.as_str()).collect();
        for id in &chains_needing_stables {
            assert!(chain_ids.contains(id), "Chain {} should be in registry", id);
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
