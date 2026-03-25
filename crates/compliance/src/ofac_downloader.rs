//! OFAC SDN list auto-downloader.
//!
//! Downloads the OFAC SDN list from the US Treasury website,
//! parses crypto addresses, and updates the sanctions checker.
//!
//! Run daily via background scheduler.

use anyhow::{Context, Result};

/// OFAC SDN list download URL (CSV format with digital currency addresses).
pub const OFAC_SDN_CSV_URL: &str = "https://www.treasury.gov/ofac/downloads/sdn.csv";

/// OFAC consolidated non-SDN list (additional sanctioned entities).
pub const OFAC_CONS_URL: &str = "https://www.treasury.gov/ofac/downloads/consolidated/cons_prim.csv";

/// Download and parse crypto addresses from the OFAC SDN list.
pub async fn download_ofac_addresses(client: &reqwest::Client) -> Result<Vec<String>> {
    let resp = client
        .get(OFAC_SDN_CSV_URL)
        .send()
        .await
        .context("Failed to download OFAC SDN list")?
        .text()
        .await
        .context("Failed to read OFAC response")?;

    let addresses = parse_crypto_addresses(&resp);
    tracing::info!(count = addresses.len(), "OFAC crypto addresses parsed");
    Ok(addresses)
}

/// Parse cryptocurrency addresses from OFAC CSV data.
///
/// OFAC lists crypto addresses in the format:
/// "Digital Currency Address - XBT abc123..."
/// "Digital Currency Address - ETH 0xabc..."
fn parse_crypto_addresses(csv_data: &str) -> Vec<String> {
    let mut addresses = Vec::new();

    for line in csv_data.lines() {
        // Look for "Digital Currency Address" entries
        if line.contains("Digital Currency Address") {
            // Extract the address after the dash
            // Format: "Digital Currency Address - XBT <address>"
            // or: "Digital Currency Address - ETH <address>"
            for part in line.split(';') {
                let trimmed = part.trim();
                if trimmed.contains("Digital Currency Address") {
                    // Try to extract address
                    if let Some(addr_part) = trimmed.split(" - ").nth(1) {
                        // Skip the currency code (XBT, ETH, etc.)
                        let tokens: Vec<&str> = addr_part.split_whitespace().collect();
                        if tokens.len() >= 2 {
                            let addr = tokens[1].trim_end_matches(';').trim_end_matches(',');
                            if !addr.is_empty() {
                                addresses.push(addr.to_lowercase());
                            }
                        }
                    }
                }
            }
        }

        // Also catch standalone crypto addresses (0x... or bc1...)
        for word in line.split([',', ';', '"', ' ']) {
            let w = word.trim();
            if (w.starts_with("0x") && w.len() == 42) ||
               (w.starts_with("bc1") && w.len() > 20) ||
               (w.starts_with("T") && w.len() == 34) {
                addresses.push(w.to_lowercase());
            }
        }
    }

    addresses.sort();
    addresses.dedup();
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_eth_address_from_ofac() {
        let csv = r#"
12345,"SOME ENTITY","Digital Currency Address - ETH 0xd90e2f925DA726b50C4Ed8D0Fb90Ad053324f31b;"
"#;
        let addresses = parse_crypto_addresses(csv);
        assert!(addresses.contains(&"0xd90e2f925da726b50c4ed8d0fb90ad053324f31b".to_string()));
    }

    #[test]
    fn parse_btc_address_from_ofac() {
        let csv = r#"
99999,"ENTITY","Digital Currency Address - XBT bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh;"
"#;
        let addresses = parse_crypto_addresses(csv);
        assert!(addresses.iter().any(|a| a.starts_with("bc1q")));
    }

    #[test]
    fn parse_standalone_eth_address() {
        let csv = r#"something,0xdAC17F958D2ee523a2206206994597C13D831ec7,something"#;
        let addresses = parse_crypto_addresses(csv);
        assert!(addresses.contains(&"0xdac17f958d2ee523a2206206994597c13d831ec7".to_string()));
    }

    #[test]
    fn empty_csv_returns_empty() {
        assert!(parse_crypto_addresses("").is_empty());
        assert!(parse_crypto_addresses("no crypto here").is_empty());
    }

    #[test]
    fn dedup_addresses() {
        let csv = "0xABC1234567890123456789012345678901234567,0xabc1234567890123456789012345678901234567";
        let addresses = parse_crypto_addresses(csv);
        assert_eq!(addresses.len(), 1); // deduped
    }
}
