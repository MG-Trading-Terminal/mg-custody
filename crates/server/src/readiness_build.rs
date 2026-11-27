//! Feature readiness probe driver.
//!
//! One async function that enumerates every custody subsystem, runs
//! its cold-start probe, and returns a `ReadinessReport`. Called
//! twice:
//!
//!   - once at boot (main.rs) — so the startup log table is honest
//!     and the first request sees live state, not an "optimistic
//!     probably-works" default.
//!   - again every ~60s from a background task (main.rs spawns it)
//!     so a vendor going down mid-uptime flips the handler gates
//!     from Ready to Degraded without requiring a service restart.
//!
//! Probes here are the same ones called inline during the earlier
//! one-shot version — pulled out verbatim so the first refresh
//! produces identical output to the previous boot behaviour.

use custody_common::config::CustodyConfig;
use custody_common::email::EmailSender;
use custody_common::readiness::{FeatureState, FeatureStatus, ReadinessReport};
use custody_compliance::travel_rule::TwentyOneTravelConfig;

/// Build a fresh `ReadinessReport` by re-running every probe.
pub async fn build(
    config: &CustodyConfig,
    email_sender: &EmailSender,
    travel_rule_configured: bool,
    trust: Option<&TwentyOneTravelConfig>,
) -> ReadinessReport {
    // Record wall-clock duration in a Prometheus histogram so drifting
    // vendor latency surfaces on the dashboard even when every probe
    // still passes.
    let _timer = custody_common::prometheus_metrics::READINESS_BUILD_DURATION.start_timer();
    let mut readiness = ReadinessReport::new();

    // Email transport. For smtp mode we do a full EHLO + (STARTTLS or
    // implicit TLS) + AUTH handshake via lettre so misconfigured
    // credentials, port/TLS mismatches, and dead relays all surface
    // as Degraded.
    match email_sender {
        EmailSender::Smtp(cfg) => match cfg.probe_connection().await {
            Ok(()) => readiness.push(FeatureStatus::ready(
                "email.smtp",
                "Outbound email via lettre SMTP (EHLO+TLS+AUTH verified)",
            )),
            Err(e) => readiness.push(FeatureStatus::degraded(
                "email.smtp",
                "Outbound email via lettre SMTP",
                e,
            )),
        },
        EmailSender::Console(_) => readiness.push(FeatureStatus::disabled(
            "email.smtp",
            "Outbound email — currently printing to stdout (dev transport)",
        )),
        EmailSender::Noop => readiness.push(FeatureStatus::disabled(
            "email.smtp",
            "Outbound email (noop transport)",
        )),
    }

    // HSM signing backend. We intentionally skip the PIN login so a
    // wrong probe never burns an auth attempt against the HSM's
    // lockout counter.
    {
        let lib_path = std::env::var("HSM_LIB_PATH").ok().filter(|s| !s.is_empty());
        match lib_path {
            Some(path) => {
                let slot_id: u64 = std::env::var("HSM_SLOT_ID")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                match custody_signing::probe_pkcs11(&path, slot_id) {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "signing.hsm",
                        "PKCS#11 HSM signing (ETH/ERC-20/Tron/Solana)",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "signing.hsm",
                        "PKCS#11 HSM signing",
                        e,
                    )),
                }
            }
            None => readiness.push(FeatureStatus::disabled(
                "signing.hsm",
                "PKCS#11 HSM signing. Set HSM_LIB_PATH + HSM_PIN to enable.",
            )),
        }
    }

    // AML screening. A key alone doesn't mean the vendor is reachable.
    {
        let chainalysis = std::env::var("CUSTODY_CHAINALYSIS_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let elliptic = std::env::var("CUSTODY_ELLIPTIC_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        match (chainalysis.as_ref(), elliptic.as_ref()) {
            (None, None) => readiness.push(FeatureStatus::disabled(
                "screening.aml",
                "AML screening. Set CUSTODY_CHAINALYSIS_API_KEY or CUSTODY_ELLIPTIC_API_KEY to enable.",
            )),
            _ => {
                let mut problems: Vec<String> = Vec::new();
                if chainalysis.is_some() {
                    if let Err(e) = custody_common::readiness::probe_http_reachable(
                        "https://api.chainalysis.com/api/kyt/v2/users",
                    )
                    .await
                    {
                        problems.push(format!("chainalysis: {e}"));
                    }
                }
                if elliptic.is_some() {
                    if let Err(e) = custody_common::readiness::probe_http_reachable(
                        "https://api.elliptic.co/v2",
                    )
                    .await
                    {
                        problems.push(format!("elliptic: {e}"));
                    }
                }
                if problems.is_empty() {
                    readiness.push(FeatureStatus::ready(
                        "screening.aml",
                        "AML screening (Chainalysis / Elliptic) on every withdrawal",
                    ));
                } else {
                    readiness.push(FeatureStatus::degraded(
                        "screening.aml",
                        "AML screening (Chainalysis / Elliptic)",
                        problems.join("; "),
                    ));
                }
            }
        }
    }

    // Push notifications — parse-only probes.
    {
        let fcm = std::env::var("CUSTODY_FCM__SERVICE_ACCOUNT_JSON")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let apns_keys = [
            "CUSTODY_APNS__TEAM_ID",
            "CUSTODY_APNS__KEY_ID",
            "CUSTODY_APNS__KEY_P8",
            "CUSTODY_APNS__BUNDLE_ID",
        ];
        let apns_present: Vec<&str> = apns_keys
            .iter()
            .filter(|k| std::env::var(k).is_ok_and(|v| !v.is_empty()))
            .copied()
            .collect();
        let apns_missing: Vec<String> = apns_keys
            .iter()
            .filter(|k| !std::env::var(k).is_ok_and(|v| !v.is_empty()))
            .map(|s| (*s).to_string())
            .collect();
        let vapid_key = std::env::var("CUSTODY_VAPID__PRIVATE_KEY_PEM")
            .ok()
            .filter(|s| !s.is_empty());
        let vapid_sub = std::env::var("CUSTODY_VAPID__SUBJECT")
            .ok()
            .filter(|s| !s.is_empty());

        readiness.push(match fcm.as_deref() {
            Some(raw) => match custody_common::push_notifications::probe_fcm_service_account(raw) {
                Ok(()) => FeatureStatus::ready(
                    "push.fcm",
                    "FCM HTTP v1 (Android + background Web)",
                ),
                Err(e) => FeatureStatus::degraded("push.fcm", "FCM HTTP v1", e),
            },
            None => FeatureStatus::disabled(
                "push.fcm",
                "FCM push. Set CUSTODY_FCM__SERVICE_ACCOUNT_JSON to enable.",
            ),
        });

        if apns_present.len() == apns_keys.len() {
            let key_pem = std::env::var("CUSTODY_APNS__KEY_P8").unwrap_or_default();
            match custody_common::push_notifications::probe_apns_private_key(&key_pem) {
                Ok(()) => readiness.push(FeatureStatus::ready(
                    "push.apns",
                    "APNs (iOS) HTTP/2 + ES256 JWT",
                )),
                Err(e) => readiness.push(FeatureStatus::degraded(
                    "push.apns",
                    "APNs (iOS)",
                    e,
                )),
            }
        } else if apns_present.is_empty() {
            readiness.push(FeatureStatus::disabled(
                "push.apns",
                "APNs. Set all 4 CUSTODY_APNS__* env vars to enable.",
            ));
        } else {
            readiness.push(FeatureStatus::misconfigured(
                "push.apns",
                "APNs (partial config)",
                apns_missing,
            ));
        }

        readiness.push(match (vapid_key.as_deref(), vapid_sub) {
            (Some(pem), Some(_)) => {
                match custody_common::push_notifications::probe_vapid_private_key(pem) {
                    Ok(()) => FeatureStatus::ready("push.web", "Web Push (RFC 8291 + VAPID)"),
                    Err(e) => FeatureStatus::degraded("push.web", "Web Push (RFC 8291 + VAPID)", e),
                }
            }
            (None, None) => FeatureStatus::disabled(
                "push.web",
                "Web Push. Set CUSTODY_VAPID__PRIVATE_KEY_PEM + CUSTODY_VAPID__SUBJECT.",
            ),
            (key, sub) => FeatureStatus::misconfigured(
                "push.web",
                "Web Push (partial config)",
                [
                    (key.is_none()).then(|| "CUSTODY_VAPID__PRIVATE_KEY_PEM".to_string()),
                    (sub.is_none()).then(|| "CUSTODY_VAPID__SUBJECT".to_string()),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
        });
    }

    // Travel Rule — same shape as AML screening. A key being set is
    // not the same as the vendor being reachable; probe the Notabene
    // base URL so a DNS outage or expired contract surfaces as
    // Degraded before the first real counterparty dispatch.
    if travel_rule_configured {
        match custody_common::readiness::probe_http_reachable("https://api.notabene.id/tf/v1").await
        {
            Ok(()) => readiness.push(FeatureStatus::ready(
                "travel_rule.notabene",
                "Travel Rule dispatch via Notabene",
            )),
            Err(e) => readiness.push(FeatureStatus::degraded(
                "travel_rule.notabene",
                "Travel Rule dispatch via Notabene",
                e,
            )),
        }
    } else {
        readiness.push(FeatureStatus::disabled(
            "travel_rule.notabene",
            "Automatic Notabene Travel Rule dispatch is disabled.",
        ));
    }

    if let Some(trust) = trust {
        let probe = async { trust.connect().await?.check_routing().await }.await;
        let feature = "travel_rule.21travel";
        let description = "21 Travel Rule TRUST dispatch";
        match probe {
            Ok(()) => readiness.push(FeatureStatus::ready(feature, description)),
            Err(e) => readiness.push(FeatureStatus::degraded(feature, description, e.to_string())),
        }
    }

    // MPC multi-host receiver.
    if std::env::var("CUSTODY_MPC__ENABLED").ok().as_deref() == Some("true") {
        let peers_raw = std::env::var("CUSTODY_MPC__PEERS").unwrap_or_default();
        let peers: Vec<&str> = peers_raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if peers.is_empty() {
            readiness.push(FeatureStatus::misconfigured(
                "mpc.multi_host",
                "MPC receiver serving /mpc/message",
                vec!["CUSTODY_MPC__PEERS".into()],
            ));
        } else {
            let mut unreachable: Vec<String> = Vec::new();
            for peer in &peers {
                if let Err(e) = custody_common::readiness::probe_http_reachable(peer).await {
                    unreachable.push(format!("{peer} ({e})"));
                }
            }
            if unreachable.is_empty() {
                readiness.push(FeatureStatus::ready(
                    "mpc.multi_host",
                    "MPC receiver serving /mpc/message",
                ));
            } else {
                readiness.push(FeatureStatus::degraded(
                    "mpc.multi_host",
                    "MPC receiver serving /mpc/message",
                    format!(
                        "{}/{} peers unreachable: {}",
                        unreachable.len(),
                        peers.len(),
                        unreachable.join(", "),
                    ),
                ));
            }
        }
    } else {
        readiness.push(FeatureStatus::disabled(
            "mpc.multi_host",
            "MPC multi-host. Set CUSTODY_MPC__ENABLED=true + HMAC_SECRET + PEERS.",
        ));
    }

    // Bitcoin consolidation scheduler
    readiness.push(if std::env::var("CUSTODY_HOT_WALLET__BTC_ADDRESS").is_ok() {
        FeatureStatus::ready(
            "consolidation.btc",
            "BTC hot-wallet consolidation scheduler + worker",
        )
    } else {
        FeatureStatus::disabled(
            "consolidation.btc",
            "BTC consolidation. Set CUSTODY_HOT_WALLET__BTC_ADDRESS.",
        )
    });

    // Chain scanners.
    match config.chains.ethereum.as_ref() {
        Some(eth) => {
            let rpc_url = std::env::var("CUSTODY_CHAINS__ETHEREUM__RPC_URL")
                .unwrap_or_else(|_| eth.rpc_url.clone());
            if rpc_url.is_empty() {
                readiness.push(FeatureStatus::misconfigured(
                    "scanner.ethereum",
                    "Ethereum scanner + broadcaster",
                    vec!["chains.ethereum.rpc_url".into()],
                ));
            } else {
                match custody_common::readiness::probe_ethereum_rpc(&rpc_url).await {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "scanner.ethereum",
                        "Ethereum scanner + broadcaster",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "scanner.ethereum",
                        "Ethereum scanner + broadcaster",
                        e,
                    )),
                }
            }
        }
        None => readiness.push(FeatureStatus::disabled(
            "scanner.ethereum",
            "Ethereum chain not present in config.chains.*",
        )),
    }

    match config.chains.bitcoin.as_ref() {
        Some(btc) => {
            let rpc_url = std::env::var("CUSTODY_CHAINS__BITCOIN__RPC_URL")
                .unwrap_or_else(|_| btc.rpc_url.clone());
            if rpc_url.is_empty() {
                readiness.push(FeatureStatus::misconfigured(
                    "scanner.bitcoin",
                    "Bitcoin scanner + broadcaster",
                    vec!["chains.bitcoin.rpc_url".into()],
                ));
            } else {
                match custody_common::readiness::probe_bitcoin_rpc(&rpc_url).await {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "scanner.bitcoin",
                        "Bitcoin scanner + broadcaster",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "scanner.bitcoin",
                        "Bitcoin scanner + broadcaster",
                        e,
                    )),
                }
            }
        }
        None => readiness.push(FeatureStatus::disabled(
            "scanner.bitcoin",
            "Bitcoin chain not present in config.chains.*",
        )),
    }

    match config.chains.tron.as_ref() {
        Some(tron) => {
            let api_url = std::env::var("CUSTODY_CHAINS__TRON__API_URL")
                .unwrap_or_else(|_| tron.api_url.clone());
            if api_url.is_empty() {
                readiness.push(FeatureStatus::misconfigured(
                    "scanner.tron",
                    "Tron scanner + broadcaster",
                    vec!["chains.tron.api_url".into()],
                ));
            } else {
                match custody_common::readiness::probe_tron_api(&api_url).await {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "scanner.tron",
                        "Tron scanner + broadcaster",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "scanner.tron",
                        "Tron scanner + broadcaster",
                        e,
                    )),
                }
            }
        }
        None => readiness.push(FeatureStatus::disabled(
            "scanner.tron",
            "Tron chain not present in config.chains.*",
        )),
    }

    match config.chains.solana.as_ref() {
        Some(sol) => {
            let rpc_url = std::env::var("CUSTODY_CHAINS__SOLANA__RPC_URL")
                .unwrap_or_else(|_| sol.rpc_url.clone());
            if rpc_url.is_empty() {
                readiness.push(FeatureStatus::misconfigured(
                    "scanner.solana",
                    "Solana scanner + broadcaster",
                    vec!["chains.solana.rpc_url".into()],
                ));
            } else {
                match custody_common::readiness::probe_solana_rpc(&rpc_url).await {
                    Ok(()) => readiness.push(FeatureStatus::ready(
                        "scanner.solana",
                        "Solana scanner + broadcaster",
                    )),
                    Err(e) => readiness.push(FeatureStatus::degraded(
                        "scanner.solana",
                        "Solana scanner + broadcaster",
                        e,
                    )),
                }
            }
        }
        None => readiness.push(FeatureStatus::disabled(
            "scanner.solana",
            "Solana chain not present in config.chains.*",
        )),
    }

    readiness
}

