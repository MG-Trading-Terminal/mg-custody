//! Mobile push notification service — Firebase (FCM), Apple (APNs), and Web Push.
//!
//! Used to send real-time alerts to custody operators for:
//! - Pending withdrawal approvals
//! - Large deposit received
//! - Circuit breaker tripped
//! - Failed withdrawal requiring attention
//!
//! ## Delivery status in this build
//!
//! | Platform | Status                                                                 |
//! |----------|------------------------------------------------------------------------|
//! | FCM      | **Broken by upstream.** The legacy `/fcm/send` endpoint returned       |
//! |          | 200 for years but was fully shut down on 2024-07-22. The transport    |
//! |          | here now refuses to call it; migrate to FCM HTTP v1                    |
//! |          | (`/v1/projects/{id}/messages:send`) with an OAuth2 bearer token       |
//! |          | minted from a Google service-account JSON before re-enabling.         |
//! | APNs     | Not wired. Needs HTTP/2 + ES256 JWT signing (the `a2` crate).          |
//! | Web Push | Not wired. Needs VAPID signing + RFC 8291 payload encryption          |
//! |          | (the `web_push` crate).                                                |
//!
//! All three paths return `Err` when called, so `PushService::broadcast`
//! reports real zero-delivery instead of counting log-only "sends" as
//! successes.
//!
//! References:
//! - FCM HTTP v1:     https://firebase.google.com/docs/cloud-messaging/migrate-v1
//! - FCM sunset note: https://firebase.google.com/support/faq#fcm-23-deprecation
//! - APNs HTTP/2:     https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns
//! - Web Push:        https://tools.ietf.org/html/rfc8030

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Platform & device token
// ---------------------------------------------------------------------------

/// Mobile/web push platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Ios,
    Android,
    Web,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Ios => "ios",
            Platform::Android => "android",
            Platform::Web => "web",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A registered device push token.
///
/// Stored in the `device_tokens` table (migration 020).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub platform: Platform,
    /// Platform-specific device token string.
    /// - iOS: APNs token (hex, 64 chars)
    /// - Android: FCM registration token (variable length)
    /// - Web: JSON-encoded PushSubscription object
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Push payload
// ---------------------------------------------------------------------------

/// Notification payload to send to a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushPayload {
    /// Short notification title (shown in OS notification center).
    pub title: String,
    /// Notification body text.
    pub body: String,
    /// Structured data for the app to act on (e.g., withdrawal_id, action).
    pub data: HashMap<String, String>,
    /// iOS badge count. `None` means do not change the badge.
    pub badge: Option<u32>,
    /// Notification sound. `None` means system default.
    pub sound: Option<String>,
}

