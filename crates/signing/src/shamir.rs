//! Shamir's Secret Sharing over GF(256) for seed backup.
//!
//! Splits a secret into `n` shares such that any `k` shares can reconstruct
//! the original. Uses arithmetic in GF(2^8) with the AES irreducible polynomial
//! x^8 + x^4 + x^3 + x + 1 (0x11B).
//!
//! Default configuration: 3-of-5 (k=3 threshold, n=5 shares).
//!
//! Each share is tagged with a 1-based index (x-coordinate) and the share data.
//! Share format: `index (1 byte) || share_data (same length as secret)`.
//!
//! Reference: Shamir, A. "How to Share a Secret" (1979).
//!            <https://dl.acm.org/doi/10.1145/359168.359176>

use anyhow::{Result, ensure};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A single share of a split secret.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Share {
    /// 1-based share index (x-coordinate in the polynomial).
    pub index: u8,
    /// Share data (same length as the original secret).
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Share {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Share(index={}, len={})", self.index, self.data.len())
    }
}

impl Clone for Share {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            data: self.data.clone(),
        }
    }
}

// ── GF(256) arithmetic ───────────────────────────────────────────

/// GF(256) addition (XOR).
fn gf_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// GF(256) multiplication using Russian Peasant algorithm.
/// Irreducible polynomial: x^8 + x^4 + x^3 + x + 1 = 0x11B.
fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut result: u8 = 0;
    while b > 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        let carry = a & 0x80;
        a <<= 1;
        if carry != 0 {
            a ^= 0x1B; // Reduce by x^8 + x^4 + x^3 + x + 1 (lower 8 bits of 0x11B)
        }
        b >>= 1;
    }
    result
}

/// GF(256) multiplicative inverse using extended Euclidean / exponentiation.
/// a^(-1) = a^254 in GF(256) (since a^255 = 1 for nonzero a).
fn gf_inv(a: u8) -> u8 {
    assert_ne!(a, 0, "Cannot invert zero in GF(256)");
    let mut result = a;
    // a^254 = ((((((a^2)^2 · a)^2 · a)^2)^2 · a)^2)^2 · a ... etc
    // Simpler: repeated squaring for a^(2^k) then multiply
    // a^254 by square-and-multiply:
    for _ in 0..6 {
        result = gf_mul(result, result); // square
        result = gf_mul(result, a);      // multiply by a
    }
    result = gf_mul(result, result); // final square: a^254
    result
}

/// GF(256) division: a / b.
fn gf_div(a: u8, b: u8) -> u8 {
    gf_mul(a, gf_inv(b))
}

// ── Shamir SSS ───────────────────────────────────────────────────

/// Split a secret into `n` shares with threshold `k`.
///
/// Any `k` shares can reconstruct the secret. Fewer than `k` shares
/// reveal no information about the secret (information-theoretic security).
///
/// # Panics
/// - If `k < 2` or `n < k` or `n > 255` or secret is empty.
pub fn split(secret: &[u8], k: u8, n: u8) -> Result<Vec<Share>> {
    ensure!(k >= 2, "Threshold must be at least 2, got {k}");
    ensure!(n >= k, "Share count {n} must be >= threshold {k}");
    ensure!(!secret.is_empty(), "Secret must not be empty");

    let mut rng = rand::thread_rng();

    // For each byte of the secret, create a random polynomial of degree k-1
    // where the constant term is the secret byte.
    let mut shares: Vec<Share> = (1..=n)
        .map(|i| Share {
            index: i,
            data: Vec::with_capacity(secret.len()),
        })
        .collect();

    let mut coefficients = vec![0u8; (k - 1) as usize];

    for &secret_byte in secret {
        // Random coefficients for this byte's polynomial (degree 1..k-1)
        rng.fill_bytes(&mut coefficients);

        // Evaluate polynomial at x = share.index for each share
        for share in &mut shares {
            let x = share.index;
            let mut y = secret_byte; // constant term (a_0 = secret byte)
            let mut x_pow = x; // x^1

            for &coeff in &coefficients {
                y = gf_add(y, gf_mul(coeff, x_pow));
                x_pow = gf_mul(x_pow, x);
            }

            share.data.push(y);
        }
    }

    coefficients.zeroize();

    tracing::info!(threshold = k, shares = n, secret_len = secret.len(), "Secret split into shares");
    Ok(shares)
}

/// Reconstruct a secret from `k` or more shares using Lagrange interpolation.
///
/// The shares must have been produced by `split` with the same `k`.
/// Providing fewer than `k` shares will silently produce wrong output
/// (there is no checksum in basic Shamir).
pub fn reconstruct(shares: &[Share]) -> Result<Vec<u8>> {
    ensure!(!shares.is_empty(), "Need at least one share to reconstruct");

    let secret_len = shares[0].data.len();
    ensure!(
        shares.iter().all(|s| s.data.len() == secret_len),
        "All shares must have the same length"
    );

    // Verify unique indices
    let mut seen = [false; 256];
    for share in shares {
        ensure!(!seen[share.index as usize], "Duplicate share index {}", share.index);
        seen[share.index as usize] = true;
    }

    let mut secret = vec![0u8; secret_len];

    // Lagrange interpolation at x=0 for each byte position
    for byte_idx in 0..secret_len {
        let mut value = 0u8;

        for (i, share_i) in shares.iter().enumerate() {
            let x_i = share_i.index;
            let y_i = share_i.data[byte_idx];

            // Lagrange basis polynomial L_i(0)
            let mut basis = 1u8;
            for (j, share_j) in shares.iter().enumerate() {
                if i == j {
                    continue;
                }
                let x_j = share_j.index;
                // L_i(0) *= (0 - x_j) / (x_i - x_j) = x_j / (x_i ^ x_j) in GF(256)
                basis = gf_mul(basis, gf_div(x_j, gf_add(x_i, x_j)));
            }

            value = gf_add(value, gf_mul(y_i, basis));
        }

        secret[byte_idx] = value;
    }

    tracing::info!(shares_used = shares.len(), secret_len, "Secret reconstructed from shares");
    Ok(secret)
}

