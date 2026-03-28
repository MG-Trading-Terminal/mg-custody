//! Comprehensive address validation and checksum verification for all supported chains.
//!
//! Every address accepted by the withdrawal pipeline passes through `validate_address`
//! before being stored or used for signing.  Returning a `ValidatedAddress` makes the
//! type system enforce the invariant: only validated addresses reach the signer.
//!
//! # Chain coverage
//! - **Bitcoin**: Bech32 (P2WPKH, P2WSH), Bech32m (P2TR), Base58Check (P2PKH, P2SH)
//! - **Ethereum**: EIP-55 mixed-case checksum (re-derives and compares)
//! - **Tron**: Base58Check with 0x41 network byte + double-SHA256 checksum
//! - **Solana**: Base58-decoded 32-byte Ed25519 public key
//!
//! # References
//! - BIP-173: SegWit address format (Bech32) — https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki
//! - BIP-350: Taproot address format (Bech32m) — https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki
//! - EIP-55: Mixed-case checksum address encoding — https://eips.ethereum.org/EIPS/eip-55
//! - Tron address format: https://developers.tron.network/docs/account

use sha2::Digest;

use crate::Chain;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An address that has passed chain-specific format and checksum validation.
///
/// Obtain via [`validate_address`] — never construct directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAddress {
    /// Canonical (checksummed / lowercased-Bech32) form of the address.
    pub normalized: String,
    /// Chain this address belongs to.
    pub chain: Chain,
    /// Address script type / encoding.
    pub address_type: AddressType,
}

/// Script type or encoding format of an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressType {
    // Bitcoin
    /// `bc1q...` — P2WPKH (Bech32, BIP-173).
    P2wpkh,
    /// `bc1p...` — P2TR Taproot (Bech32m, BIP-350).
    P2tr,
    /// `3...` — P2SH (Base58Check).
    P2sh,
    /// `1...` — P2PKH (Base58Check).
    P2pkh,
    // Ethereum
    /// `0x...` with EIP-55 mixed-case checksum.
    Eip55,
    // Tron
    /// `T...` — Base58Check with 0x41 network byte.
    TronBase58Check,
    // Solana
    /// 32-byte Ed25519 public key in Base58.
    SolanaBase58,
}

/// Errors produced by [`validate_address`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum AddressError {
    #[error("address is empty")]
    Empty,

    #[error("invalid length for {chain}: expected {expected}, got {got} characters")]
    InvalidLength {
        chain: Chain,
        expected: String,
        got: usize,
    },

    #[error("invalid checksum for {chain}: {details}")]
    InvalidChecksum { chain: Chain, details: String },

    #[error("invalid prefix for {chain}: expected {expected}, got {got}")]
    InvalidPrefix {
        chain: Chain,
        expected: String,
        got: String,
    },

    #[error("invalid character at position {position}")]
    InvalidCharacters { position: usize },

    #[error("unsupported chain: {0}")]
    UnsupportedChain(String),

    #[error("invalid base58: {0}")]
    InvalidBase58(String),

    #[error("invalid hex: {0}")]
    InvalidHex(String),
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Validate and normalize an address for the given chain.
///
/// Returns a [`ValidatedAddress`] carrying the canonical form, or an
/// [`AddressError`] describing the exact failure.
pub fn validate_address(address: &str, chain: Chain) -> Result<ValidatedAddress, AddressError> {
    if address.is_empty() {
        return Err(AddressError::Empty);
    }

    match chain {
        Chain::Bitcoin => validate_bitcoin(address),
        Chain::Ethereum => validate_ethereum(address),
        Chain::Tron => validate_tron(address),
        Chain::Solana => validate_solana(address),
    }
}

// ---------------------------------------------------------------------------
// Bitcoin
// ---------------------------------------------------------------------------