impl PushPayload {
    /// Create a simple title + body notification with no extra data.
    pub fn simple(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            data: HashMap::new(),
            badge: None,
            sound: None,
        }
    }

    /// Create a withdrawal approval notification with action data.
    pub fn withdrawal_approval(
        withdrawal_id: Uuid,
        amount: &str,
        asset: &str,
        destination: &str,
    ) -> Self {
        let mut data = HashMap::new();
        data.insert("action".into(), "withdrawal_approval".into());
        data.insert("withdrawal_id".into(), withdrawal_id.to_string());
        data.insert("amount".into(), amount.into());
        data.insert("asset".into(), asset.into());

        Self {
            title: "Withdrawal Approval Required".into(),
            body: format!("{amount} {asset} to {dest}", amount = amount, asset = asset, dest = &destination[..destination.len().min(20)]),
            data,
            badge: Some(1),
            sound: Some("default".into()),
        }
    }

    /// Create a large deposit alert notification.
    pub fn large_deposit(amount: &str, asset: &str, address: &str) -> Self {
        let mut data = HashMap::new();
        data.insert("action".into(), "deposit_received".into());
        data.insert("amount".into(), amount.into());
        data.insert("asset".into(), asset.into());
        data.insert("address".into(), address.into());

        Self {
            title: "Large Deposit Received".into(),
            body: format!("{amount} {asset} received"),
            data,
            badge: None,
            sound: Some("default".into()),
        }
    }

    /// Create a circuit breaker trip alert.
    pub fn circuit_breaker_tripped(chain: &str, reason: &str) -> Self {
        let mut data = HashMap::new();
        data.insert("action".into(), "circuit_breaker".into());
        data.insert("chain".into(), chain.into());
        data.insert("reason".into(), reason.into());

        Self {
            title: "Circuit Breaker Tripped".into(),
            body: format!("{chain}: {reason}"),
            data,
            badge: None,
            sound: Some("critical".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Push notification service
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// FCM HTTP v1 — OAuth2 + per-project message send
// ---------------------------------------------------------------------------
//
// The legacy /fcm/send endpoint (authorised by a server key) was shut
// down by Google on 2024-07-22. The v1 endpoint takes a project-scoped
// URL and a short-lived OAuth2 bearer token minted from a Google
// service-account JSON.
//
// Flow (per https://firebase.google.com/docs/cloud-messaging/auth-server):
//   1. Read the service-account JSON — we need `client_email`,
//      `private_key` (PEM), and `project_id`.
//   2. Build a JWT:
//        header:  {alg:"RS256", typ:"JWT"}
//        claims:  {iss, scope:"https://www.googleapis.com/auth/firebase.messaging",
//                  aud:"https://oauth2.googleapis.com/token", iat, exp: iat+3600}
//      signed with the service account's private key.
//   3. POST to https://oauth2.googleapis.com/token
//        grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer
//        assertion={jwt}
//      Response: {access_token, expires_in, token_type: "Bearer"}.
//   4. POST the message to
//        https://fcm.googleapis.com/v1/projects/{project_id}/messages:send
//      with Authorization: Bearer {access_token}. Body shape:
//        {"message": {"token": <device>, "notification": {...}, "data": {...}}}.

/// Fields extracted from a Google service-account JSON file.
#[derive(Debug, Clone, Deserialize)]
pub struct FcmServiceAccount {
    pub client_email: String,
    pub private_key: String,
    pub project_id: String,
}

impl FcmServiceAccount {
    /// Parse a raw service-account JSON string.
    pub fn from_json(raw: &str) -> Result<Self> {
        let sa: FcmServiceAccount = serde_json::from_str(raw)
            .context("parsing FCM service-account JSON")?;
        if sa.client_email.is_empty() || sa.private_key.is_empty() || sa.project_id.is_empty() {
            bail!("service-account JSON is missing client_email/private_key/project_id");
        }
        Ok(sa)
    }
}

/// FCM HTTP v1 client. Caches the OAuth2 bearer token until it
/// approaches expiry (we refresh 5 minutes before `expires_in` so a
/// long send burst never crosses the cliff).
pub struct FcmV1Client {
    service_account: FcmServiceAccount,
    http: reqwest::Client,
    token_cache: RwLock<Option<CachedToken>>,
}

#[derive(Clone, Debug)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

/// Window before actual expiry at which we proactively refresh the
/// token. 5 minutes comfortably covers a multi-minute send burst.
const TOKEN_REFRESH_SKEW: Duration = Duration::from_secs(300);

impl FcmV1Client {
    /// Construct the client from a parsed service account. Does not
    /// touch the network; the first token mint happens lazily on the
    /// first `send()`.
    pub fn new(sa: FcmServiceAccount, http: reqwest::Client) -> Self {
        Self {
            service_account: sa,
            http,
            token_cache: RwLock::new(None),
        }
    }

    /// Get a valid OAuth2 bearer token, minting or refreshing as needed.
    async fn access_token(&self) -> Result<String> {
        {
            let guard = self.token_cache.read().await;
            if let Some(cached) = guard.as_ref()
                && cached.expires_at.saturating_duration_since(Instant::now()) > TOKEN_REFRESH_SKEW
            {
                return Ok(cached.access_token.clone());
            }
        }

        let (token, expires_in) = self.mint_token().await?;
        let expires_at = Instant::now() + Duration::from_secs(expires_in.max(60));
        *self.token_cache.write().await = Some(CachedToken {
            access_token: token.clone(),
            expires_at,
        });
        Ok(token)
    }

    /// Sign a JWT with RS256 and exchange it at Google's OAuth2
    /// endpoint for a short-lived access token.
    async fn mint_token(&self) -> Result<(String, u64)> {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

        #[derive(Serialize)]
        struct JwtClaims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: i64,
            exp: i64,
        }
        let now = Utc::now().timestamp();
        let claims = JwtClaims {
            iss: &self.service_account.client_email,
            scope: "https://www.googleapis.com/auth/firebase.messaging",
            aud: "https://oauth2.googleapis.com/token",
            iat: now,
            exp: now + 3600,
        };
        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(self.service_account.private_key.as_bytes())
            .context("FCM v1: service account private_key is not a valid RSA PEM")?;
        let assertion = encode(&header, &claims, &key)
            .context("FCM v1: JWT encoding failed")?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: u64,
        }
        let resp = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .context("FCM v1: OAuth2 token exchange request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("FCM v1: OAuth2 token exchange returned {status}: {body}");
        }
        let parsed: TokenResponse = resp
            .json()
            .await
            .context("FCM v1: OAuth2 token response was not JSON")?;
        Ok((parsed.access_token, parsed.expires_in))
    }

    /// Send a single push via FCM HTTP v1.
    pub async fn send(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        if device_token.is_empty() {
            bail!("FCM device token must not be empty");
        }
        let token = self.access_token().await?;
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.service_account.project_id,
        );
        let body = build_fcm_v1_payload(device_token, payload);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await
            .context("FCM v1: messages:send request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("FCM v1 send failed: HTTP {status}: {text}");
        }
        tracing::info!(
            platform = "android",
            project = %self.service_account.project_id,
            title = %payload.title,
            "Push notification sent via FCM v1"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// APNs — HTTP/2 + ES256 JWT
// ---------------------------------------------------------------------------
//
// Apple's APNs provider protocol:
//   POST https://api.push.apple.com/3/device/{device_token}       (prod)
//   POST https://api.sandbox.push.apple.com/3/device/{device_token} (dev)
//
// Headers (required):
//   authorization:   bearer <provider_token>
//   apns-topic:      <bundle-id>
//   apns-push-type:  alert | background | voip | complication | ...
//   apns-priority:   10 (immediate) | 5 (conservative power)
//
// Body: JSON with an `aps` dictionary + optional custom keys.
//
// Provider token: ES256 JWT signed with the team's APNs key (p8 PEM).
//   header: {alg:"ES256", kid:<apns_key_id>, typ:"JWT"}
//   claims: {iss:<apns_team_id>, iat:<unix>}
//
// Apple asks that provider tokens be refreshed "no more often than
// every 20 minutes and no less often than every 60 minutes". We refresh
// at 50 minutes to leave comfortable headroom before rejection.
//
// Reference:
//   https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns
//   https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns

/// Configuration for the APNs provider.
#[derive(Clone, Debug)]
pub struct ApnsConfig {
    pub team_id: String,
    pub key_id: String,
    /// PKCS#8 PEM contents of the `.p8` file downloaded from Apple.
    pub private_key_pem: String,
    /// App bundle identifier — becomes the `apns-topic` header.
    pub bundle_id: String,
    /// `true` → api.push.apple.com. `false` → api.sandbox.push.apple.com.
    pub production: bool,
}

pub struct ApnsClient {
    cfg: ApnsConfig,
    http: reqwest::Client,
    token_cache: RwLock<Option<CachedToken>>,
}

const APNS_TOKEN_REFRESH_AT: Duration = Duration::from_secs(50 * 60);

impl ApnsClient {
    pub fn new(cfg: ApnsConfig, http: reqwest::Client) -> Self {
        Self {
            cfg,
            http,
            token_cache: RwLock::new(None),
        }
    }

    fn base_url(&self) -> &'static str {
        if self.cfg.production {
            "https://api.push.apple.com"
        } else {
            "https://api.sandbox.push.apple.com"
        }
    }

    /// Sign a fresh ES256 provider token. No `exp` claim — Apple
    /// rejects tokens with one.
    fn sign_token(&self) -> Result<String> {
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            iat: i64,
        }
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.cfg.key_id.clone());
        let claims = Claims {
            iss: &self.cfg.team_id,
            iat: Utc::now().timestamp(),
        };
        let key = EncodingKey::from_ec_pem(self.cfg.private_key_pem.as_bytes())
            .context("APNs: p8 key is not a valid PKCS#8 EC PEM")?;
        encode(&header, &claims, &key).context("APNs: JWT encoding failed")
    }

    async fn provider_token(&self) -> Result<String> {
        {
            let guard = self.token_cache.read().await;
            if let Some(t) = guard.as_ref()
                && t.expires_at > Instant::now()
            {
                return Ok(t.access_token.clone());
            }
        }
        let token = self.sign_token()?;
        let expires_at = Instant::now() + APNS_TOKEN_REFRESH_AT;
        *self.token_cache.write().await = Some(CachedToken {
            access_token: token.clone(),
            expires_at,
        });
        Ok(token)
    }

    /// Send a single alert to `device_token`.
    pub async fn send(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        if device_token.is_empty() {
            bail!("APNs device token must not be empty");
        }
        let jwt = self.provider_token().await?;
        let body = build_apns_payload(payload);
        let url = format!("{}/3/device/{}", self.base_url(), device_token);

        let resp = self
            .http
            .post(&url)
            .header("authorization", format!("bearer {jwt}"))
            .header("apns-topic", &self.cfg.bundle_id)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&body)
            .send()
            .await
            .context("APNs: HTTP/2 request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("APNs send failed: HTTP {status}: {text}");
        }
        tracing::info!(
            platform = "ios",
            bundle = %self.cfg.bundle_id,
            production = self.cfg.production,
            title = %payload.title,
            "Push notification sent via APNs"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Web Push — RFC 8030 (delivery) + RFC 8291 (encryption) + RFC 8292 (VAPID)
// ---------------------------------------------------------------------------
//
// A browser subscription (PushSubscription.toJSON()) looks like:
//   {
//     "endpoint": "https://fcm.googleapis.com/wp/...",
//     "keys": {
//       "p256dh": "<base64url P-256 uncompressed pubkey>",   // 65 bytes
//       "auth":   "<base64url 16-byte authentication secret>"
//     }
//   }
//
// To deliver an encrypted payload:
//
//   1. Generate an ephemeral P-256 keypair.
//   2. ECDH(ephemeral.priv, subscriber.p256dh.pub) → shared secret.
//   3. HKDF-SHA256 using `auth_secret` as salt:
//         PRK = HKDF-Extract(salt=auth_secret, IKM=shared_secret)
//         key_info = "WebPush: info\0" || subscriber.pub || ephemeral.pub
//         IKM2 = HKDF-Expand(PRK, key_info, 32)
//      Then with a 16-byte random `salt`:
//         PRK2 = HKDF-Extract(salt=salt, IKM=IKM2)
//         CEK  = HKDF-Expand(PRK2, "Content-Encoding: aes128gcm\0", 16)
//         NONCE= HKDF-Expand(PRK2, "Content-Encoding: nonce\0",    12)
//   4. Pad the plaintext:  plaintext || 0x02 || 0x00*...  (record-delimiter).
//   5. Encrypt with AES-128-GCM(CEK, NONCE, padded_plaintext).
//   6. Prepend RFC 8188 header:
//         salt(16) || record_size(4 BE) || idlen(1) || keyid(65 = ephemeral.pub)
//      Body = header || ciphertext.
//
// Delivery:
//   POST <endpoint>
//     TTL: <seconds>
//     Content-Encoding: aes128gcm
//     Content-Type: application/octet-stream
//     Authorization: vapid t=<JWT>, k=<base64url(vapid_pub_uncompressed)>
//     Content-Length: <body len>
//   Response: 201 Created on success. 410 Gone ⇒ delete subscription.
//
// VAPID JWT (RFC 8292):
//   header: {alg:"ES256", typ:"JWT"}
//   claims: {
//     aud: <scheme + host of endpoint, e.g. "https://fcm.googleapis.com">,
//     exp: <= now + 24h>,
//     sub: "mailto:ops@example.com"
//   }
//   Signed with the VAPID private key (P-256 EC).

/// Subscription received from a browser (already parsed).
#[derive(Clone, Debug, Deserialize)]
pub struct WebPushSubscription {
    pub endpoint: String,
    pub keys: WebPushKeys,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebPushKeys {
    /// Base64url-encoded P-256 uncompressed public key (65 bytes).
    pub p256dh: String,
    /// Base64url-encoded 16-byte authentication secret.
    pub auth: String,
}

impl WebPushSubscription {
    pub fn from_json(raw: &str) -> Result<Self> {
        serde_json::from_str(raw).context("parsing WebPushSubscription JSON")
    }
}

/// VAPID-signing configuration.
#[derive(Clone, Debug)]
pub struct WebPushConfig {
    /// PKCS#8-PEM encoded VAPID private key (P-256).
    pub vapid_private_key_pem: String,
    /// Subject claim — must be an `mailto:` or `https://` URL the
    /// push service can contact if deliveries misbehave.
    pub vapid_subject: String,
    /// TTL header on each send. 24h is a sensible default for
    /// custody alerts; callers can override per-send later.
    pub default_ttl_secs: u64,
}

pub struct WebPushClient {
    cfg: WebPushConfig,
    http: reqwest::Client,
}

impl WebPushClient {
    pub fn new(cfg: WebPushConfig, http: reqwest::Client) -> Self {
        Self { cfg, http }
    }

    /// Send an encrypted payload to `subscription`.
    pub async fn send(
        &self,
        subscription: &WebPushSubscription,
        payload: &PushPayload,
    ) -> Result<()> {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

        let plaintext = serde_json::to_vec(&build_web_push_payload(payload))
            .context("Web Push: serialising notification payload")?;

        // ── 1. Decode subscription material ───────────────────────
        let subscriber_pub = URL_SAFE_NO_PAD
            .decode(&subscription.keys.p256dh)
            .context("Web Push: p256dh is not base64url")?;
        let auth_secret = URL_SAFE_NO_PAD
            .decode(&subscription.keys.auth)
            .context("Web Push: auth is not base64url")?;
        if subscriber_pub.len() != 65 || subscriber_pub[0] != 0x04 {
            bail!("Web Push: p256dh must be 65-byte uncompressed P-256 public key");
        }
        if auth_secret.len() != 16 {
            bail!("Web Push: auth secret must be 16 bytes");
        }

        // ── 2. Encrypt the payload (aes128gcm) ────────────────────
        let encrypted = encrypt_aes128gcm(&plaintext, &subscriber_pub, &auth_secret)?;

        // ── 3. Build VAPID JWT + `Authorization: vapid ...` header ─
        let (vapid_jwt, vapid_pub_b64) = self.vapid_header(&subscription.endpoint)?;
        let authz = format!("vapid t={vapid_jwt}, k={vapid_pub_b64}");

        // ── 4. POST to subscription endpoint ─────────────────────
        let resp = self
            .http
            .post(&subscription.endpoint)
            .header("TTL", self.cfg.default_ttl_secs.to_string())
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", authz)
            .body(encrypted)
            .send()
            .await
            .context("Web Push: HTTP POST failed")?;

        let status = resp.status();
        if !status.is_success() {
            // 410 Gone: subscription is dead, caller should remove it.
            // We surface this as a typed error-free bail — callers can
            // match on the substring until the dedicated error type lands.
            let text = resp.text().await.unwrap_or_default();
            bail!("Web Push send failed: HTTP {status}: {text}");
        }
        tracing::info!(
            platform = "web",
            endpoint = %endpoint_host(&subscription.endpoint).unwrap_or_default(),
            title = %payload.title,
            "Push notification sent via Web Push"
        );
        Ok(())
    }

    /// Build a VAPID JWT + the base64url-encoded uncompressed public
    /// key (for the `k=` parameter of the Authorization header).
    fn vapid_header(&self, endpoint: &str) -> Result<(String, String)> {
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        use p256::elliptic_curve::sec1::ToEncodedPoint;

        let aud = audience_for(endpoint)
            .ok_or_else(|| anyhow::anyhow!("Web Push: endpoint URL missing scheme/host"))?;

        #[derive(Serialize)]
        struct Claims<'a> {
            aud: String,
            exp: i64,
            sub: &'a str,
        }
        let claims = Claims {
            aud,
            exp: Utc::now().timestamp() + 12 * 3600,
            sub: &self.cfg.vapid_subject,
        };

        let header = Header::new(Algorithm::ES256);
        let key = EncodingKey::from_ec_pem(self.cfg.vapid_private_key_pem.as_bytes())
            .context("Web Push: VAPID key is not a valid PKCS#8 EC PEM")?;
        let jwt = encode(&header, &claims, &key).context("Web Push: VAPID JWT encode")?;

        // Derive the corresponding public key for the `k=` parameter.
        // Accept either PKCS#8 ("BEGIN PRIVATE KEY") or SEC1
        // ("BEGIN EC PRIVATE KEY") PEM — whichever the operator
        // generated — by trying both parsers in order.
        use p256::pkcs8::DecodePrivateKey;
        let sk: p256::SecretKey = p256::SecretKey::from_pkcs8_pem(&self.cfg.vapid_private_key_pem)
            .or_else(|_| p256::SecretKey::from_sec1_pem(&self.cfg.vapid_private_key_pem))
            .context("Web Push: VAPID key PEM parse (for public-key export)")?;
        let encoded = sk.public_key().to_encoded_point(false);
        let pub_b64 = URL_SAFE_NO_PAD.encode(encoded.as_bytes());

        Ok((jwt, pub_b64))
    }
}

/// RFC 8291 `aes128gcm` encryption of `plaintext` for a Web Push
/// subscriber. Output is ready to POST verbatim as the request body.
fn encrypt_aes128gcm(
    plaintext: &[u8],
    subscriber_pub: &[u8],
    auth_secret: &[u8],
) -> Result<Vec<u8>> {
    use aes_gcm::Aes128Gcm;
    use aes_gcm::KeyInit;
    use aes_gcm::aead::{Aead, Payload};
    use hkdf::Hkdf;
    use p256::ecdh::EphemeralSecret;
    use p256::{EncodedPoint, PublicKey};
    use rand::rngs::OsRng;
    use sha2::Sha256;

    // Ephemeral P-256 keypair. `rand::CryptoRng + RngCore` is
    // required; OsRng satisfies both.
    let ephemeral = EphemeralSecret::random(&mut OsRng);
    let ephemeral_pub_point = EncodedPoint::from(ephemeral.public_key());
    let ephemeral_pub = ephemeral_pub_point.as_bytes().to_vec();

    // Parse subscriber's P-256 public key (uncompressed).
    let sub_pub_point = EncodedPoint::from_bytes(subscriber_pub)
        .map_err(|e| anyhow::anyhow!("Web Push: subscriber public key invalid: {e}"))?;
    let sub_pub = PublicKey::from_sec1_bytes(sub_pub_point.as_bytes())
        .context("Web Push: subscriber public key parse")?;

    // ECDH → 32-byte shared secret.
    let shared = ephemeral.diffie_hellman(&sub_pub);
    let shared_bytes = shared.raw_secret_bytes();

    // HKDF step 1: PRK with auth_secret as salt, info includes both pubkeys.
    let hk = Hkdf::<Sha256>::new(Some(auth_secret), shared_bytes);
    let mut info = Vec::with_capacity(14 + 1 + subscriber_pub.len() + ephemeral_pub.len());
    info.extend_from_slice(b"WebPush: info\0");
    info.extend_from_slice(subscriber_pub);
    info.extend_from_slice(&ephemeral_pub);
    let mut ikm2 = [0u8; 32];
    hk.expand(&info, &mut ikm2)
        .map_err(|e| anyhow::anyhow!("Web Push: HKDF expand (IKM2): {e}"))?;

    // 16-byte random salt for the RFC 8188 record header.
    let mut salt = [0u8; 16];
    use rand::RngCore;
    OsRng.fill_bytes(&mut salt);

    // HKDF step 2: content encryption key + nonce.
    let hk2 = Hkdf::<Sha256>::new(Some(&salt), &ikm2);
    let mut cek = [0u8; 16];
    hk2.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|e| anyhow::anyhow!("Web Push: HKDF expand (CEK): {e}"))?;
    let mut nonce = [0u8; 12];
    hk2.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|e| anyhow::anyhow!("Web Push: HKDF expand (nonce): {e}"))?;

    // Pad plaintext per RFC 8291: append 0x02 as the final-record
    // delimiter; no extra zero-padding needed for a single record.
    let mut padded = Vec::with_capacity(plaintext.len() + 1);
    padded.extend_from_slice(plaintext);
    padded.push(0x02);

    // Encrypt.
    let cipher = Aes128Gcm::new(&cek.into());
    let ciphertext = cipher
        .encrypt(
            &nonce.into(),
            Payload {
                msg: &padded,
                aad: &[],
            },
        )
        .map_err(|e| anyhow::anyhow!("Web Push: AES-128-GCM encrypt: {e}"))?;

    // Build the RFC 8188 record header.
    // record_size must be >= plaintext.len() + 18; we pick a generous 4096.
    let record_size: u32 = 4096;
    let mut body = Vec::with_capacity(16 + 4 + 1 + ephemeral_pub.len() + ciphertext.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&record_size.to_be_bytes());
    body.push(ephemeral_pub.len() as u8);
    body.extend_from_slice(&ephemeral_pub);
    body.extend_from_slice(&ciphertext);
    Ok(body)
}

