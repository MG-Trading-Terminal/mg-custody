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
use custody_common::readiness::{FeatureStatus, ReadinessReport};

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