/// State transition emitted by [`diff_and_alert`]. `Degradation` and
/// `Misconfiguration` are the "fire an alert" cases — a feature that
/// was serving traffic five minutes ago now isn't. `Recovery` goes
/// the other way (Degraded → Ready) and logs at info so an operator
/// watching log tails sees when the outage cleared.
#[derive(Debug, Clone)]
pub enum Transition<'a> {
    Degradation {
        feature: &'a str,
        reason: String,
    },
    Misconfiguration {
        feature: &'a str,
        missing: Vec<String>,
    },
    Recovery {
        feature: &'a str,
        summary: &'a str,
    },
}

/// Compare consecutive reports and return every notable transition.
/// "Notable" means a change from Ready to anything worse, or from
/// Degraded/Misconfigured back to Ready. Disabled→Ready (operator
/// just turned a feature on) and Ready→Disabled (operator turned it
/// off) are intentionally NOT emitted — those are deploy-time changes,
/// not incidents.
pub fn diff_transitions<'a>(
    old: &'a ReadinessReport,
    new: &'a ReadinessReport,
) -> Vec<Transition<'a>> {
    let mut out = Vec::new();
    for new_feat in &new.features {
        let Some(old_feat) = old.features.iter().find(|f| f.name == new_feat.name) else {
            // Feature appeared for the first time in this refresh —
            // treat as deploy-time change, not an incident.
            continue;
        };
        match (&old_feat.state, &new_feat.state) {
            (FeatureState::Ready, FeatureState::Degraded { reason }) => {
                out.push(Transition::Degradation {
                    feature: &new_feat.name,
                    reason: reason.clone(),
                });
            }
            (FeatureState::Ready, FeatureState::Misconfigured { missing }) => {
                out.push(Transition::Misconfiguration {
                    feature: &new_feat.name,
                    missing: missing.clone(),
                });
            }
            (
                FeatureState::Degraded { .. } | FeatureState::Misconfigured { .. },
                FeatureState::Ready,
            ) => {
                out.push(Transition::Recovery {
                    feature: &new_feat.name,
                    summary: &new_feat.summary,
                });
            }
            _ => {}
        }
    }
    out
}

