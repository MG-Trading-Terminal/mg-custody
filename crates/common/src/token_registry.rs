//! Dynamic token registry.
//!
//! Maps (asset_symbol, chain) → (contract_address, decimals).
//! Loaded from config at startup, can be extended at runtime.
//! The single source of truth for token metadata across the system.

use std::collections::HashMap;

use crate::Chain;

/// Token metadata.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub symbol: String,
    pub chain: Chain,
    /// Contract address (None for native tokens like ETH, BTC, SOL, TRX).
    pub contract_address: Option<String>,
    pub decimals: u8,
    /// Whether this is the chain's native token.
    pub is_native: bool,
}

/// Token registry for all supported tokens across all chains.
#[derive(Debug, Clone)]
pub struct TokenRegistry {
    /// Key: "SYMBOL:chain" (e.g., "USDT:ethereum", "ETH:ethereum")
    tokens: HashMap<String, TokenInfo>,
}

impl TokenRegistry {
    pub fn new() -> Self {
        Self { tokens: HashMap::new() }
    }

    /// Create a registry pre-populated with all known tokens.
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();

        // Native tokens
        reg.register(TokenInfo { symbol: "BTC".into(), chain: Chain::Bitcoin, contract_address: None, decimals: 8, is_native: true });
        reg.register(TokenInfo { symbol: "ETH".into(), chain: Chain::Ethereum, contract_address: None, decimals: 18, is_native: true });
        reg.register(TokenInfo { symbol: "TRX".into(), chain: Chain::Tron, contract_address: None, decimals: 6, is_native: true });
        reg.register(TokenInfo { symbol: "SOL".into(), chain: Chain::Solana, contract_address: None, decimals: 9, is_native: true });

        // Ethereum ERC-20
        reg.register(TokenInfo { symbol: "USDT".into(), chain: Chain::Ethereum, contract_address: Some("0xdAC17F958D2ee523a2206206994597C13D831ec7".into()), decimals: 6, is_native: false });
        reg.register(TokenInfo { symbol: "USDC".into(), chain: Chain::Ethereum, contract_address: Some("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".into()), decimals: 6, is_native: false });

        // Tron TRC-20
        reg.register(TokenInfo { symbol: "USDT".into(), chain: Chain::Tron, contract_address: Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".into()), decimals: 6, is_native: false });
        reg.register(TokenInfo { symbol: "USDC".into(), chain: Chain::Tron, contract_address: Some("TEkxiTehnzSmSe2XqrBj4w32RUN966rdz8".into()), decimals: 6, is_native: false });

        // Solana SPL
        reg.register(TokenInfo { symbol: "USDT".into(), chain: Chain::Solana, contract_address: Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".into()), decimals: 6, is_native: false });
        reg.register(TokenInfo { symbol: "USDC".into(), chain: Chain::Solana, contract_address: Some("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".into()), decimals: 6, is_native: false });

        reg
    }

    /// Register a token.
    pub fn register(&mut self, info: TokenInfo) {
        let key = Self::key(&info.symbol, info.chain);
        self.tokens.insert(key, info);
    }

    /// Look up a token by symbol and chain.
    pub fn get(&self, symbol: &str, chain: Chain) -> Option<&TokenInfo> {
        self.tokens.get(&Self::key(symbol, chain))
    }

    /// Get contract address for a token (None for native tokens).
    pub fn contract_address(&self, symbol: &str, chain: Chain) -> Option<&str> {
        self.get(symbol, chain)
            .and_then(|t| t.contract_address.as_deref())
    }

    /// Get decimals for a token.
    pub fn decimals(&self, symbol: &str, chain: Chain) -> Option<u8> {
        self.get(symbol, chain).map(|t| t.decimals)
    }

    /// Check if a token is native to its chain.
    pub fn is_native(&self, symbol: &str, chain: Chain) -> bool {
        self.get(symbol, chain).map(|t| t.is_native).unwrap_or(false)
    }

    /// List all tokens for a chain.
    pub fn tokens_for_chain(&self, chain: Chain) -> Vec<&TokenInfo> {
        self.tokens.values().filter(|t| t.chain == chain).collect()
    }

    /// Total registered tokens.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    fn key(symbol: &str, chain: Chain) -> String {
        format!("{}:{}", symbol.to_uppercase(), chain.as_str())
    }
}

impl Default for TokenRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_tokens() {
        let reg = TokenRegistry::with_defaults();
        assert!(reg.len() >= 10);
    }

    #[test]
    fn native_token_lookup() {
        let reg = TokenRegistry::with_defaults();

        let eth = reg.get("ETH", Chain::Ethereum).unwrap();
        assert!(eth.is_native);
        assert_eq!(eth.decimals, 18);
        assert!(eth.contract_address.is_none());

        let btc = reg.get("BTC", Chain::Bitcoin).unwrap();
        assert!(btc.is_native);
        assert_eq!(btc.decimals, 8);
    }

    #[test]
    fn erc20_token_lookup() {
        let reg = TokenRegistry::with_defaults();

        let usdt = reg.get("USDT", Chain::Ethereum).unwrap();
        assert!(!usdt.is_native);
        assert_eq!(usdt.decimals, 6);
        assert!(usdt.contract_address.as_ref().unwrap().starts_with("0x"));
    }

    #[test]
    fn same_symbol_different_chains() {
        let reg = TokenRegistry::with_defaults();

        let usdt_eth = reg.get("USDT", Chain::Ethereum).unwrap();
        let usdt_tron = reg.get("USDT", Chain::Tron).unwrap();
        let usdt_sol = reg.get("USDT", Chain::Solana).unwrap();

        assert_ne!(usdt_eth.contract_address, usdt_tron.contract_address);
        assert_ne!(usdt_tron.contract_address, usdt_sol.contract_address);
        assert_eq!(usdt_eth.decimals, 6);
        assert_eq!(usdt_tron.decimals, 6);
        assert_eq!(usdt_sol.decimals, 6);
    }

    #[test]
    fn tokens_for_chain() {
        let reg = TokenRegistry::with_defaults();
        let eth_tokens = reg.tokens_for_chain(Chain::Ethereum);
        assert!(eth_tokens.len() >= 3); // ETH + USDT + USDC
    }

    #[test]
    fn contract_address_helper() {
        let reg = TokenRegistry::with_defaults();
        assert!(reg.contract_address("ETH", Chain::Ethereum).is_none()); // native
        assert!(reg.contract_address("USDT", Chain::Ethereum).is_some()); // ERC-20
    }

    #[test]
    fn unknown_token_returns_none() {
        let reg = TokenRegistry::with_defaults();
        assert!(reg.get("DOGE", Chain::Ethereum).is_none());
    }

    #[test]
    fn register_custom_token() {
        let mut reg = TokenRegistry::with_defaults();
        let before = reg.len();

        reg.register(TokenInfo {
            symbol: "WBTC".into(),
            chain: Chain::Ethereum,
            contract_address: Some("0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599".into()),
            decimals: 8,
            is_native: false,
        });

        assert_eq!(reg.len(), before + 1);
        assert_eq!(reg.decimals("WBTC", Chain::Ethereum), Some(8));
    }

    #[test]
    fn case_insensitive_symbol() {
        let reg = TokenRegistry::with_defaults();
        // Key is uppercased internally
        assert!(reg.get("eth", Chain::Ethereum).is_some());
        assert!(reg.get("Eth", Chain::Ethereum).is_some());
        assert!(reg.get("ETH", Chain::Ethereum).is_some());
    }
}