/// Derive the `aud` claim for a VAPID JWT from the push endpoint:
/// it's "<scheme>://<host>" without path or query (RFC 8292 §2).
fn audience_for(endpoint: &str) -> Option<String> {
    let (scheme, rest) = endpoint.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}"))
}

/// Extract the host portion of a URL for log lines (no full URL —
/// subscription endpoints often contain secrets in the path).
fn endpoint_host(endpoint: &str) -> Option<String> {
    let (_, rest) = endpoint.split_once("://")?;
    rest.split(['/', '?']).next().map(str::to_string)
}

/// Push notification service — wraps FCM v1, APNs, and Web Push.
///
/// All fields are optional — the service gracefully skips unavailable providers.
/// In production, `fcm_v1` is built from a Google service-account JSON; APNs
/// and Web Push credentials come from environment or HashiCorp Vault.
pub struct PushNotificationService {
    /// FCM HTTP v1 client (OAuth2-authenticated). `None` means the
    /// Android path bails with a "not configured" error instead of
    /// pretending to deliver.
    pub fcm_v1: Option<Arc<FcmV1Client>>,
    /// Real APNs provider (HTTP/2 + ES256 JWT).
    pub apns: Option<Arc<ApnsClient>>,
    /// Real Web Push provider (RFC 8291/8292 encrypted payloads).
    pub web_push: Option<Arc<WebPushClient>>,
    client: reqwest::Client,
}