/// Per-(feature, kind) last-alert timestamps. Held across refresh
/// ticks so a flapping feature doesn't page oncall every minute.
#[derive(Default)]
pub struct AlertState {
    last_fired: std::collections::HashMap<(String, &'static str), std::time::Instant>,
}

/// How long to wait before re-alerting on the same (feature, kind).
/// Five minutes is a compromise: long enough that a noisy flapping
/// vendor coalesces into one page per cluster of bounces, short
/// enough that a genuine second outage after a brief recovery still
/// pages oncall.
pub const ALERT_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl AlertState {
    /// Return true if we should fire this alert now (i.e. cooldown
    /// has elapsed) and record that we did. Recovery alerts never
    /// cool down — they're cheap signal, and a "service recovered"
    /// message after each flap isn't as noisy as a page.
    fn should_fire(&mut self, feature: &str, kind: &'static str) -> bool {
        if kind == "recovery" {
            return true;
        }
        let key = (feature.to_string(), kind);
        let now = std::time::Instant::now();
        match self.last_fired.get(&key) {
            Some(prev) if now.duration_since(*prev) < ALERT_COOLDOWN => false,
            _ => {
                self.last_fired.insert(key, now);
                true
            }
        }
    }
}

/// Fire each transition to:
///   1. `tracing::error!` (degradation/misconfiguration) or
///      `tracing::info!` (recovery) on the `custody::readiness::alert`
///      target. Log aggregators can alert on this target alone.
///   2. A webhook POST if `CUSTODY_READINESS__ALERT_WEBHOOK` is set.
///      The body is a JSON `{feature, kind, reason, timestamp}` so
///      Slack/PagerDuty/Opsgenie incoming webhooks can parse it with
///      their standard templates.
///
/// Does `feature` match this silence pattern? Accepted forms:
/// - `scanner.ethereum` — exact match
/// - `scanner.*`        — prefix match (everything under "scanner.")
/// - `*`                — matches every feature (full mute)
///
/// Kept deliberately limited to prefix wildcards. Arbitrary glob
/// patterns add error surface without meaningful power for our
/// dotted-namespace feature names.
pub fn silence_pattern_matches(pattern: &str, feature: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        // "scanner.*" matches "scanner.ethereum" but NOT "scanner"
        // itself — a silence is always about a specific named child.
        return feature.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.') || rest.is_empty() && pattern == ".*");
    }
    pattern == feature
}

