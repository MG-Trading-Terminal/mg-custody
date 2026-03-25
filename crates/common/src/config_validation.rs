//! Configuration validation.
//!
//! Validates all config values on startup. Fails fast with clear error messages.

use anyhow::{ensure, Result};
use crate::config::CustodyConfig;

/// Validate the entire custody configuration.
pub fn validate_config(config: &CustodyConfig) -> Result<Vec<String>> {
    let mut warnings = Vec::new();

    // API
    ensure!(!config.api.bind.is_empty(), "api.bind must not be empty");
    ensure!(config.api.bind.contains(':'), "api.bind must be host:port format");

    // Database
    ensure!(!config.database.url.is_empty(), "database.url must not be empty");
    ensure!(config.database.url.starts_with("postgres"), "database.url must be a PostgreSQL URL");
    ensure!(config.database.max_connections > 0, "database.max_connections must be > 0");
    if config.database.max_connections < 5 {
        warnings.push("database.max_connections < 5 may cause connection starvation".into());
    }

    // Limits
    ensure!(config.limits.max_single_withdrawal_usd > rust_decimal::Decimal::ZERO,
        "limits.max_single_withdrawal_usd must be > 0");
    ensure!(config.limits.max_hourly_outflow_usd > config.limits.max_single_withdrawal_usd,
        "limits.max_hourly_outflow_usd must be > max_single_withdrawal_usd");
    ensure!(config.limits.max_daily_outflow_usd > config.limits.max_hourly_outflow_usd,
        "limits.max_daily_outflow_usd must be > max_hourly_outflow_usd");

    // Chains
    if let Some(ref eth) = config.chains.ethereum {
        if eth.enabled {
            ensure!(!eth.rpc_url.is_empty(), "chains.ethereum.rpc_url must not be empty when enabled");
            ensure!(eth.required_confirmations >= 1, "chains.ethereum.required_confirmations must be >= 1");
            if eth.required_confirmations < 12 {
                warnings.push(format!(
                    "chains.ethereum.required_confirmations={} (recommended: 12)",
                    eth.required_confirmations
                ));
            }
            if eth.max_gas_price_gwei == 0 {
                warnings.push("chains.ethereum.max_gas_price_gwei=0 will block all withdrawals".into());
            }
        }
    }

    if let Some(ref btc) = config.chains.bitcoin {
        if btc.enabled {
            ensure!(!btc.rpc_url.is_empty(), "chains.bitcoin.rpc_url must not be empty when enabled");
            if btc.required_confirmations < 3 {
                warnings.push(format!(
                    "chains.bitcoin.required_confirmations={} (recommended: 3)",
                    btc.required_confirmations
                ));
            }
        }
    }

    // Wallets
    ensure!(config.wallets.hot_max_pct <= 100, "wallets.hot_max_pct must be <= 100");
    ensure!(config.wallets.warm_max_pct <= 100, "wallets.warm_max_pct must be <= 100");
    if config.wallets.hot_max_pct > 10 {
        warnings.push(format!(
            "wallets.hot_max_pct={}% (recommended: <= 5% for security)",
            config.wallets.hot_max_pct
        ));
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_default_config() {
        let config = CustodyConfig::load_from("config/default.toml").unwrap();
        let warnings = validate_config(&config).unwrap();
        // Default config should be valid
        for w in &warnings {
            println!("Warning: {w}");
        }
    }

    #[test]
    fn reject_empty_bind() {
        let mut config = CustodyConfig::load_from("config/default.toml").unwrap();
        config.api.bind = "".into();
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn reject_negative_limits() {
        let mut config = CustodyConfig::load_from("config/default.toml").unwrap();
        config.limits.max_single_withdrawal_usd = rust_decimal::Decimal::ZERO;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn warn_on_low_confirmations() {
        let mut config = CustodyConfig::load_from("config/default.toml").unwrap();
        if let Some(ref mut eth) = config.chains.ethereum {
            eth.required_confirmations = 3;
        }
        let warnings = validate_config(&config).unwrap();
        assert!(warnings.iter().any(|w| w.contains("recommended: 12")));
    }
}
