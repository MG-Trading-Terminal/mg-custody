//! Multi-source price aggregator.
//!
//! Aggregates prices from multiple free sources:
//! - Binance public API (no auth needed)
//! - CoinGecko (already in price_oracle.rs)
//! - CoinMarketCap (needs API key)
//!
//! Uses median of sources for robustness.

use anyhow::Result;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::HashMap;

/// Binance ticker price response.
#[derive(Debug, Deserialize)]
pub struct BinanceTicker {
    pub symbol: String,
    pub price: String,
}

/// Fetch prices from Binance public API (no auth needed).
pub async fn fetch_binance_prices(client: &reqwest::Client) -> Result<HashMap<String, Decimal>> {
    let url = "https://api.binance.com/api/v3/ticker/price";

    let tickers: Vec<BinanceTicker> = client
        .get(url)
        .send()
        .await?
        .json()
        .await?;

    // Map USDT pairs to our symbols
    let target_pairs: HashMap<&str, &str> = HashMap::from([
        ("BTCUSDT", "BTC"),
        ("ETHUSDT", "ETH"),
        ("SOLUSDT", "SOL"),
        ("TRXUSDT", "TRX"),
        ("BNBUSDT", "BNB"),
        ("AVAXUSDT", "AVAX"),
        ("MATICUSDT", "MATIC"),
        ("MNTUSDT", "MNT"),
    ]);

    let mut prices = HashMap::new();

    for ticker in &tickers {
        if let Some(&symbol) = target_pairs.get(ticker.symbol.as_str())
            && let Ok(price) = ticker.price.parse::<Decimal>() {
                prices.insert(symbol.to_string(), price);
            }
    }

    // Stablecoins always $1
    prices.insert("USDT".into(), Decimal::ONE);
    prices.insert("USDC".into(), Decimal::ONE);

    Ok(prices)
}

/// Take the median of multiple price sources for robustness.
pub fn median_price(prices: &[Decimal]) -> Option<Decimal> {
    if prices.is_empty() {
        return None;
    }
    let mut sorted = prices.to_vec();
    sorted.sort();
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((sorted[mid - 1] + sorted[mid]) / Decimal::new(2, 0))
    } else {
        Some(sorted[mid])
    }
}

/// Multi-source price aggregation.
pub fn aggregate_prices(
    binance: &HashMap<String, Decimal>,
    coingecko: &HashMap<String, Decimal>,
) -> HashMap<String, Decimal> {
    let mut result = HashMap::new();

    // Get all unique symbols
    let mut symbols: Vec<String> = binance.keys().chain(coingecko.keys()).cloned().collect();
    symbols.sort();
    symbols.dedup();

    for symbol in symbols {
        let mut prices = Vec::new();
        if let Some(&p) = binance.get(&symbol) { prices.push(p); }
        if let Some(&p) = coingecko.get(&symbol) { prices.push(p); }

        if let Some(median) = median_price(&prices) {
            result.insert(symbol, median);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn median_single_value() {
        assert_eq!(median_price(&[dec!(100)]), Some(dec!(100)));
    }

    #[test]
    fn median_two_values() {
        assert_eq!(median_price(&[dec!(100), dec!(200)]), Some(dec!(150)));
    }

    #[test]
    fn median_three_values() {
        assert_eq!(median_price(&[dec!(100), dec!(300), dec!(200)]), Some(dec!(200)));
    }

    #[test]
    fn median_empty() {
        assert_eq!(median_price(&[]), None);
    }

    #[test]
    fn aggregate_two_sources() {
        let mut binance = HashMap::new();
        binance.insert("BTC".into(), dec!(65000));
        binance.insert("ETH".into(), dec!(3500));

        let mut coingecko = HashMap::new();
        coingecko.insert("BTC".into(), dec!(65100));
        coingecko.insert("SOL".into(), dec!(150));

        let result = aggregate_prices(&binance, &coingecko);
        // BTC: median of [65000, 65100] = 65050
        assert_eq!(result["BTC"], dec!(65050));
        // ETH: only binance = 3500
        assert_eq!(result["ETH"], dec!(3500));
        // SOL: only coingecko = 150
        assert_eq!(result["SOL"], dec!(150));
    }
}