/// `state` carries anti-flap cooldowns: a degradation/misconfiguration
/// for the same feature within `ALERT_COOLDOWN` is suppressed.
///
/// `silence_patterns` carries operator-created silence patterns
/// (exact feature name or `<prefix>.*`). If any pattern matches a
/// transition's feature, the alert is dropped entirely (no log, no
/// webhook). Recovery events still fire because operators want the
/// audible "it came back" even during a maintenance window.
///
/// Failures posting to the webhook are logged but not retried — a
/// subsequent transition will try again; queueing alerts just means
/// a paged oncall sees the incident twice instead of timed out.
/// Validate alert-sink environment variables at boot. Logs a
/// warning for each suspicious value — doesn't refuse to start,
/// because an operator may be testing a staging receiver with
/// deliberately odd URLs. Catches the common footguns:
///   - Slack webhook pointed at a non-Slack host
///   - PagerDuty routing key that isn't the expected 32 hex chars
///   - generic webhook without https://
///   - HMAC secret shorter than 16 chars (brute-forceable)
///   - heartbeat URL without https://
///
/// Called once at startup from main.rs, after env parsing but
/// before the refresh task spawns.
pub fn validate_alert_config() {
    if let Ok(url) = std::env::var("CUSTODY_READINESS__ALERT_WEBHOOK")
        && !url.is_empty()
        && !url.starts_with("https://")
        && !url.starts_with("http://127.")
        && !url.starts_with("http://localhost")
    {
        tracing::warn!(
            target: "custody::readiness::config",
            url = %url,
            "Alert webhook is not https:// — payload is sent in the clear. \
             Set CUSTODY_READINESS__ALERT_WEBHOOK_SECRET at minimum if you must use http."
        );
    }

    if let Ok(url) = std::env::var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK")
        && !url.is_empty()
        && !url.starts_with("https://hooks.slack.com/")
    {
        tracing::warn!(
            target: "custody::readiness::config",
            url = %url,
            "Slack webhook URL does not match https://hooks.slack.com/... shape. \
             Verify the URL was copied from Slack's Incoming Webhooks app, not from \
             a custom proxy or someone's personal inbox."
        );
    }

    if let Ok(key) = std::env::var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY")
        && !key.is_empty()
    {
        let trimmed = key.trim();
        if trimmed.len() != 32 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            tracing::warn!(
                target: "custody::readiness::config",
                len = trimmed.len(),
                "PagerDuty routing key is not the expected 32 hex chars — v2 Events API \
                 will reject requests. Copy the Integration Key from the PD service page."
            );
        }
    }

    if let Ok(secret) = std::env::var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET")
        && !secret.is_empty()
        && secret.len() < 16
    {
        tracing::warn!(
            target: "custody::readiness::config",
            len = secret.len(),
            "Alert webhook HMAC secret is under 16 chars — consider at least 32 random bytes \
             so a brute-force attack on the signature isn't practical."
        );
    }

    if let Ok(url) = std::env::var("CUSTODY_READINESS__HEARTBEAT_URL")
        && !url.is_empty()
        && !url.starts_with("https://")
    {
        tracing::warn!(
            target: "custody::readiness::config",
            url = %url,
            "Heartbeat URL is not https:// — a MITM could suppress heartbeats to defeat \
             the dead-man's-switch alert. Use https:// wherever the monitor supports it."
        );
    }
}

