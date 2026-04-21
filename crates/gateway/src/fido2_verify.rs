//! Real WebAuthn / FIDO2 assertion verification.
//!
//! Supports ES256 (ECDSA P-256 with SHA-256) — the universal
//! case covered by YubiKeys, Touch ID, Windows Hello, and every
//! modern platform authenticator. Other algorithms (RS256,
//! EdDSA) are a straightforward extension when we need them;
//! keep the match on kty/alg honest so unsupported algos
//! surface as verification errors rather than silent passes.
//!
//! Reference: W3C WebAuthn L3 §7.2 "Verifying an Authentication
//! Assertion" — https://www.w3.org/TR/webauthn-3/#sctn-verifying-assertion
//!
//! What the caller passes in (all base64url-encoded):
//!   - `client_data_json` — raw JSON produced by the browser
//!   - `authenticator_data` — raw authenticatorData from the
//!     authenticator response
//!   - `signature` — raw ECDSA signature (DER-encoded, which is
//!     what browser authenticators emit)
//!   - `public_key_cose_b64` — the credential's public key, stored
//!     at registration time. We accept either base64-URL or raw
//!     hex for the stored form.
//!
//! What we check (in order):
//!   1. clientDataJSON JSON-parses
//!   2. type == "webauthn.get"
//!   3. challenge == expected (base64url-compared)
//!   4. origin == expected
//!   5. COSE key decodes as P-256 ES256
//!   6. signature verifies over `authenticatorData || sha256(clientDataJSON)`
//!
//! Any failure returns Err with a message precise enough for an
//! operator to diagnose — "origin mismatch: expected X, got Y" is
//! more useful than "verification failed".

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;

/// Expected relying-party origin, per CUSTODY_FIDO2_RP_ORIGIN env.
/// Defaults to "https://custody.local" matching the dev default
/// used in fido2_routes. Production deployments MUST set this.
pub fn fido2_expected_origin_default() -> String {
    std::env::var("CUSTODY_FIDO2_RP_ORIGIN")
        .unwrap_or_else(|_| "https://custody.local".into())
}

/// Expected relying-party ID, per CUSTODY_FIDO2_RP_ID env. This
/// is the bare domain the credential was registered against —
/// NOT the full origin. SHA-256(rp_id) is compared against the
/// first 32 bytes of authenticatorData to ensure the assertion
/// was scoped to our RP (WebAuthn §7.2 step 14).
pub fn fido2_expected_rp_id_default() -> String {
    std::env::var("CUSTODY_FIDO2_RP_ID").unwrap_or_else(|_| "custody.local".into())
}

