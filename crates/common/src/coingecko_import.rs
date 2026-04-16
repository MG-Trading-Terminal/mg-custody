//! CoinGecko bulk token importer.
//!
//! Fetches top tokens by market cap and resolves their contract addresses
//! across supported chains. Used by POST /v1/admin/tokens/bulk-import.
//!
//! CoinGecko free API: 10-30 req/min. We batch efficiently.

use std::collections::HashMap;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// CoinGecko coin market data response.
#[derive(Debug, Deserialize)]
pub struct CoinGeckoMarketEntry {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub current_price: Option<f64>,
    pub market_cap: Option<f64>,
    pub market_cap_rank: Option<u32>,
    pub image: Option<String>,
    /// Platform contract addresses: {"ethereum": "0x...", "tron": "T...", ...}
    #[serde(default)]
    pub platforms: Option<HashMap<String, Option<String>>>,
}

/// Resolved token for import.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedToken {
    pub symbol: String,
    pub name: String,
    pub chain: String,
    pub contract_address: String,
    pub decimals: u8,
    pub coingecko_id: String,
    pub logo_url: Option<String>,
    pub market_cap_rank: Option<u32>,
    pub is_stablecoin: bool,
}

/// Map CoinGecko platform names to our chain IDs.
fn platform_to_chain(platform: &str) -> Option<&'static str> {
    match platform {
        "ethereum" => Some("ethereum"),
        "binance-smart-chain" | "bsc" => Some("bsc"),
        "polygon-pos" | "polygon" => Some("polygon"),
        "arbitrum-one" | "arbitrum" => Some("arbitrum"),
        "optimistic-ethereum" | "optimism" => Some("optimism"),
        "avalanche" | "avalanche-c-chain" => Some("avalanche"),
        "base" => Some("base"),
        "tron" => Some("tron"),
        "solana" => Some("solana"),
        "fantom" | "ftm" => Some("fantom"),
        "cronos" => Some("cronos"),
        "gnosis" | "xdai" => Some("gnosis"),
        "celo" => Some("celo"),
        "moonbeam" => Some("moonbeam"),
        "zksync" | "zksync-era" => Some("zksync"),
        "linea" => Some("linea"),
        "mantle" => Some("mantle"),
        "blast" => Some("blast"),
        "metis-andromeda" | "metis" => Some("metis"),
        "manta-pacific" | "manta" => Some("manta"),
        "kava" => Some("kava"),
        "scroll" => Some("scroll"),
        _ => None,
    }
}

/// Known stablecoin CoinGecko IDs.
const STABLECOIN_IDS: &[&str] = &[
    "tether", "usd-coin", "dai", "frax", "true-usd", "binance-usd",
    "paypal-usd", "ethena-usde", "gho", "crvusd", "liquity-usd",
    "usdd", "just-stablecoin", "first-digital-usd",
];

/// Default decimals by chain (most ERC-20 are 18, stablecoins are 6).
fn guess_decimals(symbol: &str, chain: &str, is_stablecoin: bool) -> u8 {
    if is_stablecoin {
        // BSC and opBNB stablecoins use 18 decimals
        if chain == "bsc" || chain == "opbnb" { return 18; }
        return 6;
    }
    // Native-wrapped tokens
    match symbol.to_uppercase().as_str() {
        "WBTC" => 8,
        "WETH" | "STETH" | "RETH" | "CBETH" => 18,
        _ => 18, // Most ERC-20 tokens use 18
    }
}

