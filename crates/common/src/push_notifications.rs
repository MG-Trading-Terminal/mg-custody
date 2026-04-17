//! Mobile push notification service — Firebase (FCM), Apple (APNs), and Web Push.
//!
//! Used to send real-time alerts to custody operators for:
//! - Pending withdrawal approvals
//! - Large deposit received
//! - Circuit breaker tripped
//! - Failed withdrawal requiring attention
//!
//! FCM: HTTP POST to https://fcm.googleapis.com/fcm/send (legacy V1 API)
//! APNs: HTTP/2 to api.push.apple.com — stubbed here, real impl needs `h2` crate
//! Web Push: RFC 8030 — stubbed here, real impl needs VAPID signing
//!
//! References:
//! - FCM legacy HTTP: https://firebase.google.com/docs/cloud-messaging/http-server-ref
//! - APNs HTTP/2: https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns
//! - Web Push RFC 8030: https://tools.ietf.org/html/rfc8030

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// Push notification service — wraps FCM, APNs, and Web Push.
///
/// All fields are optional — the service gracefully skips unavailable providers.
/// In production, FCM_SERVER_KEY and APNS credentials come from environment
/// or HashiCorp Vault.
pub struct PushNotificationService {
    /// Firebase Cloud Messaging server key (legacy HTTP API).
    pub fcm_server_key: Option<String>,
    /// APNs key ID (used to sign JWT for HTTP/2 APNs requests).
    pub apns_key_id: Option<String>,
    /// APNs team ID.
    pub apns_team_id: Option<String>,
    client: reqwest::Client,
}

impl PushNotificationService {
    pub fn new(
        fcm_server_key: Option<String>,
        apns_key_id: Option<String>,
        apns_team_id: Option<String>,
    ) -> Self {
        Self {
            fcm_server_key,
            apns_key_id,
            apns_team_id,
            client: reqwest::Client::new(),
        }
    }

    /// Create a service with only FCM configured.
    pub fn fcm_only(server_key: impl Into<String>) -> Self {
        Self::new(Some(server_key.into()), None, None)
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

    /// Send via FCM (Firebase Cloud Messaging) — for Android and background Web.
    ///
    /// POST to https://fcm.googleapis.com/fcm/send
    /// Authorization: key={FCM_SERVER_KEY}
    pub async fn send_fcm(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        let key = self
            .fcm_server_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("FCM server key not configured"))?;

        if device_token.is_empty() {
            bail!("FCM device token must not be empty");
        }

        let body = build_fcm_payload(device_token, payload);

        let resp = self
            .client
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("FCM HTTP request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("FCM send failed: HTTP {status}: {text}");
        }

        tracing::info!(
            platform = "android",
            title = %payload.title,
            "Push notification sent via FCM"
        );

        Ok(())
    }

    /// Send via APNs (Apple Push Notification Service).
    ///
    /// **Not wired.** Real APNs requires HTTP/2 (via `h2` or the `a2`
    /// crate) plus JWT (p8) signing with the team's APNs key. None
    /// of that is in the workspace. The prior implementation built
    /// the payload, logged it, and returned `Ok(())` — callers read
    /// that as a successful delivery and the broadcast success
    /// counter was always == total devices, lying about reach.
    ///
    /// Returns `Err(NotImplemented)`-shaped error instead so the
    /// caller can see zero-delivery and surface "APNs not wired"
    /// instead of claiming the alert reached the iPhone.
    pub async fn send_apns(&self, device_token: &str, payload: &PushPayload) -> Result<()> {
        if device_token.is_empty() {
            bail!("APNs device token must not be empty");
        }

        let key_id = self.apns_key_id.as_deref().unwrap_or("<unset>");
        let apns_payload = build_apns_payload(payload);

        tracing::warn!(
            platform = "ios",
            device_token_prefix = &device_token[..device_token.len().min(8)],
            key_id = %key_id,
            title = %payload.title,
            payload = ?apns_payload,
            "APNs push not dispatched — HTTP/2 + JWT signing not wired"
        );

        // When the a2 crate is added, replace this bail with:
        //   a2::Client::new(key, key_id, team_id, apns_url)
        //     .send(Notification { device_token, payload })
        //     .await?;
        bail!(
            "APNs delivery not implemented in this build. \
             Attach the `a2` crate and wire JWT signing before relying on iOS alerts."
        )
    }

    /// Send via Web Push (RFC 8030).
    ///
    /// **Not wired.** Real Web Push needs VAPID signing (ECDSA over
    /// P-256) plus per-message payload encryption (RFC 8291). The
    /// `web_push` crate covers both but isn't in the workspace. The
    /// prior implementation logged the payload and returned
    /// `Ok(())`, which was indistinguishable from a real dispatch.
    /// Returns `Err` so broadcast success counters don't lie.
    pub async fn send_web_push(&self, subscription_json: &str, payload: &PushPayload) -> Result<()> {
        if subscription_json.is_empty() {
            bail!("Web Push subscription JSON must not be empty");
        }

        let web_payload = build_web_push_payload(payload);

        tracing::warn!(
            platform = "web",
            title = %payload.title,
            payload = ?web_payload,
            "Web Push not dispatched — VAPID signing + payload encryption not wired"
        );

        bail!(
            "Web Push delivery not implemented in this build. \
             Attach the `web_push` crate and a VAPID key before relying on browser alerts."
        )
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

/// Build the FCM legacy HTTP JSON payload.
///
/// Reference: https://firebase.google.com/docs/cloud-messaging/http-server-ref
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

    #[test]
    fn service_fcm_only() {
        let svc = PushNotificationService::fcm_only("test-server-key");
        assert!(svc.fcm_server_key.is_some());
        assert!(svc.apns_key_id.is_none());
    }

    #[test]
    fn service_no_providers() {
        let svc = PushNotificationService::new(None, None, None);
        assert!(svc.fcm_server_key.is_none());
        assert!(svc.apns_key_id.is_none());
    }
}