/// Verify a WebAuthn authentication assertion.
///
/// Every input is the raw string as POSTed by the client (except
/// `public_key_cose_b64` which is whatever format we stored at
/// registration — we auto-detect base64url vs hex).
pub fn verify_assertion(
    public_key_cose_b64: &str,
    client_data_json_b64: &str,
    authenticator_data_b64: &str,
    signature_b64: &str,
    expected_challenge_b64url: &str,
    expected_origin: &str,
    expected_rp_id: &str,
) -> Result<()> {
    let client_data = decode_b64url(client_data_json_b64).context("clientDataJSON base64")?;
    let auth_data = decode_b64url(authenticator_data_b64).context("authenticatorData base64")?;
    let signature = decode_b64url(signature_b64).context("signature base64")?;
    let pubkey_bytes = decode_stored_pubkey(public_key_cose_b64)?;

    // 1–4. clientDataJSON structure + bindings.
    let cdj: serde_json::Value =
        serde_json::from_slice(&client_data).context("clientDataJSON is not valid JSON")?;
    let kind = cdj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("clientDataJSON.type missing"))?;
    if kind != "webauthn.get" {
        bail!("clientDataJSON.type = {kind}, expected webauthn.get");
    }
    let challenge = cdj
        .get("challenge")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("clientDataJSON.challenge missing"))?;
    // Browsers emit the challenge base64url-encoded with no padding.
    // Our server-side challenge may be stored as either; normalize
    // by stripping any padding + comparing ignorecase.
    let norm_client = challenge.trim_end_matches('=');
    let norm_expected = expected_challenge_b64url.trim_end_matches('=');
    if norm_client != norm_expected {
        bail!(
            "clientDataJSON.challenge mismatch: client={norm_client}, expected={norm_expected}"
        );
    }
    let origin = cdj
        .get("origin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("clientDataJSON.origin missing"))?;
    if origin != expected_origin {
        bail!("clientDataJSON.origin mismatch: client={origin}, expected={expected_origin}");
    }

    // 5. authenticatorData structural + scope checks. The
    //    server-side audit found this block was previously
    //    skipped — the signature was verified over authData but
    //    the authData itself was never inspected, meaning a
    //    credential bound to a DIFFERENT relying party (rp_id =
    //    evil.com) could authenticate here as long as the user
    //    held the private key. Also a signing ceremony performed
    //    without user interaction (UP flag == 0) passed.
    //    WebAuthn §7.2 requires both checks explicitly.
    //
    //    authData layout:
    //      [0..32]   rpIdHash (SHA-256 of the registered rp_id)
    //      [32]      flags  (bit 0 = UP, bit 2 = UV, bit 6 = AT, bit 7 = ED)
    //      [33..37]  signCount (big-endian u32) — monotonicity is
    //                enforced by the caller via DB CAS, not here
    //      [37..]    attestedCredentialData / extensions (ignored)
    use sha2::{Digest, Sha256};
    if auth_data.len() < 37 {
        bail!(
            "authenticatorData too short ({} bytes, need ≥37 for rpIdHash + flags + signCount)",
            auth_data.len()
        );
    }
    let rp_id_hash_expected = Sha256::digest(expected_rp_id.as_bytes());
    // Constant-time compare is not required here (neither half is
    // secret — the hash is deterministic from public config), but
    // we use a byte-wise compare with clear error messaging.
    if auth_data[..32] != rp_id_hash_expected[..] {
        bail!(
            "authenticatorData.rpIdHash mismatch: assertion was signed for a \
             different relying party (expected SHA-256({expected_rp_id}))"
        );
    }
    let flags = auth_data[32];
    if flags & 0x01 == 0 {
        bail!(
            "authenticatorData.flags: User Presence bit (UP, 0x01) not set — \
             assertion must be produced by a user-interacted authenticator"
        );
    }

    // 6. Parse the COSE public key. YubiKeys + Touch ID + Windows
    //    Hello default to ES256 (alg = -7, kty = EC2, crv = P-256).
    let (pub_x, pub_y) = parse_cose_es256(&pubkey_bytes)
        .context("COSE public key not parseable as ES256 P-256")?;

    // 7. Signature verification — ECDSA over P-256 with SHA-256.
    //    Signed message is authenticatorData || SHA-256(clientDataJSON).
    use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
    use p256::EncodedPoint;

    let client_data_hash = Sha256::digest(&client_data);
    let mut signed = Vec::with_capacity(auth_data.len() + 32);
    signed.extend_from_slice(&auth_data);
    signed.extend_from_slice(&client_data_hash);

    let encoded =
        EncodedPoint::from_affine_coordinates(&pub_x.into(), &pub_y.into(), /*compress=*/ false);
    let verifying_key =
        VerifyingKey::from_encoded_point(&encoded).context("P-256 public key reconstruct")?;

    // Browser authenticators emit ECDSA signatures DER-encoded.
    let sig = Signature::from_der(&signature).context("signature DER parse")?;

    verifying_key
        .verify(&signed, &sig)
        .context("ECDSA verification failed — signature does not match stored public key + message")?;

    Ok(())
}