/// Fetch top N tokens from CoinGecko markets API.
///
/// Returns raw market data. Call `resolve_tokens()` to map to our chains.
pub async fn fetch_top_tokens(
    client: &reqwest::Client,
    count: u32,
) -> anyhow::Result<Vec<CoinGeckoMarketEntry>> {
    let mut all = Vec::new();
    let per_page = 250.min(count);
    let pages = (count as f32 / per_page as f32).ceil() as u32;

    for page in 1..=pages {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page={}&page={}&sparkline=false",
            per_page, page
        );

        let resp: Vec<CoinGeckoMarketEntry> = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await?
            .json()
            .await?;

        all.extend(resp);

        // Rate limiting: 1 second between pages
        if page < pages {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    Ok(all)
}

/// Fetch coin details with platform contract addresses.
pub async fn fetch_coin_platforms(
    client: &reqwest::Client,
    coin_id: &str,
) -> anyhow::Result<HashMap<String, String>> {
    let url = format!(
        "https://api.coingecko.com/api/v3/coins/{}?localization=false&tickers=false&market_data=false&community_data=false&developer_data=false&sparkline=false",
        coin_id
    );

    let resp: serde_json::Value = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?
        .json()
        .await?;

    let platforms = resp["platforms"].as_object()
        .map(|p| {
            p.iter()
                .filter_map(|(k, v)| {
                    v.as_str()
                        .filter(|s| !s.is_empty())
                        .map(|s| (k.clone(), s.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(platforms)
}

/// Resolve CoinGecko market entries into importable tokens per chain.
pub fn resolve_tokens(entries: &[CoinGeckoMarketEntry]) -> Vec<ResolvedToken> {
    let mut resolved = Vec::new();

    for entry in entries {
        let is_stablecoin = STABLECOIN_IDS.contains(&entry.id.as_str());

        if let Some(ref platforms) = entry.platforms {
            for (platform, addr_opt) in platforms {
                let Some(chain) = platform_to_chain(platform) else { continue };
                let Some(addr) = addr_opt.as_ref().filter(|a| !a.is_empty()) else { continue };

                let decimals = guess_decimals(&entry.symbol, chain, is_stablecoin);

                resolved.push(ResolvedToken {
                    symbol: entry.symbol.to_uppercase(),
                    name: entry.name.clone(),
                    chain: chain.to_string(),
                    contract_address: addr.clone(),
                    decimals,
                    coingecko_id: entry.id.clone(),
                    logo_url: entry.image.clone(),
                    market_cap_rank: entry.market_cap_rank,
                    is_stablecoin,
                });
            }
        }
    }

    resolved
}

/// Result of a bulk import operation.
#[derive(Debug, Serialize)]
pub struct BulkImportResult {
    pub fetched: usize,
    pub resolved: usize,
    pub imported: usize,
    pub skipped_duplicate: usize,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_mapping() {
        assert_eq!(platform_to_chain("ethereum"), Some("ethereum"));
        assert_eq!(platform_to_chain("binance-smart-chain"), Some("bsc"));
        assert_eq!(platform_to_chain("polygon-pos"), Some("polygon"));
        assert_eq!(platform_to_chain("arbitrum-one"), Some("arbitrum"));
        assert_eq!(platform_to_chain("tron"), Some("tron"));
        assert_eq!(platform_to_chain("solana"), Some("solana"));
        assert_eq!(platform_to_chain("unknown-chain"), None);
    }

    #[test]
    fn stablecoin_decimals() {
        assert_eq!(guess_decimals("USDT", "ethereum", true), 6);
        assert_eq!(guess_decimals("USDT", "bsc", true), 18);
        assert_eq!(guess_decimals("USDC", "polygon", true), 6);
        assert_eq!(guess_decimals("UNI", "ethereum", false), 18);
        assert_eq!(guess_decimals("WBTC", "ethereum", false), 8);
    }

    #[test]
    fn resolve_from_market_entry() {
        let entry = CoinGeckoMarketEntry {
            id: "uniswap".into(),
            symbol: "uni".into(),
            name: "Uniswap".into(),
            current_price: Some(10.0),
            market_cap: Some(6_000_000_000.0),
            market_cap_rank: Some(20),
            image: Some("https://example.com/uni.png".into()),
            platforms: Some(HashMap::from([
                ("ethereum".into(), Some("0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984".into())),
                ("polygon-pos".into(), Some("0xb33EaAd8d922B1083446DC23f610c2567fB5180f".into())),
                ("unknown-chain".into(), Some("0xabc".into())),
            ])),
        };

        let resolved = resolve_tokens(&[entry]);
        assert_eq!(resolved.len(), 2); // ethereum + polygon (unknown-chain filtered out)
        assert!(resolved.iter().any(|t| t.chain == "ethereum"));
        assert!(resolved.iter().any(|t| t.chain == "polygon"));
        assert!(resolved.iter().all(|t| t.symbol == "UNI"));
        assert!(resolved.iter().all(|t| t.decimals == 18));
    }

    #[test]
    fn stablecoin_detection() {
        assert!(STABLECOIN_IDS.contains(&"tether"));
        assert!(STABLECOIN_IDS.contains(&"usd-coin"));
        assert!(STABLECOIN_IDS.contains(&"dai"));
        assert!(!STABLECOIN_IDS.contains(&"bitcoin"));
    }
}
