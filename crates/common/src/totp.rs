//! TOTP (Time-based One-Time Password) implementation per RFC 6238 / RFC 4226.
//!
//! Uses HMAC-SHA1 as specified in RFC 6238 (the default algorithm used by
//! Google Authenticator, Authy, and all major authenticator apps).
//!
//! Security properties:
//! - 20-byte secret, generated from OsRng (CSPRNG)
//! - 6-digit code, 30-second step window
//! - Clock-skew tolerance: ±1 window (accepts current, previous, next step)
//! - Secret is Zeroize + ZeroizeOnDrop — memory is cleared on drop
//!
//! References:
//! - RFC 6238: TOTP — <https://www.rfc-editor.org/rfc/rfc6238>
//! - RFC 4226: HOTP — <https://www.rfc-editor.org/rfc/rfc4226>
//! - Key Uri Format: <https://github.com/google/google-authenticator/wiki/Key-Uri-Format>

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha1 = Hmac<Sha1>;

/// Step size in seconds per RFC 6238 §5.2 (T0 = Unix epoch, X = 30s).
const TOTP_STEP_SECONDS: u64 = 30;
/// Number of digits in the OTP.
const TOTP_DIGITS: u32 = 6;
/// Number of windows to check on either side of the current step (clock skew).
const TOTP_SKEW_WINDOWS: i64 = 1;
/// Secret length in bytes per RFC 4226 §4 recommendation.
const SECRET_BYTES: usize = 20;

/// TOTP secret — 20 raw bytes stored and zeroed on drop.
///
/// Never log, serialize, or persist this type without encryption.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct TotpSecret {
    pub raw: [u8; SECRET_BYTES],
}

impl TotpSecret {
    /// Generate a cryptographically random TOTP secret using OsRng.
    pub fn generate() -> Self {
        let mut raw = [0u8; SECRET_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut raw);
        Self { raw }
    }

    /// Decode from a base32 string (as stored encrypted in the database).
    ///
    /// Returns None if the input is not valid base32 or wrong length.
    pub fn from_base32(encoded: &str) -> Option<Self> {
        let decoded = base32_decode(encoded)?;
        if decoded.len() != SECRET_BYTES {
            return None;
        }
        let mut raw = [0u8; SECRET_BYTES];
        raw.copy_from_slice(&decoded);
        Some(Self { raw })
    }

    /// Encode as base32 string (standard RFC 4648 alphabet, no padding).
    ///
    /// Used for QR code generation and manual entry in authenticator apps.
    pub fn to_base32(&self) -> String {
        base32_encode(&self.raw)
    }

    /// Generate the TOTP code for a given Unix timestamp.
    ///
    /// `time` is Unix timestamp in seconds (use `SystemTime::now()` or a test value).
    pub fn generate_code(&self, time: u64) -> String {
        let counter = time / TOTP_STEP_SECONDS;
        hotp_code(&self.raw, counter)
    }

    /// Verify a submitted TOTP code against the current time window, ±1 step for clock skew.
    ///
    /// `time` is Unix timestamp in seconds.
    /// Returns true only if the code matches any of the three accepted windows.
    ///
    /// This method is stateless — it does NOT defend against
    /// replay of a code within its validity window. Callers that
    /// need replay defence must use `verify_counter` and persist
    /// the returned counter as the per-user "last accepted".
    pub fn verify(&self, code: &str, time: u64) -> bool {
        self.verify_counter(code, time).is_some()
    }

    /// Verify and return the matched TOTP counter.
    ///
    /// Returns `Some(counter)` if the code matches any of the
    /// three windows (current, previous, next) for clock skew.
    /// Returns `None` for invalid format or no match.
    ///
    /// The returned counter is the authoritative value for
    /// replay defence: persist `max(stored, counter)` per user,
    /// and reject any subsequent submission whose matched
    /// counter is ≤ the stored value.
    pub fn verify_counter(&self, code: &str, time: u64) -> Option<u64> {
        if code.len() != TOTP_DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let current_counter = (time / TOTP_STEP_SECONDS) as i64;
        for delta in -TOTP_SKEW_WINDOWS..=TOTP_SKEW_WINDOWS {
            let counter = (current_counter + delta) as u64;
            let expected = hotp_code(&self.raw, counter);
            if constant_time_eq(code.as_bytes(), expected.as_bytes()) {
                return Some(counter);
            }
        }
        None
    }