/// Auto-detect the stored pubkey format: we accept base64url OR
/// hex. Some codebases (including early parts of this one) stored
/// the COSE blob as plain hex; new registrations use base64url.
fn decode_stored_pubkey(s: &str) -> Result<Vec<u8>> {
    if let Ok(b) = decode_b64url(s) {
        return Ok(b);
    }
    hex::decode(s).map_err(|e| anyhow!("public_key_cose is neither base64url nor hex: {e}"))
}

fn decode_b64url(s: &str) -> Result<Vec<u8>> {
    let s = s.trim_end_matches('=');
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| anyhow!("base64url decode: {e}"))
}

/// Minimal CBOR parser that extracts the (x, y) coordinates from
/// a COSE_Key ES256 blob. We don't need a full CBOR library — the
/// structure is a map with a small set of well-known keys:
///
///   { 1: 2,    // kty = EC2
///     3: -7,   // alg = ES256
///    -1: 1,    // crv = P-256
///    -2: bstr32,  // x
///    -3: bstr32   // y
///   }
///
/// CBOR encoding uses 1-byte tags: 0xA5 = map with 5 entries,
/// 0x01..0x17 = positive small int, 0x21..0x37 = negative small
/// (-1..-23 offset by 0x20), 0x58 0x20 = byte string length 32.
/// This hand-roll covers every ES256 key I've seen from real
/// authenticators; anything more exotic returns an error.
fn parse_cose_es256(bytes: &[u8]) -> Result<([u8; 32], [u8; 32])> {
    // Require map header.
    if bytes.is_empty() {
        bail!("COSE blob empty");
    }
    let major = bytes[0] & 0xE0;
    if major != 0xA0 {
        bail!("COSE top-level is not a map (tag 0x{:02x})", bytes[0]);
    }
    // We don't strictly enforce count — just walk entries until
    // we find x and y.
    let mut i = 1;
    let mut x: Option<[u8; 32]> = None;
    let mut y: Option<[u8; 32]> = None;
    let mut kty_ec2 = false;
    let mut alg_es256 = false;
    let mut crv_p256 = false;
    while i < bytes.len() && (x.is_none() || y.is_none()) {
        // Read the key (small positive or negative int).
        let (key, consumed) = read_cbor_int(&bytes[i..])?;
        i += consumed;
        if i >= bytes.len() {
            bail!("COSE truncated after key {key}");
        }
        match key {
            1 => {
                let (v, c) = read_cbor_int(&bytes[i..])?;
                i += c;
                if v == 2 {
                    kty_ec2 = true;
                }
            }
            3 => {
                let (v, c) = read_cbor_int(&bytes[i..])?;
                i += c;
                if v == -7 {
                    alg_es256 = true;
                }
            }
            -1 => {
                let (v, c) = read_cbor_int(&bytes[i..])?;
                i += c;
                if v == 1 {
                    crv_p256 = true;
                }
            }
            -2 => {
                let (bs, c) = read_cbor_bytes(&bytes[i..])?;
                i += c;
                if bs.len() != 32 {
                    bail!("COSE x length {}, expected 32", bs.len());
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(bs);
                x = Some(arr);
            }
            -3 => {
                let (bs, c) = read_cbor_bytes(&bytes[i..])?;
                i += c;
                if bs.len() != 32 {
                    bail!("COSE y length {}, expected 32", bs.len());
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(bs);
                y = Some(arr);
            }
            _ => {
                // Skip unknown value.
                let skipped = skip_cbor_value(&bytes[i..])?;
                i += skipped;
            }
        }
    }
    if !kty_ec2 {
        bail!("COSE kty != EC2 (not an EC key)");
    }
    if !alg_es256 {
        bail!("COSE alg != ES256 (unsupported algorithm; only ES256 is currently wired)");
    }
    if !crv_p256 {
        bail!("COSE crv != P-256");
    }
    Ok((
        x.ok_or_else(|| anyhow!("COSE x coordinate missing"))?,
        y.ok_or_else(|| anyhow!("COSE y coordinate missing"))?,
    ))
}

fn read_cbor_int(bytes: &[u8]) -> Result<(i64, usize)> {
    if bytes.is_empty() {
        bail!("CBOR int: empty input");
    }
    let major = bytes[0] & 0xE0;
    let addl = (bytes[0] & 0x1F) as u64;
    let (value, consumed) = match addl {
        0..=23 => (addl, 1),
        24 if bytes.len() >= 2 => (bytes[1] as u64, 2),
        25 if bytes.len() >= 3 => (u16::from_be_bytes([bytes[1], bytes[2]]) as u64, 3),
        _ => bail!("CBOR int: unsupported additional 0x{addl:x}"),
    };
    match major {
        0x00 => Ok((value as i64, consumed)),
        0x20 => Ok((-(value as i64 + 1), consumed)),
        _ => bail!("CBOR int: expected major 0 or 1, got 0x{major:02x}"),
    }
}

fn read_cbor_bytes(bytes: &[u8]) -> Result<(&[u8], usize)> {
    if bytes.is_empty() {
        bail!("CBOR bytes: empty input");
    }
    let major = bytes[0] & 0xE0;
    if major != 0x40 {
        bail!("CBOR bytes: expected major 2, got 0x{major:02x}");
    }
    let addl = bytes[0] & 0x1F;
    let (len, header) = match addl {
        0..=23 => (addl as usize, 1),
        24 if bytes.len() >= 2 => (bytes[1] as usize, 2),
        25 if bytes.len() >= 3 => (
            u16::from_be_bytes([bytes[1], bytes[2]]) as usize,
            3,
        ),
        _ => bail!("CBOR bytes: unsupported length encoding"),
    };
    if bytes.len() < header + len {
        bail!("CBOR bytes: truncated ({} expected, {} available)", len, bytes.len() - header);
    }
    Ok((&bytes[header..header + len], header + len))
}

/// Walk past one CBOR value without interpreting it. Used to skip
/// map entries with unknown keys.
fn skip_cbor_value(bytes: &[u8]) -> Result<usize> {
    if bytes.is_empty() {
        bail!("CBOR skip: empty input");
    }
    let major = bytes[0] & 0xE0;
    match major {
        0x00 | 0x20 => read_cbor_int(bytes).map(|(_, c)| c),
        0x40 | 0x60 => {
            let (_, c) = read_cbor_bytes(bytes)?;
            Ok(c)
        }
        _ => bail!("CBOR skip: unsupported major 0x{major:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn parse_cose_es256_realistic_key() {
        // Hand-built COSE_Key ES256 blob. In CBOR:
        //   { 1: 2, 3: -7, -1: 1, -2: <32 bytes>, -3: <32 bytes> }
        let mut blob = Vec::new();
        blob.push(0xA5); // map(5)
        // 1 -> 2
        blob.push(0x01);
        blob.push(0x02);
        // 3 -> -7
        blob.push(0x03);
        blob.push(0x26); // -7 = 0x20 | 6
        // -1 -> 1
        blob.push(0x20); // -1
        blob.push(0x01);
        // -2 -> x (32 bytes of 0xAA)
        blob.push(0x21);
        blob.push(0x58); // bstr with 1-byte length
        blob.push(0x20); // length 32
        blob.extend_from_slice(&[0xAA; 32]);
        // -3 -> y (32 bytes of 0xBB)
        blob.push(0x22);
        blob.push(0x58);
        blob.push(0x20);
        blob.extend_from_slice(&[0xBB; 32]);

        let (x, y) = parse_cose_es256(&blob).unwrap();
        assert_eq!(x, [0xAA; 32]);
        assert_eq!(y, [0xBB; 32]);
    }

    #[test]
    fn parse_cose_rejects_non_ec2_kty() {
        // kty = 3 (RSA) — we don't support it.
        let mut blob = vec![0xA1]; // map(1)
        blob.push(0x01);
        blob.push(0x03);
        let err = parse_cose_es256(&blob).unwrap_err();
        assert!(err.to_string().contains("kty"));
    }

    #[test]
    fn verify_rejects_wrong_type() {
        // clientDataJSON with type "webauthn.create" (registration)
        // must not pass an authentication verification.
        let cdj = serde_json::json!({
            "type": "webauthn.create",
            "challenge": "abc",
            "origin": "https://example.com",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);
        let err = verify_assertion(
            "AA",  // doesn't matter, we fail before key parse
            &cdj_b64,
            "AA",
            "AA",
            "abc",
            "https://example.com",
            "example.com",
        )
        .unwrap_err();
        assert!(err.to_string().contains("webauthn.get"));
    }

    #[test]
    fn verify_rejects_origin_mismatch() {
        let cdj = serde_json::json!({
            "type": "webauthn.get",
            "challenge": "abc",
            "origin": "https://evil.com",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);
        let err = verify_assertion(
            "AA", &cdj_b64, "AA", "AA", "abc", "https://custody.local", "custody.local",
        )
        .unwrap_err();
        assert!(err.to_string().contains("origin mismatch"));
    }

    #[test]
    fn verify_rejects_wrong_rp_id_hash() {
        // Assertion signed for a DIFFERENT rp_id — even with
        // valid challenge + origin, must not authenticate here.
        let cdj = serde_json::json!({
            "type": "webauthn.get",
            "challenge": "abc",
            "origin": "https://custody.local",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);

        // authData with rpIdHash = SHA-256("evil.com") + UP flag set + zero signCount
        let mut auth = Vec::new();
        auth.extend_from_slice(&sha2::Sha256::digest(b"evil.com"));
        auth.push(0x01); // UP flag
        auth.extend_from_slice(&[0u8; 4]); // signCount
        let auth_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&auth);

        let err = verify_assertion(
            "AA", &cdj_b64, &auth_b64, "AA", "abc", "https://custody.local", "custody.local",
        )
        .unwrap_err();
        assert!(err.to_string().contains("rpIdHash mismatch"));
    }

    #[test]
    fn verify_rejects_missing_up_flag() {
        // rpIdHash correct, but UP flag NOT set — authenticator
        // produced the signature without user interaction.
        let cdj = serde_json::json!({
            "type": "webauthn.get",
            "challenge": "abc",
            "origin": "https://custody.local",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);

        let mut auth = Vec::new();
        auth.extend_from_slice(&sha2::Sha256::digest(b"custody.local"));
        auth.push(0x00); // flags = 0 — no UP
        auth.extend_from_slice(&[0u8; 4]);
        let auth_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&auth);

        let err = verify_assertion(
            "AA", &cdj_b64, &auth_b64, "AA", "abc", "https://custody.local", "custody.local",
        )
        .unwrap_err();
        assert!(err.to_string().contains("User Presence"));
    }

    #[test]
    fn verify_rejects_truncated_auth_data() {
        let cdj = serde_json::json!({
            "type": "webauthn.get",
            "challenge": "abc",
            "origin": "https://custody.local",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);

        // Only 10 bytes — well short of the required 37.
        let auth_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&[0u8; 10]);

        let err = verify_assertion(
            "AA", &cdj_b64, &auth_b64, "AA", "abc", "https://custody.local", "custody.local",
        )
        .unwrap_err();
        assert!(err.to_string().contains("authenticatorData too short"));
    }

    #[test]
    fn verify_rejects_challenge_mismatch() {
        let cdj = serde_json::json!({
            "type": "webauthn.get",
            "challenge": "zzz",
            "origin": "https://custody.local",
        });
        let cdj_bytes = serde_json::to_vec(&cdj).unwrap();
        let cdj_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cdj_bytes);
        let err = verify_assertion(
            "AA", &cdj_b64, "AA", "AA", "expected-abc", "https://custody.local", "custody.local",
        )
        .unwrap_err();
        assert!(err.to_string().contains("challenge mismatch"));
    }
}
