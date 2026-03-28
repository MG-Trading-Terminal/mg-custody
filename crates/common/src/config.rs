//! Typed configuration for the custody service.
//!
//! Loaded from `config/default.toml` with environment variable overrides
//! via the `CUSTODY_` prefix (e.g., `CUSTODY_DATABASE__URL`).

use std::collections::HashMap;

use figment::{Figment, providers::{Env, Format, Toml}};
use rust_decimal::Decimal;
use serde::Deserialize;

/// Top-level custody service configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CustodyConfig {
    pub api: ApiConfig,
    pub database: DatabaseConfig,
    pub exchange: ExchangeConfig,
    pub key_manager: KeyManagerConfig,
    pub limits: LimitsConfig,
    pub wallets: WalletsConfig,
    pub chains: ChainsConfig,
    #[serde(default)]
    pub background: BackgroundConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackgroundConfig {
    #[serde(default = "default_12")]
    pub confirmation_interval_secs: u64,
    #[serde(default = "default_5")]
    pub pipeline_interval_secs: u64,
    #[serde(default = "default_30")]
    pub webhook_retry_interval_secs: u64,
    #[serde(default = "default_600")]
    pub reconciliation_interval_secs: u64,
    #[serde(default = "default_120")]
    pub gas_station_interval_secs: u64,
    #[serde(default = "default_30_days")]
    pub webhook_cleanup_days: i32,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            confirmation_interval_secs: 12,
            pipeline_interval_secs: 5,
            webhook_retry_interval_secs: 30,
            reconciliation_interval_secs: 600,
            gas_station_interval_secs: 120,
            webhook_cleanup_days: 30,
        }
    }
}

fn default_12() -> u64 { 12 }
fn default_5() -> u64 { 5 }
fn default_30() -> u64 { 30 }
fn default_600() -> u64 { 600 }
fn default_120() -> u64 { 120 }
fn default_30_days() -> i32 { 30 }

impl CustodyConfig {
    /// Load config from default.toml + environment overrides.
    pub fn load() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Toml::file("config/default.toml"))
            .merge(Env::prefixed("CUSTODY_").split("__"))
            .extract()
    }

    /// Load config from a specific TOML file (for testing).
    pub fn load_from(path: &str) -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("CUSTODY_").split("__"))
            .extract()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExchangeConfig {
    pub webhook_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyManagerConfig {
    pub backend: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    pub max_single_withdrawal_usd: Decimal,
    pub max_hourly_outflow_usd: Decimal,
    pub max_daily_outflow_usd: Decimal,
    pub auto_approve_threshold_usd: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WalletsConfig {
    pub hot_max_pct: u32,
    pub warm_max_pct: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainsConfig {
    pub bitcoin: Option<BitcoinChainConfig>,
    pub ethereum: Option<EthereumChainConfig>,
    pub tron: Option<TronChainConfig>,
    pub solana: Option<SolanaChainConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BitcoinChainConfig {
    pub enabled: bool,
    pub rpc_url: String,
    pub required_confirmations: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EthereumChainConfig {
    pub enabled: bool,
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub required_confirmations: u64,
    pub max_gas_price_gwei: u64,
    #[serde(default)]
    pub tokens: HashMap<String, TokenEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenEntry {
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TronChainConfig {
    pub enabled: bool,
    pub api_url: String,
    pub required_confirmations: u64,
    #[serde(default)]
    pub tokens: HashMap<String, TokenEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SolanaChainConfig {
    pub enabled: bool,
    pub rpc_url: String,
    pub commitment: String,
    #[serde(default)]
    pub tokens: HashMap<String, TokenEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_config() {
        let config = CustodyConfig::load_from("config/default.toml").unwrap();
        assert_eq!(config.api.bind, "0.0.0.0:8443");
        assert_eq!(config.database.max_connections, 20);

        let eth = config.chains.ethereum.unwrap();
        assert!(eth.enabled);
        assert_eq!(eth.required_confirmations, 12);
        assert_eq!(eth.max_gas_price_gwei, 200);
        assert_eq!(eth.tokens.len(), 2);
        assert_eq!(eth.tokens.get("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap().symbol, "USDT");
        assert_eq!(eth.tokens.get("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap().decimals, 6);

        let btc = config.chains.bitcoin.unwrap();
        assert!(btc.enabled);
        assert_eq!(btc.required_confirmations, 3);

        let sol = config.chains.solana.unwrap();
        assert_eq!(sol.commitment, "finalized");

        let limits = config.limits;
        assert_eq!(limits.max_single_withdrawal_usd, Decimal::new(500000, 0));
    }
}