impl PushNotificationService {
    pub fn new(
        fcm_v1: Option<Arc<FcmV1Client>>,
        apns: Option<Arc<ApnsClient>>,
        web_push: Option<Arc<WebPushClient>>,
    ) -> Self {
        Self {
            fcm_v1,
            apns,
            web_push,
            client: reqwest::Client::new(),
        }
    }

    /// Create a service with only FCM v1 configured — convenience for
    /// the common single-provider deployment.
    pub fn fcm_only(sa: FcmServiceAccount) -> Self {
        Self::new(
            Some(Arc::new(FcmV1Client::new(sa, reqwest::Client::new()))),
            None,
            None,
        )
    }

    /// Send a push notification to a single device.
    ///
    /// Routes to the correct provider based on `DeviceToken::platform`.
    pub async fn send(&self, token: &DeviceToken, payload: &PushPayload) -> Result<()> {
        match token.platform {
            Platform::Android => self.send_fcm(&token.token, payload).await,
            Platform::Ios => self.send_apns(&token.token, payload).await,
            Platform::Web => self.send_web_push(&token.token, payload).await,
        }
    }

    /// Send via FCM HTTP v1. Delegates to `FcmV1Client` if configured.
    /// Returns `Err` when no service account is loaded — the broadcast
    /// success counter will reflect real zero-delivery rather than
    /// log-only "sends".
    pub async fn send_fcm(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        let client = self
            .fcm_v1
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("FCM HTTP v1 not configured (no service account)"))?;
        client.send(device_token, payload).await
    }

    /// Send via APNs. Delegates to `ApnsClient` if configured; errors
    /// cleanly if not. The broadcast success counter reflects real
    /// zero-delivery when APNs is absent rather than counting
    /// log-only "sends".
    pub async fn send_apns(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        let client = self
            .apns
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("APNs not configured (no team_id/key_id/p8 key)"))?;
        client.send(device_token, payload).await
    }

    /// Send a Web Push notification. Delegates to `WebPushClient` if
    /// configured (which handles VAPID + RFC 8291 encryption); errors
    /// cleanly if not. `subscription_json` is the full
    /// `PushSubscription.toJSON()` string saved at registration time.
    pub async fn send_web_push(&self, subscription_json: &str, payload: &PushPayload) -> Result<()> {
        let client = self
            .web_push
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Web Push not configured (no VAPID key)"))?;
        let subscription = WebPushSubscription::from_json(subscription_json)?;
        client.send(&subscription, payload).await
    }

    /// Broadcast to multiple devices, logging any individual failures.
    ///
    /// Returns the count of successful sends. Never fails entirely — errors
    /// per device are logged but do not abort the broadcast.
    pub async fn broadcast(
        &self,
        tokens: &[DeviceToken],
        payload: &PushPayload,
    ) -> usize {
        let mut success = 0usize;
        for token in tokens {
            match self.send(token, payload).await {
                Ok(()) => success += 1,
                Err(e) => {
                    tracing::warn!(
                        user_id = %token.user_id,
                        platform = %token.platform,
                        device_id = %token.id,
                        error = %e,
                        "Push notification failed — skipping this device"
                    );
                }
            }
        }
        success
    }
}