/// Dead-man's-switch heartbeat. Fires after every successful probe
/// sweep — if the refresh task hangs, the service crashes, or the
/// whole box loses power, the pings stop and the external monitor
/// (healthchecks.io, Dead Man's Snitch, or a self-hosted cron job
/// watching a pingback) flips to alert. This is the one alert path
/// that survives the service itself being down, which our own
/// webhook/Slack/PD channels by definition cannot.
///
/// Set `CUSTODY_READINESS__HEARTBEAT_URL` to enable. The URL is hit
/// with a plain GET (healthchecks.io convention); failures are
/// logged but never retried — a dropped heartbeat on the next tick
/// will arrive in ≤60s and catch up.
pub async fn ping_heartbeat() {
    let Some(url) = std::env::var("CUSTODY_READINESS__HEARTBEAT_URL")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        return;
    };
    if let Err(e) = client.get(&url).send().await {
        tracing::debug!(
            target: "custody::readiness::heartbeat",
            error = %e,
            "Heartbeat ping failed — next tick will catch up"
        );
    }
}

pub async fn fire_alerts(
    state: &mut AlertState,
    silence_patterns: &[String],
    transitions: &[Transition<'_>],
) {
    if transitions.is_empty() {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();

    let is_silenced = |feature: &str| -> bool {
        silence_patterns
            .iter()
            .any(|p| silence_pattern_matches(p, feature))
    };

    // Collect webhook-eligible transitions. Per-transition tracing
    // still fires inline so log aggregators see every event at the
    // exact moment it happened; the webhook side is the only one
    // that gets batched (one POST per refresh cycle, not N — avoids
    // ringing a pager N times when a whole region goes down at once).
    let mut batch: Vec<serde_json::Value> = Vec::with_capacity(transitions.len());
    for t in transitions {
        match t {
            Transition::Degradation { feature, reason } => {
                if is_silenced(feature) {
                    tracing::debug!(
                        target: "custody::readiness::alert",
                        feature = %feature,
                        "Degradation alert suppressed (silence active)"
                    );
                    continue;
                }
                if !state.should_fire(feature, "degradation") {
                    tracing::debug!(
                        target: "custody::readiness::alert",
                        feature = %feature,
                        "Degradation alert suppressed (cooldown active)"
                    );
                    continue;
                }
                tracing::error!(
                    target: "custody::readiness::alert",
                    feature = %feature,
                    reason = %reason,
                    "Feature DEGRADED — handlers for this subsystem will 503 until recovery"
                );
                batch.push(serde_json::json!({
                    "kind": "degradation",
                    "feature": feature,
                    "reason": reason,
                }));
            }
            Transition::Misconfiguration { feature, missing } => {
                if is_silenced(feature) {
                    tracing::debug!(
                        target: "custody::readiness::alert",
                        feature = %feature,
                        "Misconfiguration alert suppressed (silence active)"
                    );
                    continue;
                }
                if !state.should_fire(feature, "misconfiguration") {
                    tracing::debug!(
                        target: "custody::readiness::alert",
                        feature = %feature,
                        "Misconfiguration alert suppressed (cooldown active)"
                    );
                    continue;
                }
                tracing::error!(
                    target: "custody::readiness::alert",
                    feature = %feature,
                    missing = ?missing,
                    "Feature MISCONFIGURED — handlers for this subsystem will 503"
                );
                batch.push(serde_json::json!({
                    "kind": "misconfiguration",
                    "feature": feature,
                    "missing": missing,
                }));
            }
            Transition::Recovery { feature, summary } => {
                // Recovery bypasses cooldown AND silences — operators
                // want the "it came back" signal even during a
                // scheduled maintenance window.
                state.should_fire(feature, "recovery");
                tracing::info!(
                    target: "custody::readiness::alert",
                    feature = %feature,
                    summary = %summary,
                    "Feature RECOVERED — handlers serving this subsystem are live again"
                );
                batch.push(serde_json::json!({
                    "kind": "recovery",
                    "feature": feature,
                    "summary": summary,
                }));
            }
        }
    }

    if batch.is_empty() {
        return;
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        tracing::warn!(
            target: "custody::readiness::alert",
            "Could not build alert webhook HTTP client"
        );
        return;
    };
    // Every configured channel gets the same batch. A single outage
    // thus lands in Slack, PagerDuty, and the raw webhook at once;
    // operators pick whichever they want and unset the others.
    post_generic_webhook(&client, &now, &batch).await;
    post_slack_webhook(&client, &now, &batch).await;
    post_pagerduty(&client, transitions).await;
}

/// Send a request with exponential-backoff retry on transient
/// errors. Retries only cover failure modes an immediate replay can
/// legitimately recover from:
///
///   - connect errors / timeouts (network blip)
///   - HTTP 5xx (server-side; 500, 502, 503, 504)
///   - HTTP 429 (rate limited)
///
/// 4xx (other than 429) is a caller-side problem — webhook secret
/// revoked, Slack channel deleted, PagerDuty routing key invalid.
/// Retrying burns budget without fixing anything, so we log and
/// move on.
///
/// Three attempts total with 500ms / 1s / 2s spacing. That's ~3.5s
/// of wall time in the worst case, well under the refresh tick.
async fn send_with_retry(
    req_builder: impl Fn() -> reqwest::RequestBuilder,
    target: &'static str,
) -> Option<reqwest::Response> {
    const DELAYS_MS: [u64; 3] = [500, 1_000, 2_000];
    for (attempt, delay) in DELAYS_MS.iter().enumerate() {
        let req = req_builder();
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                // Happy path: 2xx + 3xx. Slack / PD / generic all
                // answer 200 or 202 on acceptance.
                if status.is_success() || status.is_redirection() {
                    return Some(resp);
                }
                // Retry on 5xx + 429. Stay on 4xx (other than 429).
                let retriable = status.is_server_error() || status.as_u16() == 429;
                if !retriable {
                    tracing::warn!(
                        target: "custody::readiness::alert",
                        sink = target,
                        status = %status,
                        "Alert sink returned non-retriable status"
                    );
                    return Some(resp);
                }
                if attempt + 1 == DELAYS_MS.len() {
                    tracing::warn!(
                        target: "custody::readiness::alert",
                        sink = target,
                        status = %status,
                        attempts = attempt + 1,
                        "Alert sink gave up after retries"
                    );
                    return Some(resp);
                }
                tracing::debug!(
                    target: "custody::readiness::alert",
                    sink = target,
                    status = %status,
                    attempt = attempt + 1,
                    "Retrying alert sink after transient status"
                );
            }
            Err(e) => {
                if attempt + 1 == DELAYS_MS.len() {
                    tracing::warn!(
                        target: "custody::readiness::alert",
                        sink = target,
                        error = %e,
                        attempts = attempt + 1,
                        "Alert sink delivery failed after retries"
                    );
                    return None;
                }
                tracing::debug!(
                    target: "custody::readiness::alert",
                    sink = target,
                    error = %e,
                    attempt = attempt + 1,
                    "Retrying alert sink after transport error"
                );
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(*delay)).await;
    }
    None
}

/// Generic webhook POST with HMAC-signed body. Operators use this
/// for custom receivers (internal Slack bots, incident-management
/// systems, webhook-test inboxes during rollout).
async fn post_generic_webhook(
    client: &reqwest::Client,
    timestamp: &str,
    transitions: &[serde_json::Value],
) {
    let Some(url) = std::env::var("CUSTODY_READINESS__ALERT_WEBHOOK")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let body = serde_json::json!({
        "service": "mg-custody",
        "timestamp": timestamp,
        "transitions": transitions,
    });
    let body_bytes = match serde_json::to_vec(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(target: "custody::readiness::alert", error = %e, "alert body serialize");
            return;
        }
    };

    let unix_ts = chrono::Utc::now().timestamp();
    let secret = std::env::var("CUSTODY_READINESS__ALERT_WEBHOOK_SECRET")
        .ok()
        .filter(|s| !s.is_empty());
    let signature =
        secret.as_ref().map(|s| sign_webhook(s, unix_ts, &body_bytes));

    // Retry builder re-creates the RequestBuilder each attempt;
    // reqwest consumes it on `.send()`, so we can't share one.
    send_with_retry(
        || {
            let mut r = client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body_bytes.clone());
            if let Some(sig) = signature.as_ref() {
                r = r.header("X-Custody-Signature", format!("t={unix_ts},v1={sig}"));
            }
            r
        },
        "generic_webhook",
    )
    .await;
}

