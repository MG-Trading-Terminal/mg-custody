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

/// Build a fresh `ReadinessReport` by re-running every probe.
///
/// `travel_rule_configured` is passed in instead of the provider
/// itself because the provider type pulls in a lot of transitive deps
/// we don't need here — we only care whether one is wired at all.
pub async fn build(
    config: &CustodyConfig,
    email_sender: &EmailSender,
    travel_rule_configured: bool,
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

    // Travel Rule
    readiness.push(if travel_rule_configured {
        FeatureStatus::ready("travel_rule.notabene", "Travel Rule dispatch via Notabene")
    } else {
        FeatureStatus::disabled(
            "travel_rule.notabene",
            "Travel Rule. Set CUSTODY_TRAVELRULE__NOTABENE_API_KEY.",
        )
    });

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
/// `state` carries anti-flap cooldowns: a degradation/misconfiguration
/// for the same feature within `ALERT_COOLDOWN` is suppressed.
///
/// `silenced_features` is the set of feature names currently under
/// an operator-created silence — those alerts are dropped entirely
/// (no log, no webhook) during the silence window. Recovery events
/// still fire because operators want the audible "it came back" even
/// during a maintenance window.
///
/// Failures posting to the webhook are logged but not retried — a
/// subsequent transition will try again; queueing alerts just means
/// a paged oncall sees the incident twice instead of timed out.
pub async fn fire_alerts(
    state: &mut AlertState,
    silenced_features: &std::collections::HashSet<String>,
    transitions: &[Transition<'_>],
) {
    if transitions.is_empty() {
        return;
    }
    let webhook = std::env::var("CUSTODY_READINESS__ALERT_WEBHOOK")
        .ok()
        .filter(|s| !s.is_empty());
    let now = chrono::Utc::now().to_rfc3339();
    let client = webhook
        .as_ref()
        .map(|_| reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .ok())
        .flatten();

    for t in transitions {
        match t {
            Transition::Degradation { feature, reason } => {
                if silenced_features.contains(*feature) {
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
                post_webhook(&client, &webhook, &now, "degradation", feature, Some(reason.as_str()), None).await;
            }
            Transition::Misconfiguration { feature, missing } => {
                if silenced_features.contains(*feature) {
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
                let missing_str = missing.join(", ");
                post_webhook(&client, &webhook, &now, "misconfiguration", feature, None, Some(&missing_str)).await;
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
                post_webhook(&client, &webhook, &now, "recovery", feature, Some(summary), None).await;
            }
        }
    }
}

async fn post_webhook(
    client: &Option<reqwest::Client>,
    url: &Option<String>,
    timestamp: &str,
    kind: &str,
    feature: &str,
    reason: Option<&str>,
    missing: Option<&str>,
) {
    let (Some(client), Some(url)) = (client.as_ref(), url.as_ref()) else {
        return;
    };
    let body = serde_json::json!({
        "service": "mg-custody",
        "kind": kind,
        "feature": feature,
        "reason": reason,
        "missing": missing,
        "timestamp": timestamp,
    });
    if let Err(e) = client.post(url).json(&body).send().await {
        tracing::warn!(
            target: "custody::readiness::alert",
            error = %e,
            "Readiness alert webhook delivery failed"
        );
    }
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
    async fn fire_alerts_respects_silences() {
        // Degradation on a silenced feature must not even try to
        // write to the tracing target. We can't easily assert on
        // tracing output without a subscriber fixture, but we CAN
        // assert that the cooldown state remains untouched — an
        // unsuppressed alert would tick the cooldown.
        let mut alerts = AlertState::default();
        let silenced: std::collections::HashSet<String> =
            ["scanner.ethereum".to_string()].into_iter().collect();
        let transitions = vec![Transition::Degradation {
            feature: "scanner.ethereum",
            reason: "unreachable".into(),
        }];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        // Cooldown HashMap must be empty — silenced alerts never
        // touch the cooldown tracker.
        assert!(alerts.last_fired.is_empty());
    }

    #[tokio::test]
    async fn fire_alerts_ignores_empty_silence_set() {
        let mut alerts = AlertState::default();
        let silenced: std::collections::HashSet<String> = std::collections::HashSet::new();
        let transitions = vec![Transition::Degradation {
            feature: "scanner.ethereum",
            reason: "unreachable".into(),
        }];
        fire_alerts(&mut alerts, &silenced, &transitions).await;
        // Cooldown should have one entry now (alert was fired).
        assert_eq!(alerts.last_fired.len(), 1);
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
}