/// Split a seed using 3-of-5 Shamir Secret Sharing (default configuration).
pub fn split_3_of_5(secret: &[u8]) -> Result<Vec<Share>> {
    split(secret, 3, 5)
}

/// Reconstruct from any 3+ shares of a 3-of-5 split.
pub fn reconstruct_3_of_5(shares: &[Share]) -> Result<Vec<u8>> {
    ensure!(shares.len() >= 3, "Need at least 3 shares for 3-of-5 reconstruction");
    reconstruct(shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secret() -> Vec<u8> {
        (0..64).collect() // 64-byte test secret
    }

    #[test]
    fn gf256_arithmetic() {
        // Addition is XOR
        assert_eq!(gf_add(0x53, 0xCA), 0x99);

        // Multiplication
        assert_eq!(gf_mul(0x53, 0xCA), 0x01); // These are inverses in AES GF(256)

        // Multiplicative identity
        assert_eq!(gf_mul(0x53, 1), 0x53);
        assert_eq!(gf_mul(1, 0xCA), 0xCA);

        // Zero multiplication
        assert_eq!(gf_mul(0, 0x53), 0);
        assert_eq!(gf_mul(0xCA, 0), 0);
    }

    #[test]
    fn gf256_inverse() {
        // a * a^(-1) = 1
        for a in 1..=255u8 {
            let inv = gf_inv(a);
            assert_eq!(gf_mul(a, inv), 1, "Inverse failed for {a}");
        }
    }

    #[test]
    fn split_3_of_5_roundtrip() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        assert_eq!(shares.len(), 5);
        for share in &shares {
            assert_eq!(share.data.len(), 64);
        }

        // Reconstruct with all 5 shares
        let recovered = reconstruct(&shares).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn reconstruct_with_minimum_threshold() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        // Any 3 shares should work
        let combos: Vec<[usize; 3]> = vec![
            [0, 1, 2],
            [0, 1, 3],
            [0, 1, 4],
            [0, 2, 3],
            [0, 2, 4],
            [0, 3, 4],
            [1, 2, 3],
            [1, 2, 4],
            [1, 3, 4],
            [2, 3, 4],
        ];

        for combo in combos {
            let subset: Vec<Share> = combo.iter().map(|&i| shares[i].clone()).collect();
            let recovered = reconstruct(&subset).unwrap();
            assert_eq!(
                recovered, secret,
                "Failed to reconstruct with shares {:?}",
                combo
            );
        }
    }

    #[test]
    fn reconstruct_with_4_shares() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        let subset: Vec<Share> = shares[0..4].iter().map(|s| s.clone()).collect();
        let recovered = reconstruct(&subset).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn two_shares_give_wrong_result() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        // 2 shares (below threshold) should NOT reconstruct correctly
        let subset: Vec<Share> = shares[0..2].iter().map(|s| s.clone()).collect();
        let recovered = reconstruct(&subset).unwrap();
        assert_ne!(recovered, secret, "2 shares should not reconstruct a 3-of-5 secret");
    }

    #[test]
    fn split_produces_different_shares() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        // All shares should be different from each other
        for i in 0..shares.len() {
            for j in (i + 1)..shares.len() {
                assert_ne!(shares[i].data, shares[j].data);
            }
        }
    }

    #[test]
    fn share_debug_is_safe() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();
        let debug = format!("{:?}", shares[0]);
        assert!(debug.contains("index=1"));
        assert!(debug.contains("len=64"));
        // Should not contain actual share data
        assert!(!debug.contains("[0, "));
    }

    #[test]
    fn split_rejects_invalid_params() {
        assert!(split(&[1, 2, 3], 1, 5).is_err()); // k < 2
        assert!(split(&[1, 2, 3], 5, 3).is_err()); // n < k
        assert!(split(&[], 3, 5).is_err());          // empty secret
    }

    #[test]
    fn duplicate_share_indices_rejected() {
        let secret = test_secret();
        let shares = split_3_of_5(&secret).unwrap();

        let dup = vec![shares[0].clone(), shares[0].clone(), shares[1].clone()];
        assert!(reconstruct(&dup).is_err());
    }

    #[test]
    fn single_byte_secret() {
        let secret = vec![42u8];
        let shares = split(&secret, 2, 3).unwrap();
        let recovered = reconstruct(&shares[0..2].iter().map(|s| s.clone()).collect::<Vec<_>>()).unwrap();
        assert_eq!(recovered, secret);
    }

    #[test]
    fn custom_threshold_2_of_3() {
        let secret = test_secret();
        let shares = split(&secret, 2, 3).unwrap();

        assert_eq!(shares.len(), 3);

        // Any 2 shares reconstruct
        for combo in [[0, 1], [0, 2], [1, 2]] {
            let subset: Vec<Share> = combo.iter().map(|&i| shares[i].clone()).collect();
            let recovered = reconstruct(&subset).unwrap();
            assert_eq!(recovered, secret);
        }
    }
}