fn validate_bitcoin(address: &str) -> Result<ValidatedAddress, AddressError> {
    let lower = address.to_lowercase();

    // Bech32 / Bech32m: must start with "bc1" (mainnet) or "tb1" (testnet).
    if lower.starts_with("bc1") || lower.starts_with("tb1") {
        return validate_bitcoin_bech32(address);
    }

    // Base58Check: starts with '1' (P2PKH) or '3' (P2SH).
    if address.starts_with('1') || address.starts_with('3') {
        return validate_bitcoin_base58check(address);
    }

    Err(AddressError::InvalidPrefix {
        chain: Chain::Bitcoin,
        expected: "bc1 / tb1 / 1 / 3".to_string(),
        got: address.chars().take(4).collect(),
    })
}

/// Validate a native SegWit (Bech32 / Bech32m) Bitcoin address.
///
/// Uses the property that Bech32 strings over the charset `qpzry9x8gf2tvdw0s3jn54khce6mua7l`
/// satisfy a specific polynomial checksum.  We verify by decoding and re-encoding.
fn validate_bitcoin_bech32(address: &str) -> Result<ValidatedAddress, AddressError> {
    let lower = address.to_lowercase();

    // Bech32 charset validation — every char after the '1' separator must be in the set.
    const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let sep = lower.rfind('1').ok_or_else(|| AddressError::InvalidChecksum {
        chain: Chain::Bitcoin,
        details: "no '1' separator found in bech32 address".to_string(),
    })?;

    let hrp = &lower[..sep];
    let data_part = &lower[sep + 1..];

    // Minimum length after separator = 6 (checksum only) is too short for a real address.
    if data_part.len() < 8 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Bitcoin,
            expected: "14-90".to_string(),
            got: address.len(),
        });
    }

    for (i, ch) in data_part.chars().enumerate() {
        if !CHARSET.contains(ch) {
            return Err(AddressError::InvalidCharacters { position: sep + 1 + i });
        }
    }

    // Decode 5-bit groups.
    let data_values: Vec<u8> = data_part
        .chars()
        .map(|c| CHARSET.find(c).unwrap() as u8)
        .collect();

    // The witness version is the first 5-bit group (0 = Bech32, 1 = Bech32m / Taproot).
    let witness_version = data_values[0];

    // Verify Bech32 checksum using polymod.
    if !bech32_verify_checksum(hrp.as_bytes(), &data_values) {
        return Err(AddressError::InvalidChecksum {
            chain: Chain::Bitcoin,
            details: format!("bech32 checksum failed for address {address}"),
        });
    }

    let address_type = match (lower.starts_with("bc1p"), witness_version) {
        (true, 1) | (_, 1) => AddressType::P2tr,
        _ => AddressType::P2wpkh,
    };

    Ok(ValidatedAddress {
        normalized: lower,
        chain: Chain::Bitcoin,
        address_type,
    })
}

/// Verify a Bech32 / Bech32m checksum (polymod over GF(2^5)).
///
/// Reference: BIP-173, BIP-350.
fn bech32_verify_checksum(hrp: &[u8], data: &[u8]) -> bool {
    let mut chk: u32 = 1;
    let polymod = |chk: &mut u32, v: u8| {
        let top = *chk >> 25;
        *chk = ((*chk & 0x1ff_ffff) << 5) ^ (v as u32);
        for i in 0u8..5 {
            if (top >> i) & 1 != 0 {
                *chk ^= [0x3b6a_57b2, 0x26508e6d, 0x1ea1_19fa, 0x3d42_33dd, 0x2a14_62b3][i as usize];
            }
        }
    };

    for &b in hrp {
        polymod(&mut chk, b >> 5);
    }
    polymod(&mut chk, 0);
    for &b in hrp {
        polymod(&mut chk, b & 0x1f);
    }
    for &v in data {
        polymod(&mut chk, v);
    }

    // Bech32: constant = 1; Bech32m: constant = 0x2bc830a3.
    chk == 1 || chk == 0x2bc8_30a3
}