/// Slack Incoming Webhook. Formats every transition as a separate
/// attachment so the Slack message is readable even when a whole
/// region fails — each attachment gets a colour band
/// (red for degradation, yellow for misconfiguration, green for
/// recovery) that Slack renders as a left border.
///
/// Docs: https://api.slack.com/messaging/webhooks
async fn post_slack_webhook(
    client: &reqwest::Client,
    timestamp: &str,
    transitions: &[serde_json::Value],
) {
    let Some(url) = std::env::var("CUSTODY_READINESS__ALERT_SLACK_WEBHOOK")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let attachments: Vec<serde_json::Value> = transitions
        .iter()
        .map(|t| {
            let kind = t.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let feature = t.get("feature").and_then(|v| v.as_str()).unwrap_or("?");
            let (color, title_prefix) = match kind {
                "degradation" => ("danger", "🚨 DEGRADED"),
                "misconfiguration" => ("warning", "⚠️ MISCONFIGURED"),
                "recovery" => ("good", "✅ RECOVERED"),
                _ => ("#888", "ℹ️"),
            };
            let text = t
                .get("reason")
                .or_else(|| t.get("summary"))
                .or_else(|| t.get("missing"))
                .map(|v| match v {
                    serde_json::Value::Array(a) => a
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => v.as_str().unwrap_or("").to_string(),
                })
                .unwrap_or_default();
            serde_json::json!({
                "color": color,
                "title": format!("{title_prefix} · {feature}"),
                "text": text,
                "footer": "mg-custody readiness",
                "ts": chrono::Utc::now().timestamp(),
            })
        })
        .collect();
    let body = serde_json::json!({
        "text": format!(
            "MG Custody readiness — {} transition(s) at {timestamp}",
            transitions.len(),
        ),
        "attachments": attachments,
    });
    send_with_retry(|| client.post(&url).json(&body), "slack").await;
}