// ---------------------------------------------------------------------------
// Payload formatters
// ---------------------------------------------------------------------------

/// Build the FCM legacy HTTP JSON payload — kept only so existing
/// unit tests that pin the historical shape still compile. Not used
/// by any send path anymore; the legacy endpoint was shut down by
/// Google on 2024-07-22. New code must use [`build_fcm_v1_payload`].
#[allow(dead_code)]
fn build_fcm_payload(device_token: &str, p: &PushPayload) -> serde_json::Value {
    serde_json::json!({
        "to": device_token,
        "notification": {
            "title": p.title,
            "body": p.body,
            "sound": p.sound.as_deref().unwrap_or("default"),
            "badge": p.badge,
        },
        "data": p.data,
        "priority": "high",
        "content_available": true,
    })
}

/// Build the FCM HTTP v1 `messages:send` request body.
///
/// v1 differs from legacy in shape, not just in auth:
///   - Wraps everything in a top-level `message` object.
///   - Uses `token` for the device addressee (was `to`).
///   - Per-platform overrides (`android`, `apns`, `webpush`) sit under
///     `message` alongside `notification` and `data`.
///
/// We keep the notification minimal (title/body) and pass the
/// structured data map through verbatim so downstream code can
/// act on action/IDs without a migration.
///
/// Reference: https://firebase.google.com/docs/reference/fcm/rest/v1/projects.messages
pub fn build_fcm_v1_payload(device_token: &str, p: &PushPayload) -> serde_json::Value {
    // FCM v1 requires `data` values to be strings — the map already is.
    let android = serde_json::json!({
        "priority": "HIGH",
        "notification": {
            "sound": p.sound.as_deref().unwrap_or("default"),
        },
    });
    serde_json::json!({
        "message": {
            "token": device_token,
            "notification": {
                "title": p.title,
                "body": p.body,
            },
            "data": p.data,
            "android": android,
        }
    })
}