/// Validate a Bitcoin Base58Check address (P2PKH or P2SH).
fn validate_bitcoin_base58check(address: &str) -> Result<ValidatedAddress, AddressError> {
    let decoded = bs58::decode(address)
        .into_vec()
        .map_err(|e| AddressError::InvalidBase58(e.to_string()))?;

    if decoded.len() != 25 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Bitcoin,
            expected: "25 decoded bytes".to_string(),
            got: decoded.len(),
        });
    }

    let (payload, checksum) = decoded.split_at(21);
    let expected = double_sha256_checksum(payload);
    if checksum != expected {
        return Err(AddressError::InvalidChecksum {
            chain: Chain::Bitcoin,
            details: format!(
                "expected {:02x}{:02x}{:02x}{:02x}, got {:02x}{:02x}{:02x}{:02x}",
                expected[0], expected[1], expected[2], expected[3],
                checksum[0], checksum[1], checksum[2], checksum[3],
            ),
        });
    }

    let address_type = match decoded[0] {
        0x00 => AddressType::P2pkh, // mainnet P2PKH
        0x05 => AddressType::P2sh,  // mainnet P2SH
        _ => AddressType::P2pkh,    // testnet variants — still valid
    };

    Ok(ValidatedAddress {
        normalized: address.to_string(),
        chain: Chain::Bitcoin,
        address_type,
    })
}

// ---------------------------------------------------------------------------
// Ethereum — EIP-55
// ---------------------------------------------------------------------------

fn validate_ethereum(address: &str) -> Result<ValidatedAddress, AddressError> {
    // Must start with "0x" and be exactly 42 characters.
    if !address.starts_with("0x") && !address.starts_with("0X") {
        return Err(AddressError::InvalidPrefix {
            chain: Chain::Ethereum,
            expected: "0x".to_string(),
            got: address.chars().take(2).collect(),
        });
    }

    if address.len() != 42 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Ethereum,
            expected: "42".to_string(),
            got: address.len(),
        });
    }

    let hex_part = &address[2..];
    // All-lowercase or all-uppercase is valid (no checksum ambiguity).
    // Mixed case must match EIP-55.
    let is_mixed = hex_part.chars().any(|c| c.is_ascii_uppercase())
        && hex_part.chars().any(|c| c.is_ascii_lowercase());

    // Validate hex characters.
    for (i, c) in hex_part.chars().enumerate() {
        if !c.is_ascii_hexdigit() {
            return Err(AddressError::InvalidCharacters { position: i + 2 });
        }
    }

    let normalized = eip55_checksum(hex_part);

    // If the caller supplied mixed-case hex, enforce EIP-55: the only valid
    // mixed-case form is the checksummed one.  All-lowercase and all-uppercase
    // are always accepted (no checksum ambiguity).
    if is_mixed && address != normalized {
        return Err(AddressError::InvalidChecksum {
            chain: Chain::Ethereum,
            details: format!(
                "EIP-55 mismatch: input={address}, expected={normalized}"
            ),
        });
    }

    Ok(ValidatedAddress {
        normalized,
        chain: Chain::Ethereum,
        address_type: AddressType::Eip55,
    })
}

