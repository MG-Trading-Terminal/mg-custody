//! Cryptographic utilities — hashing, encoding, checksums, key derivation helpers.

/// SHA-256 hash.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let result = sha2::Sha256::digest(data);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Double SHA-256 (used in Bitcoin).
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
    sha256(&sha256(data))
}

/// RIPEMD-160(SHA-256(data)) — used in Bitcoin address derivation.
pub fn hash160(data: &[u8]) -> [u8; 20] {
    use sha2::Digest;
    let sha = sha2::Sha256::digest(data);
    let ripe = ripemd::Ripemd160::digest(sha);
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&ripe);
    hash
}

/// Keccak-256 hash (used in Ethereum).
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    use sha3::Digest;
    let result = sha3::Keccak256::digest(data);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// HMAC-SHA256.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).unwrap();
    mac.update(data);
    let result = mac.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result.into_bytes());
    hash
}

/// HMAC-SHA512.
pub fn hmac_sha512(key: &[u8], data: &[u8]) -> [u8; 64] {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha512>::new_from_slice(key).unwrap();
    mac.update(data);
    let result = mac.finalize();
    let mut hash = [0u8; 64];
    hash.copy_from_slice(&result.into_bytes());
    hash
}

/// Base58Check encode (used in Bitcoin/Tron).
pub fn base58check_encode(version: u8, payload: &[u8]) -> String {
    let mut data = Vec::with_capacity(1 + payload.len() + 4);
    data.push(version);
    data.extend_from_slice(payload);
    let checksum = double_sha256(&data);
    data.extend_from_slice(&checksum[..4]);
    bs58::encode(data).into_string()
}

/// Verify Base58Check checksum.
pub fn base58check_verify(encoded: &str) -> bool {
    let decoded = match bs58::decode(encoded).into_vec() {
        Ok(d) => d,
        Err(_) => return false,
    };
    if decoded.len() < 5 { return false; }
    let (payload, checksum) = decoded.split_at(decoded.len() - 4);
    let expected = double_sha256(payload);
    checksum == &expected[..4]
}

/// EIP-55 mixed-case checksum for Ethereum addresses.
pub fn eip55_checksum(address: &str) -> String {
    let addr = address.trim_start_matches("0x").to_lowercase();
    let hash = keccak256(addr.as_bytes());
    let hash_hex = hex::encode(hash);

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");
    for (i, c) in addr.chars().enumerate() {
        if c.is_ascii_digit() {
            checksummed.push(c);
        } else {
            let nibble = u8::from_str_radix(&hash_hex[i..i + 1], 16).unwrap_or(0);
            if nibble >= 8 { checksummed.push(c.to_ascii_uppercase()); }
            else { checksummed.push(c); }
        }
    }
    checksummed
}

/// Verify EIP-55 checksum.
pub fn verify_eip55(address: &str) -> bool {
    if !address.starts_with("0x") || address.len() != 42 { return false; }
    eip55_checksum(address) == address
}

/// Generate a random 32-byte value.
pub fn random_bytes_32() -> [u8; 32] {
    rand::random()
}

/// Constant-time comparison of two byte slices.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (&x, &y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty() {
        let hash = sha256(b"");
        assert_eq!(hex::encode(hash), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn sha256_hello() {
        let hash = sha256(b"hello");
        assert_eq!(hex::encode(hash), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn double_sha256_known() {
        let hash = double_sha256(b"hello");
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, sha256(b"hello")); // different from single hash
    }

    #[test]
    fn hash160_known() {
        let hash = hash160(b"test");
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn keccak256_empty() {
        let hash = keccak256(b"");
        assert_eq!(hex::encode(hash), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    }

    #[test]
    fn keccak256_hello() {
        let hash = keccak256(b"hello");
        assert_eq!(hex::encode(hash), "1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8");
    }

    #[test]
    fn hmac_sha256_known() {
        let mac = hmac_sha256(b"key", b"message");
        assert_eq!(mac.len(), 32);
        // Known HMAC-SHA256("key", "message")
        assert_eq!(hex::encode(mac), "6e9ef29b75fffc5b7abae527d58fdadb2fe42e7219011976917343065f58ed4a");
    }

    #[test]
    fn hmac_sha512_length() {
        let mac = hmac_sha512(b"key", b"data");
        assert_eq!(mac.len(), 64);
    }

    #[test]
    fn base58check_bitcoin_mainnet() {
        // Version 0x00 = Bitcoin mainnet P2PKH
        let payload = [0u8; 20]; // zero hash
        let encoded = base58check_encode(0x00, &payload);
        assert!(encoded.starts_with('1'));
        assert!(base58check_verify(&encoded));
    }

    #[test]
    fn base58check_tron() {
        // Version 0x41 = Tron mainnet
        let payload = [0u8; 20];
        let encoded = base58check_encode(0x41, &payload);
        assert!(encoded.starts_with('T'));
        assert!(base58check_verify(&encoded));
    }

    #[test]
    fn base58check_invalid() {
        assert!(!base58check_verify("invalid"));
        assert!(!base58check_verify("1111")); // too short
    }

    #[test]
    fn eip55_checksum_known() {
        let addr = eip55_checksum("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed");
        assert_eq!(addr, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    #[test]
    fn eip55_all_lowercase_input() {
        let addr = eip55_checksum("0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359");
        assert_eq!(addr, "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359");
    }

    #[test]
    fn eip55_verify_correct() {
        assert!(verify_eip55("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
    }

    #[test]
    fn eip55_verify_wrong_case() {
        assert!(!verify_eip55("0x5AAEB6053F3E94C9B9A09F33669435E7EF1BEAED"));
    }

    #[test]
    fn random_bytes_unique() {
        let a = random_bytes_32();
        let b = random_bytes_32();
        assert_ne!(a, b);
    }

    #[test]
    fn constant_time_eq_same() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn constant_time_eq_different() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn constant_time_eq_different_length() {
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn hash_deterministic() {
        assert_eq!(sha256(b"test"), sha256(b"test"));
        assert_eq!(keccak256(b"test"), keccak256(b"test"));
        assert_eq!(hash160(b"test"), hash160(b"test"));
    }

    #[test]
    fn different_hash_functions_different_results() {
        let s = sha256(b"same input");
        let k = keccak256(b"same input");
        assert_ne!(&s[..], &k[..]);
    }
}