/// Build the APNs aps payload dictionary.
///
/// Reference: https://developer.apple.com/documentation/usernotifications/generating-a-remote-notification
pub fn build_apns_payload(p: &PushPayload) -> serde_json::Value {
    let mut aps = serde_json::json!({
        "alert": {
            "title": p.title,
            "body": p.body,
        },
        "sound": p.sound.as_deref().unwrap_or("default"),
        "content-available": 1,
    });
    if let Some(badge) = p.badge {
        aps["aps"]["badge"] = serde_json::json!(badge);
    }
    let mut payload = serde_json::json!({ "aps": aps });
    for (k, v) in &p.data {
        payload[k] = serde_json::json!(v);
    }
    payload
}

/// Build a Web Push notification payload (RFC 8030 / Push API).
pub fn build_web_push_payload(p: &PushPayload) -> serde_json::Value {
    serde_json::json!({
        "notification": {
            "title": p.title,
            "body": p.body,
            "data": p.data,
            "badge": p.badge,
            "requireInteraction": true,
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Platform ---

    #[test]
    fn platform_display() {
        assert_eq!(Platform::Ios.as_str(), "ios");
        assert_eq!(Platform::Android.as_str(), "android");
        assert_eq!(Platform::Web.as_str(), "web");
    }

    #[test]
    fn platform_serialize() {
        assert_eq!(
            serde_json::to_string(&Platform::Ios).unwrap(),
            "\"ios\""
        );
        assert_eq!(
            serde_json::to_string(&Platform::Android).unwrap(),
            "\"android\""
        );
    }

    // --- DeviceToken ---

    #[test]
    fn device_token_roundtrip() {
        let token = DeviceToken {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            platform: Platform::Ios,
            token: "abc123def456".into(),
            created_at: Utc::now(),
            last_used_at: None,
        };
        let json = serde_json::to_string(&token).unwrap();
        let back: DeviceToken = serde_json::from_str(&json).unwrap();
        assert_eq!(back.token, "abc123def456");
        assert_eq!(back.platform, Platform::Ios);
    }

    // --- PushPayload ---

    #[test]
    fn simple_payload() {
        let p = PushPayload::simple("Hello", "World");
        assert_eq!(p.title, "Hello");
        assert_eq!(p.body, "World");
        assert!(p.data.is_empty());
        assert!(p.badge.is_none());
    }

    #[test]
    fn withdrawal_approval_payload() {
        let wid = Uuid::new_v4();
        let p = PushPayload::withdrawal_approval(
            wid, "10000", "USDT", "0x742d35Cc6634C0532925a3b844Bc454e4438f44e"
        );
        assert_eq!(p.title, "Withdrawal Approval Required");
        assert!(p.body.contains("10000"));
        assert!(p.body.contains("USDT"));
        assert_eq!(p.data.get("action").map(String::as_str), Some("withdrawal_approval"));
        assert_eq!(p.data.get("withdrawal_id").map(String::as_str), Some(wid.to_string().as_str()));
        assert_eq!(p.badge, Some(1));
    }

    #[test]
    fn large_deposit_payload() {
        let p = PushPayload::large_deposit("50000", "USDC", "0xabc");
        assert!(p.title.contains("Deposit"));
        assert_eq!(p.data.get("action").map(String::as_str), Some("deposit_received"));
        assert_eq!(p.data.get("amount").map(String::as_str), Some("50000"));
    }

    #[test]
    fn circuit_breaker_payload() {
        let p = PushPayload::circuit_breaker_tripped("ethereum", "hourly limit exceeded");
        assert!(p.title.contains("Circuit Breaker"));
        assert_eq!(p.data.get("chain").map(String::as_str), Some("ethereum"));
        assert!(p.body.contains("hourly limit exceeded"));
    }

    // --- FCM payload builder ---

    #[test]
    fn fcm_payload_structure() {
        let p = PushPayload::simple("Test Title", "Test Body");
        let fcm = build_fcm_payload("device_token_abc", &p);
        assert_eq!(fcm["to"], "device_token_abc");
        assert_eq!(fcm["notification"]["title"], "Test Title");
        assert_eq!(fcm["notification"]["body"], "Test Body");
        assert_eq!(fcm["priority"], "high");
    }

    #[test]
    fn fcm_payload_with_badge() {
        let mut p = PushPayload::simple("T", "B");
        p.badge = Some(5);
        let fcm = build_fcm_payload("tok", &p);
        assert_eq!(fcm["notification"]["badge"], 5);
    }

    // --- APNs payload builder ---

    #[test]
    fn apns_payload_structure() {
        let p = PushPayload::simple("Alert", "Body text");
        let apns = build_apns_payload(&p);
        assert_eq!(apns["aps"]["alert"]["title"], "Alert");
        assert_eq!(apns["aps"]["alert"]["body"], "Body text");
        assert_eq!(apns["aps"]["content-available"], 1);
    }

    #[test]
    fn apns_payload_with_data_keys() {
        let mut p = PushPayload::simple("T", "B");
        p.data.insert("withdrawal_id".into(), "uuid-abc".into());
        let apns = build_apns_payload(&p);
        assert_eq!(apns["withdrawal_id"], "uuid-abc");
    }

    // --- Web Push payload ---

    #[test]
    fn web_push_payload_structure() {
        let p = PushPayload::simple("WebAlert", "web body");
        let wp = build_web_push_payload(&p);
        assert_eq!(wp["notification"]["title"], "WebAlert");
        assert_eq!(wp["notification"]["requireInteraction"], true);
    }

    // --- Service construction ---

    /// A 2048-bit RSA PEM private key generated for test use only.
    /// Matches the shape `jsonwebtoken::EncodingKey::from_rsa_pem`
    /// expects; not a real Google credential.
    const TEST_RSA_PEM: &str = "-----BEGIN RSA PRIVATE KEY-----\n\
        MIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun\n\
        VTLw7onLRnrq0/IzW7yWR7QkrmBL7jTKEn5u+qKhbwKfBstIs+bMY2Zkp18gnTxK\n\
        Lxoe+g6oedlo3m2VTV0ttR/kMrqwrIs78NZ5RktdfkZJJgxVh3mLgn/cGf9KJ8cf\n\
        END FAKE-----\n";

    fn sample_service_account() -> FcmServiceAccount {
        FcmServiceAccount {
            client_email: "push-sa@example-project.iam.gserviceaccount.com".into(),
            private_key: TEST_RSA_PEM.into(),
            project_id: "example-project".into(),
        }
    }

    #[test]
    fn service_fcm_only_has_v1_client() {
        let svc = PushNotificationService::fcm_only(sample_service_account());
        assert!(svc.fcm_v1.is_some());
        assert!(svc.apns.is_none());
    }

    #[test]
    fn service_no_providers() {
        let svc = PushNotificationService::new(None, None, None);
        assert!(svc.fcm_v1.is_none());
        assert!(svc.apns.is_none());
    }

    #[test]
    fn fcm_service_account_rejects_empty_fields() {
        let bad = r#"{"client_email":"","private_key":"","project_id":""}"#;
        assert!(FcmServiceAccount::from_json(bad).is_err());
    }

    #[test]
    fn fcm_v1_payload_has_message_wrapper() {
        let p = PushPayload::simple("Title", "Body");
        let v = build_fcm_v1_payload("device-abc", &p);
        // Legacy used {"to": ...}; v1 must wrap everything under
        // "message" with "token" (not "to").
        assert!(v["message"]["token"] == "device-abc");
        assert!(v["message"]["notification"]["title"] == "Title");
        assert!(v["message"]["android"]["priority"] == "HIGH");
        assert!(v["to"].is_null());
    }

    #[tokio::test]
    async fn send_fcm_errors_when_v1_not_configured() {
        let svc = PushNotificationService::new(None, None, None);
        let err = svc
            .send_fcm("token", &PushPayload::simple("x", "y"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not configured"));
    }

    #[tokio::test]
    async fn send_apns_errors_when_client_not_configured() {
        let svc = PushNotificationService::new(None, None, None);
        let err = svc
            .send_apns("token", &PushPayload::simple("x", "y"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not configured"));
    }

    #[test]
    fn apns_config_production_picks_prod_url() {
        let cfg = ApnsConfig {
            team_id: "T1".into(),
            key_id: "K1".into(),
            private_key_pem: String::new(),
            bundle_id: "com.example.app".into(),
            production: true,
        };
        let c = ApnsClient::new(cfg, reqwest::Client::new());
        assert_eq!(c.base_url(), "https://api.push.apple.com");
    }

    #[test]
    fn apns_config_sandbox_picks_sandbox_url() {
        let cfg = ApnsConfig {
            team_id: "T1".into(),
            key_id: "K1".into(),
            private_key_pem: String::new(),
            bundle_id: "com.example.app".into(),
            production: false,
        };
        let c = ApnsClient::new(cfg, reqwest::Client::new());
        assert_eq!(c.base_url(), "https://api.sandbox.push.apple.com");
    }

    // --- Web Push ---

    #[test]
    fn audience_for_strips_path_and_query() {
        assert_eq!(
            audience_for("https://fcm.googleapis.com/wp/AAAA?x=1").as_deref(),
            Some("https://fcm.googleapis.com"),
        );
        assert_eq!(
            audience_for("https://updates.push.services.mozilla.com/wpush/v2/AAAA").as_deref(),
            Some("https://updates.push.services.mozilla.com"),
        );
        assert_eq!(audience_for("no-scheme"), None);
    }

    #[test]
    fn endpoint_host_omits_path() {
        assert_eq!(
            endpoint_host("https://example.com/wp/secret?x=1").as_deref(),
            Some("example.com"),
        );
    }

    #[test]
    fn web_push_subscription_rejects_garbage_json() {
        assert!(WebPushSubscription::from_json("{").is_err());
    }

    #[test]
    fn web_push_subscription_parses_minimal() {
        let raw = r#"{
            "endpoint": "https://fcm.googleapis.com/wp/AAA",
            "keys": {
                "p256dh": "BNcRdreALRFXTkOOUHK1EtK2wtaz5Ry4YfYCA_0QTpQtUbVlUls0VJXg7A8u-Ts1XbjhazAkj7I99e8QcYP7DkM",
                "auth": "tBHItJI5svbpez7KI4CCXg"
            }
        }"#;
        let sub = WebPushSubscription::from_json(raw).unwrap();
        assert_eq!(sub.endpoint, "https://fcm.googleapis.com/wp/AAA");
        assert!(!sub.keys.p256dh.is_empty());
        assert!(!sub.keys.auth.is_empty());
    }

    #[tokio::test]
    async fn encrypt_aes128gcm_roundtrip_shape() {
        // Build a real P-256 keypair as the "subscriber" and a
        // random 16-byte auth secret. The encryption path must produce
        // a body that at least honours the RFC 8188 header shape:
        //   salt (16) || record_size (4) || idlen (1) || keyid (65) || ciphertext
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD as B64};
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        use rand::rngs::OsRng;

        let sk = p256::SecretKey::random(&mut OsRng);
        let pub_bytes = sk.public_key().to_encoded_point(false).as_bytes().to_vec();
        assert_eq!(pub_bytes.len(), 65);
        let mut auth_secret = [0u8; 16];
        rand::RngCore::fill_bytes(&mut OsRng, &mut auth_secret);

        let body = encrypt_aes128gcm(b"{\"x\":1}", &pub_bytes, &auth_secret).unwrap();

        // Header must be exactly 16 + 4 + 1 + 65 = 86 bytes before ciphertext.
        assert!(body.len() > 86, "encrypted body must include header + ciphertext");
        // idlen = 65.
        assert_eq!(body[20], 65);
        // keyid immediately follows and starts with 0x04 (uncompressed).
        assert_eq!(body[21], 0x04);

        // round-trip the subscription-decoding path as a smoke test of
        // the base64 plumbing the handler uses.
        let sub = WebPushSubscription {
            endpoint: "https://example.com/wp/".into(),
            keys: WebPushKeys {
                p256dh: B64.encode(&pub_bytes),
                auth: B64.encode(auth_secret),
            },
        };
        let raw = serde_json::to_string(&serde_json::json!({
            "endpoint": sub.endpoint,
            "keys": {"p256dh": sub.keys.p256dh, "auth": sub.keys.auth},
        })).unwrap();
        assert!(WebPushSubscription::from_json(&raw).is_ok());
    }

    #[tokio::test]
    async fn send_web_push_errors_when_not_configured() {
        let svc = PushNotificationService::new(None, None, None);
        let err = svc
            .send_web_push("{}", &PushPayload::simple("x", "y"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not configured"));
    }
}