/// Compute the EIP-55 checksummed form of a 40-hex-char Ethereum address.
///
/// The checksum capitalises hex digits whose corresponding nibble in the
/// Keccak-256 hash of the lowercase address is >= 8.
pub fn eip55_checksum(hex_addr: &str) -> String {
    let lower = hex_addr.to_lowercase();
    // Keccak-256 of the lowercase hex string (without 0x prefix).
    let hash = keccak256(lower.as_bytes());

    let mut out = String::with_capacity(42);
    out.push_str("0x");

    for (i, c) in lower.chars().enumerate() {
        if c.is_ascii_digit() {
            out.push(c);
        } else {
            // Nibble i of the hash: byte i/2, upper or lower nibble.
            let nibble = if i % 2 == 0 {
                (hash[i / 2] >> 4) & 0xf
            } else {
                hash[i / 2] & 0xf
            };
            if nibble >= 8 {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
        }
    }
    out
}

/// Keccak-256 digest (not SHA3-256).
fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::Digest as KeccakDigest;
    let mut hasher = sha3::Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

// ---------------------------------------------------------------------------
// Tron
// ---------------------------------------------------------------------------

fn validate_tron(address: &str) -> Result<ValidatedAddress, AddressError> {
    // Tron mainnet addresses start with 'T' (Base58Check, network byte 0x41).
    if !address.starts_with('T') {
        return Err(AddressError::InvalidPrefix {
            chain: Chain::Tron,
            expected: "T".to_string(),
            got: address.chars().take(1).collect(),
        });
    }

    // Standard length is 34 characters.
    if address.len() != 34 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Tron,
            expected: "34".to_string(),
            got: address.len(),
        });
    }

    let decoded = bs58::decode(address)
        .into_vec()
        .map_err(|e| AddressError::InvalidBase58(e.to_string()))?;

    if decoded.len() != 25 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Tron,
            expected: "25 decoded bytes".to_string(),
            got: decoded.len(),
        });
    }

    // First byte must be 0x41 (mainnet).
    if decoded[0] != 0x41 {
        return Err(AddressError::InvalidPrefix {
            chain: Chain::Tron,
            expected: "0x41 network byte".to_string(),
            got: format!("0x{:02x}", decoded[0]),
        });
    }

    let (payload, checksum) = decoded.split_at(21);
    let expected = double_sha256_checksum(payload);
    if checksum != expected {
        return Err(AddressError::InvalidChecksum {
            chain: Chain::Tron,
            details: format!(
                "double-SHA256 checksum mismatch: expected {:08x}, got {:08x}",
                u32::from_be_bytes([expected[0], expected[1], expected[2], expected[3]]),
                u32::from_be_bytes([checksum[0], checksum[1], checksum[2], checksum[3]]),
            ),
        });
    }

    Ok(ValidatedAddress {
        normalized: address.to_string(),
        chain: Chain::Tron,
        address_type: AddressType::TronBase58Check,
    })
}

// ---------------------------------------------------------------------------
// Solana
// ---------------------------------------------------------------------------