    /// Generate a provisioning URI for authenticator apps (otpauth:// scheme).
    ///
    /// `email` is the account identifier shown in the app.
    /// `issuer` is the service name shown in the app.
    ///
    /// Reference: <https://github.com/google/google-authenticator/wiki/Key-Uri-Format>
    pub fn provisioning_uri(&self, email: &str, issuer: &str) -> String {
        let secret = self.to_base32();
        // Percent-encode basic URI characters in issuer and email
        let issuer_enc = percent_encode(issuer);
        let email_enc = percent_encode(email);
        format!(
            "otpauth://totp/{issuer_enc}:{email_enc}?secret={secret}&issuer={issuer_enc}&algorithm=SHA1&digits={TOTP_DIGITS}&period={TOTP_STEP_SECONDS}"
        )
    }
}

/// Compute HOTP(key, counter) and return a zero-padded TOTP_DIGITS-digit string.
///
/// RFC 4226 §5.3 algorithm:
/// 1. HMAC-SHA1(key, counter_big_endian_8_bytes) → 20-byte MAC
/// 2. offset = mac[19] & 0x0f
/// 3. P = mac[offset..offset+4] as big-endian u32, mask high bit
/// 4. code = P % 10^digits, zero-padded to `digits` chars
fn hotp_code(key: &[u8], counter: u64) -> String {
    let counter_bytes = counter.to_be_bytes();

    let mut mac =
        HmacSha1::new_from_slice(key).expect("HMAC-SHA1 accepts any key length");
    mac.update(&counter_bytes);
    let result = mac.finalize().into_bytes();

    // Dynamic truncation per RFC 4226 §5.3
    let offset = (result[19] & 0x0f) as usize;
    let p = u32::from_be_bytes([
        result[offset] & 0x7f, // mask sign bit
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);

    let modulus = 10u32.pow(TOTP_DIGITS);
    let code = p % modulus;
    format!("{:0>width$}", code, width = TOTP_DIGITS as usize)
}

/// Constant-time byte comparison to prevent timing oracle attacks on code verification.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

// ---------------------------------------------------------------------------
// Minimal base32 (RFC 4648, standard alphabet, no padding)
// ---------------------------------------------------------------------------

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encode bytes as base32 string (no padding).
fn base32_encode(input: &[u8]) -> String {
    let mut output = String::new();
    let mut buffer: u64 = 0;
    let mut bits_in_buffer = 0u32;

    for &byte in input {
        buffer = (buffer << 8) | (byte as u64);
        bits_in_buffer += 8;
        while bits_in_buffer >= 5 {
            bits_in_buffer -= 5;
            let idx = ((buffer >> bits_in_buffer) & 0x1f) as usize;
            output.push(BASE32_ALPHABET[idx] as char);
        }
    }

    if bits_in_buffer > 0 {
        let idx = ((buffer << (5 - bits_in_buffer)) & 0x1f) as usize;
        output.push(BASE32_ALPHABET[idx] as char);
    }

    output
}

/// Decode a base32 string (case-insensitive, ignores '=' padding).
///
/// Returns None if the input contains invalid base32 characters.
fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut buffer: u64 = 0;
    let mut bits_in_buffer = 0u32;
    let mut output = Vec::new();

    for ch in input.chars() {
        if ch == '=' {
            continue; // ignore padding
        }
        let ch_upper = ch.to_ascii_uppercase();
        let value = BASE32_ALPHABET.iter().position(|&b| b as char == ch_upper)?;
        buffer = (buffer << 5) | (value as u64);
        bits_in_buffer += 5;
        if bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            output.push(((buffer >> bits_in_buffer) & 0xff) as u8);
        }
    }

    Some(output)
}

