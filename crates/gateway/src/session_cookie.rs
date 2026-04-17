//! Session-token cookie helpers.
//!
//! We ship the session token in an `HttpOnly; Secure; SameSite=Strict`
//! cookie so a reflected-XSS on the dashboard cannot exfiltrate it.
//! The `Authorization: Bearer` header path is still accepted by the
//! middleware so existing tests and service-to-service callers (HMAC
//! is unaffected) keep working during the SPA migration.
//!
//! # Why hand-rolled
//! `axum-extra::cookie` / `tower-cookies` would work, but they pull in
//! `cookie` + `time` and we only need two helpers: emit a header and
//! parse a `Cookie:` value. RFC 6265 cookie syntax is narrow enough
//! that a ~30-line parser is easier to audit than a transitive dep.
//!
//! # Configuration
//! - `CUSTODY_COOKIE__SECURE` (`true` by default) — set `Secure` flag.
//!   Flip to `false` only for local HTTP dev; browsers reject
//!   `Secure` cookies over `http://` except for `localhost`.
//! - `CUSTODY_COOKIE__DOMAIN` — optional `Domain=` attribute. Leave
//!   unset for host-only cookies (safer default).

/// Name of the cookie carrying the session token.
///
/// Prefix `__Host-` would be stricter (forces Secure, forbids Domain),
/// but breaks the local HTTP dev workflow. We keep the plain name and
/// lean on `SameSite=Strict` + `HttpOnly` instead.
pub const COOKIE_NAME: &str = "custody_session";

/// Lifetime of the cookie in seconds. Matches the DB session TTL
/// (8h) so the browser discards the cookie around the same time the
/// server would refuse it.
pub const COOKIE_MAX_AGE_SECS: u64 = 8 * 3600;

/// Build a `Set-Cookie` header value that installs the session token.
pub fn build_set_cookie(token: &str) -> String {
    let secure = cookie_secure_flag();
    let domain = std::env::var("CUSTODY_COOKIE__DOMAIN").ok();
    let mut attrs = format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}",
        name = COOKIE_NAME,
        value = token,
        max_age = COOKIE_MAX_AGE_SECS,
    );
    if secure {
        attrs.push_str("; Secure");
    }
    if let Some(d) = domain {
        attrs.push_str(&format!("; Domain={d}"));
    }
    attrs
}

/// Build a `Set-Cookie` header value that clears the session cookie.
/// Used by the logout endpoint so the browser drops the token even if
/// the DB-side revocation races or the pool is momentarily unreachable.
pub fn build_clear_cookie() -> String {
    let secure = cookie_secure_flag();
    let domain = std::env::var("CUSTODY_COOKIE__DOMAIN").ok();
    let mut attrs = format!(
        "{name}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        name = COOKIE_NAME,
    );
    if secure {
        attrs.push_str("; Secure");
    }
    if let Some(d) = domain {
        attrs.push_str(&format!("; Domain={d}"));
    }
    attrs
}

/// Extract the session token from a raw `Cookie:` header value.
///
/// Handles the common cases without pulling in a full RFC 6265 parser:
/// `name=value; other=value2` with whitespace tolerance. Cookie values
/// are taken verbatim — no percent-decoding, which matches the way the
/// middleware compares the token against the SHA-256 hash in the DB.
pub fn extract_session_token(cookie_header: &str) -> Option<&str> {
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(COOKIE_NAME)
            && let Some(value) = rest.strip_prefix('=')
            && !value.is_empty()
        {
            return Some(value);
        }
    }
    None
}

fn cookie_secure_flag() -> bool {
    std::env::var("CUSTODY_COOKIE__SECURE")
        .map(|v| !matches!(v.as_str(), "0" | "false" | "no"))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_cookie_contains_hardening_flags() {
        // SAFETY: test is single-threaded; CUSTODY_COOKIE__SECURE env manipulated only here.
        unsafe { std::env::set_var("CUSTODY_COOKIE__SECURE", "true") };
        let v = build_set_cookie("opaque_token_abc");
        assert!(v.contains("custody_session=opaque_token_abc"));
        assert!(v.contains("HttpOnly"));
        assert!(v.contains("SameSite=Strict"));
        assert!(v.contains("Secure"));
        assert!(v.contains("Max-Age=28800"));
        assert!(v.contains("Path=/"));
    }

    #[test]
    fn clear_cookie_has_zero_max_age() {
        unsafe { std::env::set_var("CUSTODY_COOKIE__SECURE", "true") };
        let v = build_clear_cookie();
        assert!(v.contains("Max-Age=0"));
        assert!(v.contains("HttpOnly"));
    }

    #[test]
    fn secure_flag_respects_env_override() {
        unsafe { std::env::set_var("CUSTODY_COOKIE__SECURE", "false") };
        let v = build_set_cookie("x");
        assert!(!v.contains("Secure"));
        unsafe { std::env::remove_var("CUSTODY_COOKIE__SECURE") };
    }

    #[test]
    fn extract_reads_single_cookie() {
        assert_eq!(
            extract_session_token("custody_session=abc123"),
            Some("abc123")
        );
    }

    #[test]
    fn extract_reads_amid_other_cookies() {
        let header = "locale=en; custody_session=xyz789; theme=dark";
        assert_eq!(extract_session_token(header), Some("xyz789"));
    }

    #[test]
    fn extract_tolerates_whitespace() {
        // Whole pair is trimmed before parsing, so leading/trailing
        // whitespace around the pair is stripped; internal whitespace
        // (none expected in a session token) is preserved.
        let header = "  custody_session=tok ; other=v";
        assert_eq!(extract_session_token(header), Some("tok"));
    }

    #[test]
    fn extract_returns_none_when_missing() {
        assert_eq!(extract_session_token("theme=dark; locale=en"), None);
        assert_eq!(extract_session_token(""), None);
    }

    #[test]
    fn extract_returns_none_for_empty_value() {
        assert_eq!(extract_session_token("custody_session="), None);
    }

    #[test]
    fn extract_does_not_match_prefix_collision() {
        // `custody_session_backup=foo` must not be mistaken for the
        // real cookie. The strip-prefix + '=' check below guards this.
        assert_eq!(
            extract_session_token("custody_session_backup=foo"),
            None
        );
    }
}
