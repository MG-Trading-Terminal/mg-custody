//! Price engine — multi-source price aggregation, Chainlink/Pyth/CoinGecko.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Chain;

/// A price quote from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceQuote {
    pub symbol: String,
    pub price_usd: Decimal,
    pub source: PriceSource,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    Chainlink,
    Pyth,
    CoinGecko,
    Binance,
    Internal,
}

/// Chainlink price feed configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainlinkFeed {
    pub pair: String,
    pub contract_address: String,
    pub chain: Chain,
    pub decimals: u8,
    pub heartbeat_secs: u32,
}

/// Known Chainlink price feeds on Ethereum mainnet.
pub fn chainlink_feeds() -> Vec<ChainlinkFeed> {
    vec![
        ChainlinkFeed { pair: "ETH/USD".into(), contract_address: "0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "BTC/USD".into(), contract_address: "0xF4030086522a5bEEa4988F8cA5B36dbC97BeE88c".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "LINK/USD".into(), contract_address: "0x2c1d072e956AFFC0D435Cb7AC38EF18d24d9127c".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "USDT/USD".into(), contract_address: "0x3E7d1eAB13ad0104d2750B8863b489D65364e32D".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 86400 },
        ChainlinkFeed { pair: "USDC/USD".into(), contract_address: "0x8fFfFfd4AfB6115b954Bd326cbe7B4BA576818f6".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 86400 },
        ChainlinkFeed { pair: "SOL/USD".into(), contract_address: "0x4ffC43a60e009B551865A93d232E33Fce9f01507".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "DAI/USD".into(), contract_address: "0xAed0c38402a5d19df6E4c03F4E2DceD6e29c1ee9".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "AAVE/USD".into(), contract_address: "0x547a514d5e3769680Ce22B2361c10Ea13619e8a9".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "UNI/USD".into(), contract_address: "0x553303d460EE0afB37EdFf9bE42922D8FF63220e".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
        ChainlinkFeed { pair: "MKR/USD".into(), contract_address: "0xec1D1B3b0443256cc3860e24a46F108e699484Aa".into(), chain: Chain::Ethereum, decimals: 8, heartbeat_secs: 3600 },
    ]
}

/// Pyth price feed IDs (Solana + cross-chain).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythFeed {
    pub symbol: String,
    pub feed_id: String,
    pub decimals: i32,
}

pub fn pyth_feeds() -> Vec<PythFeed> {
    vec![
        PythFeed { symbol: "BTC/USD".into(), feed_id: "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43".into(), decimals: -8 },
        PythFeed { symbol: "ETH/USD".into(), feed_id: "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace".into(), decimals: -8 },
        PythFeed { symbol: "SOL/USD".into(), feed_id: "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d".into(), decimals: -8 },
        PythFeed { symbol: "USDT/USD".into(), feed_id: "2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b".into(), decimals: -8 },
        PythFeed { symbol: "USDC/USD".into(), feed_id: "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a".into(), decimals: -8 },
    ]
}

/// Aggregate price from multiple sources (median).
pub fn aggregate_prices(quotes: &[PriceQuote]) -> Option<Decimal> {
    if quotes.is_empty() { return None; }
    let mut prices: Vec<Decimal> = quotes.iter().map(|q| q.price_usd).collect();
    prices.sort();
    let mid = prices.len() / 2;
    if prices.len() % 2 == 0 {
        Some((prices[mid - 1] + prices[mid]) / Decimal::new(2, 0))
    } else {
        Some(prices[mid])
    }
}

/// Check if a price is stale (older than max_age_secs).
pub fn is_stale(quote: &PriceQuote, max_age_secs: i64) -> bool {
    let age = Utc::now() - quote.timestamp;
    age.num_seconds() > max_age_secs
}

/// Calculate portfolio value in USD.
pub fn portfolio_value(holdings: &[(String, Decimal, Decimal)]) -> Decimal {
    // holdings: [(symbol, amount, price_usd)]
    holdings.iter().map(|(_, amount, price)| amount * price).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn chainlink_feeds_count() {
        assert!(chainlink_feeds().len() >= 10);
    }

    #[test]
    fn pyth_feeds_count() {
        assert!(pyth_feeds().len() >= 5);
    }

    #[test]
    fn aggregate_median_odd() {
        let quotes = vec![
            PriceQuote { symbol: "ETH".into(), price_usd: dec!(3000), source: PriceSource::Chainlink, timestamp: Utc::now(), confidence: None },
            PriceQuote { symbol: "ETH".into(), price_usd: dec!(3010), source: PriceSource::Pyth, timestamp: Utc::now(), confidence: None },
            PriceQuote { symbol: "ETH".into(), price_usd: dec!(2990), source: PriceSource::CoinGecko, timestamp: Utc::now(), confidence: None },
        ];
        assert_eq!(aggregate_prices(&quotes), Some(dec!(3000)));
    }

    #[test]
    fn aggregate_median_even() {
        let quotes = vec![
            PriceQuote { symbol: "BTC".into(), price_usd: dec!(60000), source: PriceSource::Chainlink, timestamp: Utc::now(), confidence: None },
            PriceQuote { symbol: "BTC".into(), price_usd: dec!(60100), source: PriceSource::Pyth, timestamp: Utc::now(), confidence: None },
        ];
        assert_eq!(aggregate_prices(&quotes), Some(dec!(60050)));
    }

    #[test]
    fn aggregate_empty() {
        assert_eq!(aggregate_prices(&[]), None);
    }

    #[test]
    fn portfolio_value_calc() {
        let holdings = vec![
            ("BTC".into(), dec!(1.5), dec!(60000)),
            ("ETH".into(), dec!(10), dec!(3000)),
            ("USDT".into(), dec!(50000), dec!(1)),
        ];
        assert_eq!(portfolio_value(&holdings), dec!(170000));
    }

    #[test]
    fn price_staleness() {
        let fresh = PriceQuote {
            symbol: "ETH".into(), price_usd: dec!(3000),
            source: PriceSource::Chainlink, timestamp: Utc::now(), confidence: None,
        };
        assert!(!is_stale(&fresh, 60));

        let mut stale = fresh.clone();
        stale.timestamp = Utc::now() - chrono::Duration::hours(2);
        assert!(is_stale(&stale, 3600));
    }

    #[test]
    fn chainlink_eth_usd_feed() {
        let feeds = chainlink_feeds();
        let eth = feeds.iter().find(|f| f.pair == "ETH/USD").unwrap();
        assert_eq!(eth.decimals, 8);
        assert!(eth.contract_address.starts_with("0x5f4eC3"));
    }
}