/// Minimal percent-encoding for URI label characters.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b'@' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0').to_ascii_uppercase());
                out.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0').to_ascii_uppercase());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // RFC 6238 test vectors (Appendix B) — SHA1 variant
    // Seed: "12345678901234567890" (ASCII)
    // ---------------------------------------------------------------------------

    fn rfc_secret() -> TotpSecret {
        let raw: [u8; 20] = *b"12345678901234567890";
        TotpSecret { raw }
    }

    #[test]
    fn rfc6238_test_vectors() {
        // RFC 6238 Appendix B vectors (SHA1, 8 digits but we test 6 — just verify HOTP math)
        // We verify our counter → code logic is correct by cross-checking with known values.
        // For 6 digits these are: code = hotp(seed, counter) % 1_000_000
        let secret = rfc_secret();

        // T = 59s → counter = 1 → HOTP(seed,1) = 94287082 → %1e6 = 287082
        let code_t59 = hotp_code(&secret.raw, 59 / 30);
        assert_eq!(code_t59, "287082");

        // T = 1111111109s → counter = 37037036 → HOTP = 07081804 → %1e6 = 081804
        let code_t1 = hotp_code(&secret.raw, 1_111_111_109 / 30);
        assert_eq!(code_t1, "081804");

        // T = 1111111111s → counter = 37037037 → HOTP = 14050471 → %1e6 = 050471
        let code_t2 = hotp_code(&secret.raw, 1_111_111_111 / 30);
        assert_eq!(code_t2, "050471");
    }

    #[test]
    fn generate_code_is_six_digits() {
        let secret = TotpSecret::generate();
        let code = secret.generate_code(1_700_000_000);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn verify_current_window() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;
        let code = secret.generate_code(time);
        assert!(secret.verify(&code, time));
    }

    #[test]
    fn verify_adjacent_windows_clock_skew() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;

        // Previous window
        let prev_code = secret.generate_code(time - TOTP_STEP_SECONDS);
        assert!(secret.verify(&prev_code, time), "prev window must be accepted");

        // Next window
        let next_code = secret.generate_code(time + TOTP_STEP_SECONDS);
        assert!(secret.verify(&next_code, time), "next window must be accepted");
    }

    #[test]
    fn verify_rejects_stale_code() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;
        // 2 windows back = definitely outside ±1 skew tolerance
        let stale_code = secret.generate_code(time - 2 * TOTP_STEP_SECONDS);
        assert!(!secret.verify(&stale_code, time));
    }

    #[test]
    fn verify_rejects_wrong_code() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;
        assert!(!secret.verify("000000", time));
        assert!(!secret.verify("999999", time));
    }

    #[test]
    fn verify_rejects_invalid_format() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;
        assert!(!secret.verify("12345", time)); // 5 digits
        assert!(!secret.verify("1234567", time)); // 7 digits
        assert!(!secret.verify("abcdef", time)); // non-numeric
        assert!(!secret.verify("", time)); // empty
    }

    #[test]
    fn base32_roundtrip() {
        let secret = TotpSecret::generate();
        let encoded = secret.to_base32();
        let decoded = TotpSecret::from_base32(&encoded).expect("decode should succeed");
        assert_eq!(secret.raw, decoded.raw);
    }

    #[test]
    fn base32_known_value() {
        // "12345678901234567890" in base32 = GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ
        let raw: [u8; 20] = *b"12345678901234567890";
        let secret = TotpSecret { raw };
        assert_eq!(secret.to_base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    }

    #[test]
    fn provisioning_uri_format() {
        let raw: [u8; 20] = *b"12345678901234567890";
        let secret = TotpSecret { raw };
        let uri = secret.provisioning_uri("user@example.com", "MG Custody");
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
        assert!(uri.contains("issuer=MG%20Custody"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
        assert!(uri.contains("algorithm=SHA1"));
    }

    #[test]
    fn from_base32_rejects_wrong_length() {
        assert!(TotpSecret::from_base32("AAAA").is_none());
        assert!(TotpSecret::from_base32("").is_none());
    }

    #[test]
    fn from_base32_rejects_invalid_chars() {
        // '1' and '8' are not in the base32 alphabet
        assert!(TotpSecret::from_base32("1111111111111111111111111111111111").is_none());
    }

    #[test]
    fn verify_counter_returns_matched_step() {
        let secret = rfc_secret();
        let time = 59u64;
        // Known vector: counter = 1 at t=59
        let code = hotp_code(&secret.raw, 1);
        assert_eq!(secret.verify_counter(&code, time), Some(1));
    }

    #[test]
    fn verify_counter_matches_skew_windows() {
        let secret = TotpSecret::generate();
        let time = 1_700_000_000u64;
        let cur = time / TOTP_STEP_SECONDS;

        // Current window
        let cur_code = hotp_code(&secret.raw, cur);
        assert_eq!(secret.verify_counter(&cur_code, time), Some(cur));

        // Previous window (caller uses this to reject replay)
        let prev_code = hotp_code(&secret.raw, cur - 1);
        assert_eq!(secret.verify_counter(&prev_code, time), Some(cur - 1));

        // Next window
        let next_code = hotp_code(&secret.raw, cur + 1);
        assert_eq!(secret.verify_counter(&next_code, time), Some(cur + 1));
    }

    #[test]
    fn verify_counter_rejects_invalid() {
        let secret = TotpSecret::generate();
        assert_eq!(secret.verify_counter("000000", 1_700_000_000), None);
        assert_eq!(secret.verify_counter("abcdef", 1_700_000_000), None);
        assert_eq!(secret.verify_counter("12345", 1_700_000_000), None);
    }

    #[test]
    fn codes_differ_across_steps() {
        let secret = rfc_secret();
        let t0 = 1_700_000_000u64;
        // Different 30-second windows should produce different codes (overwhelmingly likely)
        let code1 = secret.generate_code(t0);
        let code2 = secret.generate_code(t0 + 30);
        // Not guaranteed to differ but extremely unlikely to collide; use known-distinct windows
        let code3 = secret.generate_code(t0 + 60);
        // At least two of three should differ — but with RFC seed, verify all three differ
        assert!(code1 != code2 || code2 != code3 || code1 != code3,
            "at least some codes across different windows should differ");
    }
}