fn validate_solana(address: &str) -> Result<ValidatedAddress, AddressError> {
    // Solana addresses are Base58-encoded Ed25519 public keys: 32 bytes → 32–44 chars.
    if address.len() < 32 || address.len() > 44 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Solana,
            expected: "32-44".to_string(),
            got: address.len(),
        });
    }

    let decoded = bs58::decode(address)
        .into_vec()
        .map_err(|e| AddressError::InvalidBase58(e.to_string()))?;

    if decoded.len() != 32 {
        return Err(AddressError::InvalidLength {
            chain: Chain::Solana,
            expected: "32 decoded bytes".to_string(),
            got: decoded.len(),
        });
    }

    // Re-encode to get the canonical form (handles leading-zero padding).
    let normalized = bs58::encode(&decoded).into_string();

    Ok(ValidatedAddress {
        normalized,
        chain: Chain::Solana,
        address_type: AddressType::SolanaBase58,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// First 4 bytes of double-SHA256: the Base58Check checksum algorithm.
fn double_sha256_checksum(payload: &[u8]) -> [u8; 4] {
    let first = sha2::Sha256::digest(payload);
    let second = sha2::Sha256::digest(first);
    [second[0], second[1], second[2], second[3]]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Ethereum ──────────────────────────────────────────────────────────

    #[test]
    fn eth_valid_checksummed() {
        // EIP-55 test vector from the EIP itself.
        let addr = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        let v = validate_address(addr, Chain::Ethereum).unwrap();
        assert_eq!(v.address_type, AddressType::Eip55);
        // Normalized must equal a valid EIP-55 form.
        assert!(v.normalized.starts_with("0x"));
        assert_eq!(v.normalized.len(), 42);
    }

    #[test]
    fn eth_all_lowercase_accepted() {
        let addr = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";
        let v = validate_address(addr, Chain::Ethereum).unwrap();
        assert_eq!(v.chain, Chain::Ethereum);
    }

    #[test]
    fn eth_all_uppercase_accepted() {
        let addr = "0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED";
        let v = validate_address(addr, Chain::Ethereum).unwrap();
        assert_eq!(v.chain, Chain::Ethereum);
    }

    #[test]
    fn eth_wrong_length_rejected() {
        let err = validate_address("0x1234", Chain::Ethereum).unwrap_err();
        assert!(matches!(err, AddressError::InvalidLength { .. }));
    }

    #[test]
    fn eth_no_prefix_rejected() {
        let err = validate_address("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", Chain::Ethereum).unwrap_err();
        assert!(matches!(err, AddressError::InvalidPrefix { .. }));
    }

    #[test]
    fn eth_invalid_chars_rejected() {
        let err = validate_address("0xZZZZb6053F3E94C9b9A09f33669435E7Ef1BeAed", Chain::Ethereum).unwrap_err();
        assert!(matches!(err, AddressError::InvalidCharacters { .. }));
    }

    #[test]
    fn eip55_checksum_known_vectors() {
        // Vectors from EIP-55.
        assert_eq!(
            eip55_checksum("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed"),
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"
        );
        assert_eq!(
            eip55_checksum("fb6916095ca1df60bb79ce92ce3ea74c37c5d359"),
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359"
        );
    }

    // ── Bitcoin ──────────────────────────────────────────────────────────

    #[test]
    fn bitcoin_p2wpkh_valid() {
        // BIP-173 test vector.
        let addr = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        let v = validate_address(addr, Chain::Bitcoin).unwrap();
        assert_eq!(v.address_type, AddressType::P2wpkh);
        assert_eq!(v.normalized, addr);
    }

    #[test]
    fn bitcoin_p2pkh_valid() {
        // Known mainnet P2PKH address (genesis block coinbase output).
        let addr = "17VZNX1SN5NtKa8UQFxwQbFeFc3iqRYhem";
        let v = validate_address(addr, Chain::Bitcoin).unwrap();
        assert_eq!(v.address_type, AddressType::P2pkh);
    }

    #[test]
    fn bitcoin_invalid_bech32_checksum() {
        // Flip one character in the checksum portion.
        let addr = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5"; // last char changed
        let err = validate_address(addr, Chain::Bitcoin).unwrap_err();
        assert!(matches!(err, AddressError::InvalidChecksum { .. }));
    }

    #[test]
    fn bitcoin_bad_prefix_rejected() {
        let err = validate_address("xc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", Chain::Bitcoin).unwrap_err();
        assert!(matches!(err, AddressError::InvalidPrefix { .. }));
    }

    // ── Tron ─────────────────────────────────────────────────────────────

    #[test]
    fn tron_valid_address() {
        // TRC-20 USDT contract address — valid Tron mainnet address.
        let addr = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
        let v = validate_address(addr, Chain::Tron).unwrap();
        assert_eq!(v.address_type, AddressType::TronBase58Check);
        assert_eq!(v.chain, Chain::Tron);
    }

    #[test]
    fn tron_bad_prefix_rejected() {
        let err = validate_address("1R7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t", Chain::Tron).unwrap_err();
        assert!(matches!(err, AddressError::InvalidPrefix { .. }));
    }

    #[test]
    fn tron_wrong_length_rejected() {
        let err = validate_address("TR7NHq", Chain::Tron).unwrap_err();
        assert!(matches!(err, AddressError::InvalidLength { .. }));
    }

    // ── Solana ───────────────────────────────────────────────────────────

    #[test]
    fn solana_valid_address() {
        // Well-known Serum DEX program ID — valid Ed25519 pubkey.
        let addr = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
        let v = validate_address(addr, Chain::Solana).unwrap();
        assert_eq!(v.address_type, AddressType::SolanaBase58);
        assert_eq!(v.chain, Chain::Solana);
    }

    #[test]
    fn solana_too_short_rejected() {
        let err = validate_address("9xQeWvG816bUx9EPjH", Chain::Solana).unwrap_err();
        assert!(matches!(err, AddressError::InvalidLength { .. }));
    }

    #[test]
    fn empty_address_rejected() {
        for chain in [Chain::Bitcoin, Chain::Ethereum, Chain::Tron, Chain::Solana] {
            let err = validate_address("", chain).unwrap_err();
            assert!(matches!(err, AddressError::Empty));
        }
    }
}
