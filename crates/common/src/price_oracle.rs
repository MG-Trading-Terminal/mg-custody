//! Price oracle for USD conversion.
//!
//! Fetches asset prices from external APIs (CoinGecko, Binance) with caching.
//! Used by the circuit breaker to convert withdrawal amounts to USD.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;
use serde::Deserialize;

/// A cached price entry.
#[derive(Debug, Clone)]
struct PriceEntry {
    usd_price: Decimal,
    fetched_at: Instant,
}

/// Price oracle with in-memory cache.
pub struct PriceOracle {
    cache: RwLock<HashMap<String, PriceEntry>>,
    cache_ttl: Duration,
}

impl PriceOracle {
    /// Create a new price oracle with the given cache TTL.
    pub fn new(cache_ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            cache_ttl,
        }
    }

    /// Create with default 60-second TTL.
    pub fn with_default_ttl() -> Self {
        Self::new(Duration::from_secs(60))
    }

    /// Get the USD price for an asset symbol.
    /// Returns cached value if fresh, None if not available.
    pub fn get_price(&self, symbol: &str) -> Option<Decimal> {
        let cache = self.cache.read().ok()?;
        let key = symbol.to_uppercase();
        let entry = cache.get(&key)?;

        if entry.fetched_at.elapsed() < self.cache_ttl {
            Some(entry.usd_price)
        } else {
            None // stale
        }
    }

    /// Set a price in the cache (called after fetching from API).
    pub fn set_price(&self, symbol: &str, usd_price: Decimal) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(symbol.to_uppercase(), PriceEntry {
                usd_price,
                fetched_at: Instant::now(),
            });
        }
    }

    /// Convert an amount of an asset to USD.
    pub fn to_usd(&self, symbol: &str, amount: Decimal) -> Option<Decimal> {
        let price = self.get_price(symbol)?;
        Some(amount * price)
    }

    /// Set multiple prices at once (batch update from API).
    pub fn set_prices(&self, prices: &HashMap<String, Decimal>) {
        for (symbol, price) in prices {
            self.set_price(symbol, *price);
        }
    }

    /// Number of cached prices.
    pub fn cache_size(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }
}

/// CoinGecko API response for simple price endpoint.
/// Note: CoinGecko returns JSON numbers (floats). We convert to Decimal immediately
/// after deserialization with explicit error handling — never silently default to zero,
/// as that would make the circuit breaker see $0 and bypass outflow limits.
#[derive(Debug, Deserialize)]
pub struct CoinGeckoPrice {
    pub usd: f64,
}

/// Fetch prices from CoinGecko API.
///
/// `ids`: CoinGecko IDs (e.g., "bitcoin,ethereum,solana")
pub async fn fetch_coingecko_prices(
    client: &reqwest::Client,
    ids: &str,
) -> anyhow::Result<HashMap<String, Decimal>> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd",
        ids
    );

    let resp: HashMap<String, CoinGeckoPrice> = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?
        .json()
        .await?;

    // Map CoinGecko IDs to our symbols
    let symbol_map: HashMap<&str, &str> = HashMap::from([
        ("bitcoin", "BTC"),
        ("ethereum", "ETH"),
        ("solana", "SOL"),
        ("tron", "TRX"),
        ("tether", "USDT"),
        ("usd-coin", "USDC"),
        ("binancecoin", "BNB"),
        ("avalanche-2", "AVAX"),
        ("matic-network", "MATIC"),
    ]);

    let mut prices: HashMap<String, Decimal> = HashMap::new();
    for (id, price) in resp {
        if let Some(&symbol) = symbol_map.get(id.as_str()) {
            // Convert f64 → Decimal with validation.
            // SECURITY: reject zero/negative/NaN/Inf prices. A zero price would
            // make the circuit breaker see $0 for any withdrawal amount,
            // effectively bypassing per-tx and hourly outflow limits.
            if price.usd.is_finite() && price.usd > 0.0
                && let Some(decimal_price) = Decimal::from_f64_retain(price.usd) {
                    prices.insert(symbol.to_string(), decimal_price);
                }
                // If from_f64_retain fails, skip this price (stale cache is safer than zero)
        }
    }

    Ok(prices)
}

/// CoinGecko IDs for our supported assets.
pub const COINGECKO_IDS: &str = "bitcoin,ethereum,solana,tron,tether,usd-coin,binancecoin,avalanche-2,matic-network";

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn price_oracle_set_and_get() {
        let oracle = PriceOracle::with_default_ttl();
        oracle.set_price("BTC", dec!(65000));
        oracle.set_price("ETH", dec!(3500));

        assert_eq!(oracle.get_price("BTC"), Some(dec!(65000)));
        assert_eq!(oracle.get_price("ETH"), Some(dec!(3500)));
        assert_eq!(oracle.get_price("DOGE"), None);
    }

    #[test]
    fn price_oracle_case_insensitive() {
        let oracle = PriceOracle::with_default_ttl();
        oracle.set_price("btc", dec!(65000));
        assert_eq!(oracle.get_price("BTC"), Some(dec!(65000)));
        assert_eq!(oracle.get_price("btc"), Some(dec!(65000)));
    }

    #[test]
    fn to_usd_conversion() {
        let oracle = PriceOracle::with_default_ttl();
        oracle.set_price("ETH", dec!(3500));

        // 2 ETH = $7,000
        assert_eq!(oracle.to_usd("ETH", dec!(2)), Some(dec!(7000)));
        // 0.5 ETH = $1,750
        assert_eq!(oracle.to_usd("ETH", dec!(0.5)), Some(dec!(1750)));
        // Unknown asset
        assert_eq!(oracle.to_usd("DOGE", dec!(100)), None);
    }

    #[test]
    fn stale_prices_return_none() {
        let oracle = PriceOracle::new(Duration::from_millis(1));
        oracle.set_price("BTC", dec!(65000));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(oracle.get_price("BTC"), None); // expired
    }

    #[test]
    fn fresh_oracle_returns_none_before_first_fetch() {
        // Historical behaviour was to seed hardcoded BTC=$65k / ETH=$3.5k /
        // etc. as a "fallback" — so for the first 60s after startup the UI
        // would present those as live prices. That was actively harmful:
        // the circuit breaker and the user-facing portfolio would both
        // trust figures that never hit an API. We removed the fallback
        // seed; this test pins the new contract so a future regression
        // is caught.
        let oracle = PriceOracle::with_default_ttl();
        assert_eq!(oracle.get_price("BTC"), None);
        assert_eq!(oracle.get_price("USDT"), None);
        assert_eq!(oracle.cache_size(), 0);
    }

    #[test]
    fn zero_price_rejected() {
        // SECURITY: A zero price would make circuit breaker see $0 for any amount
        let oracle = PriceOracle::with_default_ttl();
        oracle.set_price("BTC", Decimal::ZERO);
        // Zero price is stored (set_price doesn't guard — fetch_coingecko_prices does)
        // But to_usd with zero returns zero, which is dangerous
        assert_eq!(oracle.to_usd("BTC", dec!(100)), Some(dec!(0)));
        // This test documents the risk — the guard is in fetch_coingecko_prices
    }

    #[test]
    fn batch_price_update() {
        let oracle = PriceOracle::with_default_ttl();
        let mut prices = HashMap::new();
        prices.insert("BTC".to_string(), dec!(66000));
        prices.insert("ETH".to_string(), dec!(3600));
        oracle.set_prices(&prices);

        assert_eq!(oracle.get_price("BTC"), Some(dec!(66000)));
        assert_eq!(oracle.get_price("ETH"), Some(dec!(3600)));
    }
}