/// PagerDuty Events API v2 — https://events.pagerduty.com/v2/enqueue.
/// Each transition becomes a separate event because the PD API is
/// per-event, not batched. dedup_key=<feature> so PD collapses
/// repeat triggers on the same feature into one incident until a
/// `resolve` event lands.
///
///   - degradation / misconfiguration → action=trigger
///   - recovery → action=resolve (same dedup_key as the trigger)
///
/// Docs: https://developer.pagerduty.com/api-reference/YXBpOjI3NDgyNjU-pager-duty-v2-events-api
async fn post_pagerduty(client: &reqwest::Client, transitions: &[Transition<'_>]) {
    let Some(routing_key) = std::env::var("CUSTODY_READINESS__ALERT_PAGERDUTY_ROUTING_KEY")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    const URL: &str = "https://events.pagerduty.com/v2/enqueue";
    for t in transitions {
        let (action, feature, severity, summary, detail): (&str, &str, &str, String, String) =
            match t {
                Transition::Degradation { feature, reason } => (
                    "trigger",
                    feature,
                    "error",
                    format!("{feature} DEGRADED"),
                    reason.clone(),
                ),
                Transition::Misconfiguration { feature, missing } => (
                    "trigger",
                    feature,
                    "warning",
                    format!("{feature} MISCONFIGURED"),
                    missing.join(", "),
                ),
                Transition::Recovery { feature, summary } => (
                    "resolve",
                    feature,
                    "info",
                    format!("{feature} RECOVERED"),
                    (*summary).to_string(),
                ),
            };
        let body = serde_json::json!({
            "routing_key": routing_key,
            "event_action": action,
            "dedup_key": feature,
            "payload": {
                "summary": summary,
                "severity": severity,
                "source": "mg-custody",
                "component": feature,
                "custom_details": {
                    "detail": detail,
                }
            }
        });
        send_with_retry(|| client.post(URL).json(&body), "pagerduty").await;
    }
}

/// HMAC-SHA256 over `<timestamp>.<body>`. Hex-encoded so it fits in
/// a plain header value. Receiver-side verification pattern:
///
///   signed = hmac_sha256(secret, f"{t}.{body}").hex()
///   constant_time_compare(signed, v1)
///
/// The timestamp is bound into the signed payload so a valid signature
/// can't be replayed against a stale body.
fn sign_webhook(secret: &str, timestamp: i64, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let bytes = mac.finalize().into_bytes();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_with(features: Vec<FeatureStatus>) -> ReadinessReport {
        let mut r = ReadinessReport::new();
        for f in features {
            r.push(f);
        }
        r
    }

    #[test]
    fn transition_ready_to_degraded_fires_degradation() {
        let old = report_with(vec![FeatureStatus::ready("scanner.ethereum", "ETH")]);
        let new = report_with(vec![FeatureStatus::degraded(
            "scanner.ethereum",
            "ETH",
            "eth_chainId timeout",
        )]);
        let t = diff_transitions(&old, &new);
        assert_eq!(t.len(), 1);
        matches!(t[0], Transition::Degradation { .. });
    }

    #[test]
    fn transition_ready_to_misconfigured_fires_misconfiguration() {
        let old = report_with(vec![FeatureStatus::ready("push.apns", "APNs")]);
        let new = report_with(vec![FeatureStatus::misconfigured(
            "push.apns",
            "APNs",
            vec!["CUSTODY_APNS__KEY_P8".into()],
        )]);
        let t = diff_transitions(&old, &new);
        assert_eq!(t.len(), 1);
        matches!(t[0], Transition::Misconfiguration { .. });
    }

    #[test]
    fn transition_degraded_to_ready_fires_recovery() {
        let old = report_with(vec![FeatureStatus::degraded(
            "scanner.solana",
            "SOL",
            "getHealth failed",
        )]);
        let new = report_with(vec![FeatureStatus::ready("scanner.solana", "SOL")]);
        let t = diff_transitions(&old, &new);
        assert_eq!(t.len(), 1);
        matches!(t[0], Transition::Recovery { .. });
    }

    #[test]
    fn transition_disabled_to_ready_is_deploy_not_incident() {
        // Operator just turned the feature on — not an alertable event.
        let old = report_with(vec![FeatureStatus::disabled("push.fcm", "FCM")]);
        let new = report_with(vec![FeatureStatus::ready("push.fcm", "FCM")]);
        let t = diff_transitions(&old, &new);
        assert!(t.is_empty());
    }

    #[test]
    fn transition_ready_to_disabled_is_deploy_not_incident() {
        // Operator chose to turn it off — not an outage.
        let old = report_with(vec![FeatureStatus::ready("push.fcm", "FCM")]);
        let new = report_with(vec![FeatureStatus::disabled("push.fcm", "FCM")]);
        let t = diff_transitions(&old, &new);
        assert!(t.is_empty());
    }

    #[test]
    fn new_feature_does_not_fire() {
        // Feature appeared for the first time (e.g. operator restarted
        // with a new chain configured). Nothing to compare against —
        // no alert.
        let old = report_with(vec![]);
        let new = report_with(vec![FeatureStatus::degraded(
            "scanner.bitcoin",
            "BTC",
            "unreachable",
        )]);
        let t = diff_transitions(&old, &new);
        assert!(t.is_empty());
    }

    #[test]
    fn alert_state_cooldown_suppresses_duplicate() {
        let mut st = AlertState::default();
        // First fire is allowed.
        assert!(st.should_fire("scanner.ethereum", "degradation"));
        // Second fire immediately after is suppressed.
        assert!(!st.should_fire("scanner.ethereum", "degradation"));
        // Different kind is tracked separately.
        assert!(st.should_fire("scanner.ethereum", "misconfiguration"));
        // Different feature is tracked separately.
        assert!(st.should_fire("scanner.bitcoin", "degradation"));
    }

    #[test]
    fn alert_state_recovery_always_fires() {
        let mut st = AlertState::default();
        // Recovery bypasses cooldown — a feature that bounces up and
        // down should still log each recovery.
        assert!(st.should_fire("scanner.ethereum", "recovery"));
        assert!(st.should_fire("scanner.ethereum", "recovery"));
        assert!(st.should_fire("scanner.ethereum", "recovery"));
    }

    #[tokio::test]
    async fn fire_alerts_respects_exact_silence() {
        // Degradation on a silenced feature must not even try to
        // write to the tracing target. We can't easily assert on
        // tracing output without a subscriber fixture, but we CAN
        // assert that the cooldown state remains untouched — an
        // unsuppressed alert would tick the cooldown.
        let mut alerts = AlertState::default();
        let silenced = vec!["scanner.ethereum".to_string()];
        let transitions = vec![Transition::Degradation {
            feature: "scanner.ethereum",
            reason: "unreachable".into(),
        }];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        assert!(alerts.last_fired.is_empty());
    }

    #[tokio::test]
    async fn fire_alerts_respects_prefix_silence() {
        let mut alerts = AlertState::default();
        let silenced = vec!["push.*".to_string()];
        let transitions = vec![
            Transition::Degradation {
                feature: "push.fcm",
                reason: "JSON parse".into(),
            },
            Transition::Degradation {
                feature: "push.apns",
                reason: "EC parse".into(),
            },
            // Different namespace — not silenced.
            Transition::Degradation {
                feature: "scanner.ethereum",
                reason: "RPC down".into(),
            },
        ];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        // Only scanner.ethereum should have ticked the cooldown.
        assert_eq!(alerts.last_fired.len(), 1);
    }

    #[tokio::test]
    async fn fire_alerts_respects_wildcard_silence() {
        let mut alerts = AlertState::default();
        let silenced = vec!["*".to_string()];
        let transitions = vec![
            Transition::Degradation {
                feature: "scanner.ethereum",
                reason: "A".into(),
            },
            Transition::Misconfiguration {
                feature: "push.apns",
                missing: vec!["CUSTODY_APNS__KEY_P8".into()],
            },
        ];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        assert!(alerts.last_fired.is_empty());
    }

    #[tokio::test]
    async fn fire_alerts_ignores_empty_silence_set() {
        let mut alerts = AlertState::default();
        let silenced: Vec<String> = vec![];
        let transitions = vec![Transition::Degradation {
            feature: "scanner.ethereum",
            reason: "unreachable".into(),
        }];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        assert_eq!(alerts.last_fired.len(), 1);
    }

    #[test]
    fn silence_pattern_exact_match() {
        assert!(silence_pattern_matches("scanner.ethereum", "scanner.ethereum"));
        assert!(!silence_pattern_matches("scanner.ethereum", "scanner.bitcoin"));
    }

    #[test]
    fn silence_pattern_prefix_wildcard() {
        assert!(silence_pattern_matches("scanner.*", "scanner.ethereum"));
        assert!(silence_pattern_matches("scanner.*", "scanner.bitcoin"));
        assert!(silence_pattern_matches("push.*", "push.fcm"));
        // Must NOT match the bare namespace (no trailing dot child).
        assert!(!silence_pattern_matches("scanner.*", "scanner"));
        assert!(!silence_pattern_matches("scanner.*", "scanning.ethereum"));
        // Cross-namespace is not matched.
        assert!(!silence_pattern_matches("scanner.*", "push.fcm"));
    }

    #[test]
    fn silence_pattern_full_wildcard() {
        assert!(silence_pattern_matches("*", "scanner.ethereum"));
        assert!(silence_pattern_matches("*", "push.apns"));
        assert!(silence_pattern_matches("*", ""));
    }

    #[test]
    fn multiple_features_diff_independently() {
        let old = report_with(vec![
            FeatureStatus::ready("a", "a"),
            FeatureStatus::ready("b", "b"),
            FeatureStatus::degraded("c", "c", "bad"),
        ]);
        let new = report_with(vec![
            FeatureStatus::degraded("a", "a", "went down"),
            FeatureStatus::ready("b", "b"),
            FeatureStatus::ready("c", "c"),
        ]);
        let t = diff_transitions(&old, &new);
        assert_eq!(t.len(), 2); // a degraded, c recovered
    }

    #[test]
    fn sign_webhook_is_deterministic_and_secret_dependent() {
        let body = br#"{"service":"mg-custody","transitions":[]}"#;
        let a1 = sign_webhook("secret-a", 1_700_000_000, body);
        let a2 = sign_webhook("secret-a", 1_700_000_000, body);
        let b = sign_webhook("secret-b", 1_700_000_000, body);
        assert_eq!(a1, a2, "same key + ts + body → same signature");
        assert_ne!(a1, b, "different secret → different signature");
        assert_eq!(a1.len(), 64, "SHA-256 hex = 64 chars");
    }

    #[test]
    fn sign_webhook_changes_with_timestamp() {
        let body = br#"{}"#;
        let at_t1 = sign_webhook("secret", 1_700_000_000, body);
        let at_t2 = sign_webhook("secret", 1_700_000_001, body);
        // Replay protection: same body 1 second later must produce
        // a different signature.
        assert_ne!(at_t1, at_t2);
    }
}
