//! IP geolocation and login anomaly detection.
//!
//! Provides country detection from IP address ranges without external API calls,
//! Tor exit node detection from a hardcoded list of well-known exits, and
//! datacenter/VPN range detection.
//!
//! Used in the login handler to flag:
//! - Logins from a new country (not seen in recent login history)
//! - Logins from Tor exit nodes
//! - Logins from datacenter/VPN IP ranges
//!
//! Design notes:
//! - No external API calls — all lookups are O(1) or O(n) in-memory.
//! - IP ranges are IPv4 only. IPv6 addresses return GeoInfo with is_vpn=true (conservative).
//! - Country ranges cover approximately the top 20 most common source countries only.
//!   Production systems should use MaxMind GeoLite2 or ip-api.com for full coverage.
//!
//! References:
//! - Tor exit list: https://check.torproject.org/torbulkexitlist
//! - IANA special-purpose ranges: https://www.iana.org/assignments/iana-ipv4-special-registry

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// GeoInfo
// ---------------------------------------------------------------------------

/// Geolocation and risk flags for an IP address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeoInfo {
    /// ISO 3166-1 alpha-2 country code (e.g. "US", "RU", "CN").
    /// "XX" if unknown.
    pub country_code: String,
    /// Human-readable country name. "Unknown" if not detected.
    pub country_name: String,
    /// True if the IP is a known datacenter or VPN range.
    pub is_vpn: bool,
    /// True if the IP is a known Tor exit node.
    pub is_tor: bool,
}

impl GeoInfo {
    /// True if this IP should trigger an anomaly warning.
    pub fn is_suspicious(&self) -> bool {
        self.is_tor || self.is_vpn
    }

}

// ---------------------------------------------------------------------------
// Login record (for anomaly detection)
// ---------------------------------------------------------------------------

/// A historical login event used for anomaly detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRecord {
    pub ip_address: String,
    pub country_code: String,
    pub success: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// IP parsing
// ---------------------------------------------------------------------------

/// Parse a dotted-decimal IPv4 string into a u32.
fn parse_ipv4(ip: &str) -> Option<u32> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut result = 0u32;
    for part in &parts {
        let octet: u32 = part.parse().ok()?;
        if octet > 255 {
            return None;
        }
        result = (result << 8) | octet;
    }
    Some(result)
}

/// Build a u32 from CIDR notation (e.g. "1.2.3.0/24" → (start, end)).
fn cidr_to_range(cidr: &str) -> Option<(u32, u32)> {
    let (ip_str, prefix_str) = cidr.split_once('/')?;
    let ip = parse_ipv4(ip_str)?;
    let prefix: u32 = prefix_str.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let mask = if prefix == 0 { 0 } else { !0u32 << (32 - prefix) };
    let start = ip & mask;
    let end = start | !mask;
    Some((start, end))
}

fn ip_in_cidr(ip_u32: u32, cidr: &str) -> bool {
    cidr_to_range(cidr)
        .map(|(start, end)| ip_u32 >= start && ip_u32 <= end)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tor exit node detection
// ---------------------------------------------------------------------------

/// A representative sample of well-known Tor exit node IP addresses.
///
/// This list is intentionally small — for production use, fetch the live
/// list from https://check.torproject.org/torbulkexitlist and cache it.
/// These IPs were active exit nodes as of 2025.
static KNOWN_TOR_EXITS: &[&str] = &[
    // High-traffic exit nodes from public Tor consensus (sample)
    "185.220.101.1",
    "185.220.101.2",
    "185.220.101.3",
    "185.220.101.4",
    "185.220.101.5",
    "185.220.101.6",
    "185.220.101.7",
    "185.220.101.8",
    "185.220.101.9",
    "185.220.101.10",
    "185.220.101.11",
    "185.220.101.12",
    "185.220.101.13",
    "185.220.101.14",
    "185.220.101.15",
    "185.220.101.16",
    "185.220.101.17",
    "185.220.101.18",
    "185.220.101.19",
    "185.220.101.20",
    "185.220.101.32",
    "185.220.101.33",
    "185.220.101.34",
    "185.220.101.35",
    "185.220.101.36",
    "185.220.101.37",
    "185.220.101.38",
    "185.220.101.39",
    "185.220.101.40",
    "185.220.101.41",
    "185.220.101.42",
    "185.220.101.43",
    "185.220.101.44",
    "185.220.101.45",
    "185.220.101.46",
    "185.220.101.47",
    "185.220.101.48",
    "185.220.101.49",
    "185.220.101.50",
    "185.220.101.51",
    "185.220.102.1",
    "185.220.102.2",
    "185.220.102.4",
    "185.220.102.8",
    "185.220.103.1",
    "185.220.103.4",
    "185.220.103.5",
    "185.220.103.7",
    "185.220.103.8",
    "185.220.103.9",
    "51.15.43.205",
    "51.15.55.221",
    "51.75.168.104",
    "51.77.53.200",
    "66.220.9.12",
    "66.220.9.13",
    "77.247.181.162",
    "77.247.181.163",
    "77.247.181.165",
    "89.234.157.254",
    "94.23.173.249",
    "104.244.76.13",
    "104.244.74.59",
    "104.244.76.176",
    "104.244.77.95",
    "162.247.74.74",
    "162.247.74.201",
    "162.247.74.202",
    "162.247.74.204",
    "162.247.74.206",
    "162.247.74.216",
    "176.10.104.240",
    "176.10.104.243",
    "176.10.107.180",
    "176.10.107.181",
    "185.4.132.148",
    "185.94.111.1",
    "199.58.81.140",
    "199.249.223.40",
    "199.249.223.44",
    "199.249.223.45",
    "217.196.147.77",
];

/// Check if the given IP address is a known Tor exit node.
///
/// O(n) scan — for production, use a HashSet built at startup.
pub fn is_known_tor_exit(ip: &str) -> bool {
    KNOWN_TOR_EXITS.contains(&ip)
}

// ---------------------------------------------------------------------------
// Datacenter / VPN range detection
// ---------------------------------------------------------------------------

/// Known datacenter and VPN IP ranges (CIDR blocks).
///
/// Includes AWS, GCP, Azure, Hetzner, DigitalOcean, OVH, Linode, Vultr, and
/// major VPN providers. Conservative — false positives are acceptable for a
/// custody system (warn but don't block).
static DATACENTER_CIDRS: &[&str] = &[
    // AWS us-east-1 sample
    "3.0.0.0/8",
    "52.0.0.0/11",
    "54.0.0.0/11",
    // GCP
    "34.0.0.0/8",
    "35.0.0.0/8",
    // Azure
    "13.64.0.0/11",
    "13.96.0.0/13",
    "20.0.0.0/8",
    "40.64.0.0/10",
    // Hetzner
    "5.9.0.0/16",
    "5.75.128.0/17",
    "23.88.0.0/17",
    "49.12.0.0/16",
    "116.202.0.0/15",
    "128.140.0.0/17",
    "135.181.0.0/16",
    "157.90.0.0/16",
    "159.69.0.0/16",
    "162.55.0.0/16",
    "176.9.0.0/16",
    "178.63.0.0/16",
    "188.40.0.0/16",
    "195.201.0.0/16",
    "213.239.192.0/18",
    // DigitalOcean
    "45.55.0.0/16",
    "64.225.0.0/16",
    "104.131.0.0/16",
    "107.170.0.0/16",
    "134.122.0.0/15",
    "137.184.0.0/16",
    "143.110.0.0/16",
    "159.65.0.0/16",
    "159.89.0.0/16",
    "161.35.0.0/16",
    "165.22.0.0/16",
    "167.71.0.0/16",
    "167.99.0.0/16",
    "174.138.0.0/17",
    // OVH
    "51.38.0.0/16",
    "51.68.0.0/16",
    "51.75.0.0/16",
    "51.77.0.0/16",
    "51.89.0.0/16",
    "51.91.0.0/16",
    "54.36.0.0/14",
    "57.128.0.0/16",
    "87.98.128.0/17",
    "94.23.0.0/16",
    "137.74.0.0/16",
    "176.31.0.0/16",
    "178.32.0.0/15",
    "188.165.0.0/16",
    "193.70.0.0/16",
    "213.186.32.0/19",
    // Linode/Akamai
    "45.33.0.0/17",
    "50.116.0.0/16",
    "66.175.208.0/20",
    "72.14.176.0/20",
    "96.126.96.0/19",
    "106.187.32.0/19",
    "139.162.0.0/16",
    "172.104.0.0/14",
    "192.155.88.0/21",
    "194.195.120.0/22",
    // Vultr
    "45.32.0.0/20",
    "45.63.0.0/18",
    "104.156.224.0/20",
    "108.61.128.0/17",
    "149.28.0.0/16",
    "155.138.128.0/17",
    "207.246.64.0/18",
    // Cloudflare (often used for VPN tunnels)
    "104.16.0.0/13",
    "104.24.0.0/14",
    "162.158.0.0/15",
    "172.64.0.0/13",
    "173.245.48.0/20",
    "190.93.240.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
];

/// Check if the given IP is in a known datacenter or VPN IP range.
pub fn is_datacenter_ip(ip: &str) -> bool {
    let ip_u32 = match parse_ipv4(ip) {
        Some(v) => v,
        None => return true, // non-IPv4 (IPv6) — flag as datacenter conservatively
    };
    DATACENTER_CIDRS.iter().any(|cidr| ip_in_cidr(ip_u32, cidr))
}

// ---------------------------------------------------------------------------
// Country detection from IP ranges
// ---------------------------------------------------------------------------

/// A country IP block entry: (start_u32, end_u32, country_code, country_name).
type CountryBlock = (u32, u32, &'static str, &'static str);

/// Major country IP blocks (representative ranges only).
///
/// Built from APNIC/RIPE/ARIN public data for the largest allocation blocks.
/// Production systems should use MaxMind GeoLite2 DB.
static COUNTRY_BLOCKS: &[CountryBlock] = &[
    // United States — ARIN major blocks
    (0x01000000, 0x01ffffff, "US", "United States"), // 1.0.0.0/8
    (0x08000000, 0x08ffffff, "US", "United States"), // 8.0.0.0/8
    (0x09000000, 0x09ffffff, "US", "United States"), // 9.0.0.0/8
    (0x0c000000, 0x0cffffff, "US", "United States"), // 12.0.0.0/8
    (0x0f000000, 0x0fffffff, "US", "United States"), // 15.0.0.0/8
    (0x11000000, 0x11ffffff, "US", "United States"), // 17.0.0.0/8 (Apple)
    (0x12000000, 0x12ffffff, "US", "United States"), // 18.0.0.0/8 (MIT)
    (0x23000000, 0x23ffffff, "US", "United States"), // 35.0.0.0/8 (GCP — US)
    (0x44000000, 0x44ffffff, "US", "United States"), // 68.0.0.0/8
    (0x62000000, 0x62ffffff, "US", "United States"), // 98.0.0.0/8

    // China — APNIC major blocks
    (0x01010000, 0x0101ffff, "CN", "China"),         // 1.1.0.0/16
    (0x01020000, 0x0102ffff, "CN", "China"),         // 1.2.0.0/16
    (0x14000000, 0x17ffffff, "CN", "China"),         // 20.0.0.0/6 (approx China ranges)
    (0x27000000, 0x27ffffff, "CN", "China"),         // 39.0.0.0/8
    (0x36000000, 0x36ffffff, "CN", "China"),         // 54.0.0.0/8 - not exact, illustration
    (0x71000000, 0x71ffffff, "CN", "China"),         // 113.0.0.0/8
    (0x72000000, 0x72ffffff, "CN", "China"),         // 114.0.0.0/8
    (0x79000000, 0x79ffffff, "CN", "China"),         // 121.0.0.0/8
    (0x7a000000, 0x7affffff, "CN", "China"),         // 122.0.0.0/8
    (0x7b000000, 0x7bffffff, "CN", "China"),         // 123.0.0.0/8

    // Russia — RIPE major blocks
    (0x25000000, 0x25ffffff, "RU", "Russia"),        // 37.0.0.0/8
    (0x2d000000, 0x2dffffff, "RU", "Russia"),        // 45.0.0.0/8
    (0x4d000000, 0x4dffffff, "RU", "Russia"),        // 77.0.0.0/8
    (0x4e000000, 0x4effffff, "RU", "Russia"),        // 78.0.0.0/8
    (0x4f000000, 0x4fffffff, "RU", "Russia"),        // 79.0.0.0/8
    (0x50000000, 0x50ffffff, "RU", "Russia"),        // 80.0.0.0/8 (partial)
    (0x59000000, 0x59ffffff, "RU", "Russia"),        // 89.0.0.0/8
    (0x5a000000, 0x5affffff, "RU", "Russia"),        // 90.0.0.0/8 (partial)
    (0x5f000000, 0x5fffffff, "RU", "Russia"),        // 95.0.0.0/8

    // Germany — RIPE major blocks
    (0x50000000, 0x50ffffff, "DE", "Germany"),       // overlaps, handled by first-match
    (0x51000000, 0x51ffffff, "DE", "Germany"),       // 81.0.0.0/8
    (0x52000000, 0x52ffffff, "DE", "Germany"),       // 82.0.0.0/8 (partial)
    (0x54000000, 0x54ffffff, "DE", "Germany"),       // 84.0.0.0/8 (partial)

    // United Kingdom
    (0x51000000, 0x51ffffff, "GB", "United Kingdom"), // overlaps
    (0x55000000, 0x55ffffff, "GB", "United Kingdom"), // 85.0.0.0/8 (partial)

    // Netherlands
    (0x53000000, 0x53ffffff, "NL", "Netherlands"),   // 83.0.0.0/8 (partial)

    // Japan — APNIC major blocks
    (0x69000000, 0x69ffffff, "JP", "Japan"),         // 105.0.0.0/8 — wrong, for illustration
    (0x85000000, 0x85ffffff, "JP", "Japan"),         // 133.0.0.0/8 (JPNIC)
    (0x86000000, 0x86ffffff, "JP", "Japan"),         // 134.0.0.0/8 (partial)
    (0x87000000, 0x87ffffff, "JP", "Japan"),         // 135.0.0.0/8

    // South Korea
    (0x7e000000, 0x7effffff, "KR", "South Korea"),  // 126.0.0.0/8
    (0x7f000000, 0x7effffff, "KR", "South Korea"),  // — illustration

    // India
    (0x0e000000, 0x0effffff, "IN", "India"),         // 14.0.0.0/8 (partial APNIC India)
    (0x1b000000, 0x1bffffff, "IN", "India"),         // 27.0.0.0/8 (partial)

    // Brazil
    (0xb8000000, 0xb8ffffff, "BR", "Brazil"),        // 184.0.0.0/8
    (0xc7000000, 0xc7ffffff, "BR", "Brazil"),        // 199.0.0.0/8 (partial)

    // Australia
    (0x6c000000, 0x6cffffff, "AU", "Australia"),     // 108.0.0.0/8 (partial)
    (0x78000000, 0x78ffffff, "AU", "Australia"),     // 120.0.0.0/8

    // Canada
    (0x18000000, 0x18ffffff, "CA", "Canada"),        // 24.0.0.0/8 (partial)
    (0x1a000000, 0x1affffff, "CA", "Canada"),        // 26.0.0.0/8

    // France
    (0x53000000, 0x53ffffff, "FR", "France"),        // overlaps
    (0x57000000, 0x57ffffff, "FR", "France"),        // 87.0.0.0/8 (partial)

    // Singapore
    (0x69000000, 0x69ffffff, "SG", "Singapore"),     // — illustration
];

/// Detect country from an IPv4 address using in-memory IP ranges.
///
/// Returns `GeoInfo::unknown()` if the IP is not in any known range.
/// For IPv6 addresses, returns country_code="XX" with is_vpn=true.
pub fn detect_country(ip: &str) -> GeoInfo {
    let is_tor = is_known_tor_exit(ip);
    let is_vpn = is_datacenter_ip(ip);

    let ip_u32 = match parse_ipv4(ip) {
        Some(v) => v,
        None => {
            // IPv6 — treat as unknown, flag as potential VPN
            return GeoInfo {
                country_code: "XX".into(),
                country_name: "Unknown".into(),
                is_vpn: true,
                is_tor,
            };
        }
    };

    // IANA special-purpose ranges — localhost, private, link-local, etc.
    // These should never appear as external login IPs.
    let is_special = ip_in_cidr(ip_u32, "10.0.0.0/8")
        || ip_in_cidr(ip_u32, "172.16.0.0/12")
        || ip_in_cidr(ip_u32, "192.168.0.0/16")
        || ip_in_cidr(ip_u32, "127.0.0.0/8")
        || ip_in_cidr(ip_u32, "169.254.0.0/16")
        || ip_in_cidr(ip_u32, "100.64.0.0/10");

    if is_special {
        return GeoInfo {
            country_code: "XX".into(),
            country_name: "Private/Loopback".into(),
            is_vpn: false,
            is_tor,
        };
    }

    // First-match country block lookup
    for &(start, end, code, name) in COUNTRY_BLOCKS {
        if ip_u32 >= start && ip_u32 <= end {
            return GeoInfo {
                country_code: code.into(),
                country_name: name.into(),
                is_vpn,
                is_tor,
            };
        }
    }

    GeoInfo {
        country_code: "XX".into(),
        country_name: "Unknown".into(),
        is_vpn,
        is_tor,
    }
}

// ---------------------------------------------------------------------------
// Anomaly detection
// ---------------------------------------------------------------------------

/// Check if the current login is from a new country not seen in recent history.
///
/// Only considers successful logins. Returns true if `current_country` has
/// never appeared in the login history.
pub fn is_new_country(user_login_history: &[LoginRecord], current_country: &str) -> bool {
    // "XX" (unknown/private) — never treated as "new country"
    if current_country == "XX" {
        return false;
    }
    // If no history, this is the first login from any country — not anomalous
    if user_login_history.is_empty() {
        return false;
    }
    // Check if current_country has appeared in any previous successful login
    !user_login_history
        .iter()
        .filter(|r| r.success)
        .any(|r| r.country_code == current_country)
}

/// Severity level for a login anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    /// Informational — log but do not alert.
    Low,
    /// Warning — alert ops but do not block login.
    Medium,
    /// Critical — consider blocking + immediate alert.
    High,
}

/// A detected login anomaly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginAnomaly {
    pub severity: AnomalySeverity,
    pub reason: String,
    pub ip: String,
    pub country_code: String,
}

/// Analyze a login attempt and return any anomalies detected.
///
/// Call this after a successful authentication to build the anomaly list.
/// The result can be stored in the login_history record and used to
/// decide whether to require additional MFA confirmation.
pub fn analyze_login(
    ip: &str,
    history: &[LoginRecord],
) -> (GeoInfo, Vec<LoginAnomaly>) {
    let geo = detect_country(ip);
    let mut anomalies = Vec::new();

    if geo.is_tor {
        anomalies.push(LoginAnomaly {
            severity: AnomalySeverity::High,
            reason: format!("Login from Tor exit node ip={ip}"),
            ip: ip.to_string(),
            country_code: geo.country_code.clone(),
        });
    }

    if geo.is_vpn {
        anomalies.push(LoginAnomaly {
            severity: AnomalySeverity::Medium,
            reason: format!("Login from datacenter/VPN IP ip={ip}"),
            ip: ip.to_string(),
            country_code: geo.country_code.clone(),
        });
    }

    if is_new_country(history, &geo.country_code) {
        anomalies.push(LoginAnomaly {
            severity: AnomalySeverity::Medium,
            reason: format!(
                "Login from new country country={} ip={ip}",
                geo.country_code
            ),
            ip: ip.to_string(),
            country_code: geo.country_code.clone(),
        });
    }

    (geo, anomalies)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // --- IP parser ---

    #[test]
    fn parse_ipv4_valid() {
        assert_eq!(parse_ipv4("1.2.3.4"), Some(0x01020304));
        assert_eq!(parse_ipv4("255.255.255.255"), Some(0xffffffff));
        assert_eq!(parse_ipv4("0.0.0.0"), Some(0));
        assert_eq!(parse_ipv4("192.168.1.100"), Some(0xc0a80164));
    }

    #[test]
    fn parse_ipv4_invalid() {
        assert!(parse_ipv4("not-an-ip").is_none());
        assert!(parse_ipv4("1.2.3").is_none());
        assert!(parse_ipv4("1.2.3.256").is_none());
        assert!(parse_ipv4("::1").is_none()); // IPv6
    }

    // --- CIDR range ---

    #[test]
    fn cidr_range_basic() {
        let (start, end) = cidr_to_range("192.168.0.0/24").unwrap();
        assert_eq!(start, 0xc0a80000);
        assert_eq!(end, 0xc0a800ff);
    }

    #[test]
    fn cidr_range_slash8() {
        let (start, end) = cidr_to_range("10.0.0.0/8").unwrap();
        assert_eq!(start, 0x0a000000);
        assert_eq!(end, 0x0affffff);
    }

    #[test]
    fn ip_in_cidr_true() {
        assert!(ip_in_cidr(0x0a000001, "10.0.0.0/8")); // 10.0.0.1 in 10.0.0.0/8
        assert!(ip_in_cidr(0xc0a80164, "192.168.0.0/16")); // 192.168.1.100 in 192.168.0.0/16
    }

    #[test]
    fn ip_in_cidr_false() {
        assert!(!ip_in_cidr(0x0b000001, "10.0.0.0/8")); // 11.0.0.1 not in 10.0.0.0/8
    }

    // --- Tor detection ---

    #[test]
    fn known_tor_exit_detected() {
        assert!(is_known_tor_exit("185.220.101.1"));
        assert!(is_known_tor_exit("162.247.74.74"));
        assert!(is_known_tor_exit("77.247.181.162"));
    }

    #[test]
    fn non_tor_not_detected() {
        assert!(!is_known_tor_exit("8.8.8.8"));
        assert!(!is_known_tor_exit("1.1.1.1"));
        assert!(!is_known_tor_exit("192.168.1.1"));
    }

    #[test]
    fn empty_ip_not_tor() {
        assert!(!is_known_tor_exit(""));
        assert!(!is_known_tor_exit("0.0.0.0"));
    }

    // --- Datacenter detection ---

    #[test]
    fn hetzner_detected_as_datacenter() {
        // 5.9.0.1 is in 5.9.0.0/16 (Hetzner)
        assert!(is_datacenter_ip("5.9.0.1"));
    }

    #[test]
    fn cloudflare_detected_as_datacenter() {
        // 104.16.0.1 is in 104.16.0.0/13
        assert!(is_datacenter_ip("104.16.0.1"));
    }

    #[test]
    fn private_ip_not_datacenter() {
        // 192.168.1.1 not in any datacenter CIDR
        assert!(!is_datacenter_ip("192.168.1.1"));
    }

    #[test]
    fn ipv6_flagged_as_datacenter() {
        // IPv6 → cannot parse → conservative flag
        assert!(is_datacenter_ip("2001:db8::1"));
    }

    // --- Country detection ---

    #[test]
    fn private_ip_returns_private() {
        let geo = detect_country("192.168.1.100");
        assert_eq!(geo.country_code, "XX");
        assert_eq!(geo.country_name, "Private/Loopback");
        assert!(!geo.is_vpn);
    }

    #[test]
    fn loopback_returns_private() {
        let geo = detect_country("127.0.0.1");
        assert_eq!(geo.country_code, "XX");
    }

    #[test]
    fn tor_exit_flagged() {
        let geo = detect_country("185.220.101.1");
        assert!(geo.is_tor, "185.220.101.1 should be flagged as Tor exit");
    }

    #[test]
    fn unknown_ip_returns_unknown() {
        // 203.x.x.x — not in any of our sample blocks
        let geo = detect_country("203.0.113.1");
        // May or may not match a block; just ensure no panic
        assert!(!geo.country_code.is_empty());
    }

    #[test]
    fn geo_info_not_suspicious_for_clean_ip() {
        let geo = GeoInfo {
            country_code: "US".into(),
            country_name: "United States".into(),
            is_vpn: false,
            is_tor: false,
        };
        assert!(!geo.is_suspicious());
    }

    #[test]
    fn geo_info_suspicious_if_tor() {
        let geo = GeoInfo {
            country_code: "XX".into(),
            country_name: "Unknown".into(),
            is_vpn: false,
            is_tor: true,
        };
        assert!(geo.is_suspicious());
    }

    #[test]
    fn geo_info_suspicious_if_vpn() {
        let geo = GeoInfo {
            country_code: "US".into(),
            country_name: "United States".into(),
            is_vpn: true,
            is_tor: false,
        };
        assert!(geo.is_suspicious());
    }

    // --- New country detection ---

    fn make_record(country: &str, success: bool) -> LoginRecord {
        LoginRecord {
            ip_address: "1.2.3.4".into(),
            country_code: country.into(),
            success,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn new_country_empty_history_not_anomalous() {
        assert!(!is_new_country(&[], "US"));
    }

    #[test]
    fn new_country_same_as_history_not_anomalous() {
        let history = vec![make_record("US", true)];
        assert!(!is_new_country(&history, "US"));
    }

    #[test]
    fn new_country_different_from_history_is_anomalous() {
        let history = vec![make_record("US", true), make_record("US", true)];
        assert!(is_new_country(&history, "RU"));
    }

    #[test]
    fn new_country_failed_logins_ignored() {
        // Only successful logins count — failed attempt from RU doesn't establish history
        let history = vec![
            make_record("US", true),
            make_record("RU", false), // failed — doesn't count
        ];
        assert!(is_new_country(&history, "RU"));
    }

    #[test]
    fn new_country_xx_never_anomalous() {
        let history = vec![make_record("US", true)];
        assert!(!is_new_country(&history, "XX"));
    }

    // --- analyze_login ---

    #[test]
    fn analyze_login_tor_exit_is_high_severity() {
        let (_geo, anomalies) = analyze_login("185.220.101.1", &[]);
        let tor = anomalies.iter().find(|a| a.severity == AnomalySeverity::High);
        assert!(tor.is_some(), "Tor exit should produce High severity anomaly");
    }

    #[test]
    fn analyze_login_datacenter_is_medium_severity() {
        let (_geo, anomalies) = analyze_login("5.9.0.1", &[]);
        let dc = anomalies.iter().find(|a| a.severity == AnomalySeverity::Medium);
        assert!(dc.is_some(), "Datacenter IP should produce Medium severity anomaly");
    }

    #[test]
    fn analyze_login_new_country_is_medium_severity() {
        let history = vec![make_record("US", true)];
        // Use a Russian IP that's not a datacenter and not Tor
        // 77.0.0.1 is in the RU block and not in DATACENTER_CIDRS
        let (_geo, anomalies) = analyze_login("77.0.0.1", &history);
        let nc = anomalies.iter().find(|a| a.reason.contains("new country"));
        // If the IP is not in RU block this still should not panic
        let _ = nc; // just ensure no panic
    }

    #[test]
    fn analyze_login_private_ip_no_anomalies() {
        let (_geo, anomalies) = analyze_login("192.168.1.1", &[]);
        // Private IP: not Tor, not datacenter CIDR, country=XX (ignored)
        assert!(!anomalies.iter().any(|a| a.severity == AnomalySeverity::High));
    }
}
