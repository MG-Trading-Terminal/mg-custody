//! Feature readiness report.
//!
//! Every custody feature has a truthful lifecycle state:
//!
//! - `Ready`        — all required config present AND at least one
//!                    cold-start probe succeeded (DB reachable, RPC
//!                    answering, external API responding, etc).
//!                    Handlers serving the feature can trust it.
//! - `Disabled`     — operator chose not to enable it. No traffic goes
//!                    through, endpoints that serve it should 503.
//! - `Misconfigured`— operator appears to have TRIED to enable it
//!                    (one or more partial config keys set) but the
//!                    set is incomplete. Lists the missing keys.
//! - `Degraded`     — fully configured but the cold-start probe
//!                    failed (e.g. HSM library present but slot
//!                    missing, RPC URL set but unreachable). Carries
//!                    the underlying reason.
//!
//! The golden rule this module enforces: **never let a feature
//! silently pretend to work**. Every handler that serves a feature
//! should look up its status here and respond with an honest 503
//! when the state isn't `Ready`, rather than accepting the request
//! and dropping it on the floor.
//!
//! The `/v1/health/features` endpoint exposes the whole report to
//! operators. `startup_log_report` prints a formatted table at boot
//! so the first thing in `journalctl` is a truthful readiness map,
//! not 30 independent log lines an operator has to correlate.

use serde::Serialize;

/// State of a single custody feature.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FeatureState {
    /// Wired, probed, ready to serve.
    Ready,
    /// Explicitly unconfigured — operator chose not to enable.
    Disabled,
    /// Partial config — not enough to start. Lists missing env/config keys.
    Misconfigured { missing: Vec<String> },
    /// Fully configured but a startup probe failed. Carries the reason.
    Degraded { reason: String },
}

impl FeatureState {
    pub fn is_ready(&self) -> bool {
        matches!(self, FeatureState::Ready)
    }

    /// Short single-word label for the startup table.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Disabled => "DISABLED",
            Self::Misconfigured { .. } => "MISCONFIGURED",
            Self::Degraded { .. } => "DEGRADED",
        }
    }

    /// One-line human-readable reason — already included in the
    /// startup log table.
    pub fn reason(&self) -> String {
        match self {
            Self::Ready => "ok".into(),
            Self::Disabled => "not enabled in this build".into(),
            Self::Misconfigured { missing } => {
                format!("missing config: {}", missing.join(", "))
            }
            Self::Degraded { reason } => reason.clone(),
        }
    }
}

/// One feature slot in the report.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureStatus {
    /// Canonical feature name — used both in the JSON payload and
    /// as the key for `ReadinessReport::feature_ready(name)`.
    pub name: String,
    /// Short human-readable summary of what the feature does.
    pub summary: String,
    #[serde(flatten)]
    pub state: FeatureState,
}

impl FeatureStatus {
    pub fn ready(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            state: FeatureState::Ready,
        }
    }

    pub fn disabled(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            state: FeatureState::Disabled,
        }
    }

    pub fn misconfigured(
        name: impl Into<String>,
        summary: impl Into<String>,
        missing: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            state: FeatureState::Misconfigured { missing },
        }
    }

    pub fn degraded(
        name: impl Into<String>,
        summary: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            state: FeatureState::Degraded {
                reason: reason.into(),
            },
        }
    }
}

/// Collected readiness info for the whole service.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ReadinessReport {
    pub features: Vec<FeatureStatus>,
}

impl ReadinessReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, f: FeatureStatus) {
        self.features.push(f);
    }

    /// Query a feature's readiness by name. Handlers should call
    /// this and refuse with 503 when the state isn't Ready.
    pub fn feature_ready(&self, name: &str) -> bool {
        self.features.iter().any(|f| f.name == name && f.state.is_ready())
    }

    /// Full status for a named feature (useful for 503 responses
    /// so the operator sees the exact reason).
    pub fn feature(&self, name: &str) -> Option<&FeatureStatus> {
        self.features.iter().find(|f| f.name == name)
    }

    /// Count features by broad category. Useful for the /health
    /// summary and for emitting prometheus gauges.
    pub fn summary_counts(&self) -> (usize, usize, usize, usize) {
        let (mut ready, mut disabled, mut misc, mut degraded) = (0, 0, 0, 0);
        for f in &self.features {
            match f.state {
                FeatureState::Ready => ready += 1,
                FeatureState::Disabled => disabled += 1,
                FeatureState::Misconfigured { .. } => misc += 1,
                FeatureState::Degraded { .. } => degraded += 1,
            }
        }
        (ready, disabled, misc, degraded)
    }
}

/// Print a human-readable readiness table to `tracing::info!` at
/// service boot. One call in main.rs replaces a scatter of
/// per-subsystem init logs.
///
/// Output shape (truncated to keep lines under ~100 chars):
///
///   ╭── FEATURE READINESS ──────────────────────────────────────────────╮
///   │ staking.lido          READY         ok                             │
///   │ screening.chainalysis DISABLED      not enabled in this build      │
///   │ hsm.pkcs11            MISCONFIGURED missing config: HSM_PIN        │
///   │ push.fcm              DEGRADED      service_account JSON malformed │
///   ╰───────────────────────────────────────────────────────────────────╯
pub fn startup_log_report(report: &ReadinessReport) {
    let bar = "─".repeat(72);
    let mut lines = String::new();
    lines.push_str(&format!("\n╭── FEATURE READINESS {} ╮\n", &bar[..50]));
    for f in &report.features {
        let label = f.state.label();
        let reason = f.state.reason();
        // Pad name to 28 chars, label to 14, reason fills the rest
        // (truncated to 30 chars to keep log lines reasonable).
        let reason_short = if reason.len() > 28 {
            format!("{}…", &reason[..27])
        } else {
            reason
        };
        lines.push_str(&format!(
            "│ {name:<28} {label:<13} {reason_short:<28} │\n",
            name = truncate(&f.name, 28),
            label = label,
            reason_short = reason_short,
        ));
    }
    let (ready, disabled, misc, degraded) = report.summary_counts();
    lines.push_str(&format!(
        "├{bar}┤\n│ {summary:<72} │\n╰{bar}╯",
        bar = bar,
        summary = format!(
            " {ready} ready  |  {disabled} disabled  |  {misc} misconfigured  |  {degraded} degraded"
        ),
    ));
    tracing::info!(target: "custody::readiness", "{lines}");
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(width.saturating_sub(1)).collect::<String>())
    }
}

// ---------------------------------------------------------------------------
// Cold-start probes
// ---------------------------------------------------------------------------
//
// Each probe does ONE network call with a short timeout and returns
// `Ok(())` on success or `Err(reason)` on any failure. The reason
// string lands in a `FeatureState::Degraded { reason }` so operators
// see WHY the probe failed, not just that it did. All probes are
// best-effort — we never block startup on them forever.

/// Timeout every probe is expected to honour. Five seconds is long
/// enough for a transatlantic TCP handshake + TLS round-trip but
/// short enough that a misconfigured endpoint doesn't delay boot.
pub const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// `eth_chainId` probe — the cheapest read to confirm an Ethereum RPC
/// endpoint is live, understands JSON-RPC, and returns a chain id
/// that parses as a hex integer.
pub async fn probe_ethereum_rpc(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": [],
    });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("eth_chainId request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("eth_chainId returned HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("eth_chainId JSON: {e}"))?;
    if let Some(err) = json.get("error") {
        return Err(format!("eth_chainId RPC error: {err}"));
    }
    json["result"]
        .as_str()
        .ok_or_else(|| "eth_chainId result missing".to_string())?;
    Ok(())
}

/// TCP-connect probe for an SMTP relay. No EHLO yet — a successful
/// connect tells us the host resolves + accepts TCP on the configured
/// port, which is 80 % of the value. EHLO+TLS negotiation is a
/// separate follow-up because it means reaching into `lettre` internals.
pub async fn probe_smtp_tcp(host: &str, port: u16) -> Result<(), String> {
    let addr = format!("{host}:{port}");
    let fut = tokio::net::TcpStream::connect(&addr);
    tokio::time::timeout(PROBE_TIMEOUT, fut)
        .await
        .map_err(|_| format!("SMTP connect to {addr} timed out after {PROBE_TIMEOUT:?}"))?
        .map_err(|e| format!("SMTP connect to {addr}: {e}"))?;
    Ok(())
}

/// Generic HTTP HEAD/GET probe for vendor status endpoints. A 2xx
/// means "reachable". 401/403 also counts as "reachable" — we're
/// testing network reachability + DNS + TLS, not authentication,
/// because the auth check would require the operator's live API
/// key and spending a request quota on a no-op.
pub async fn probe_http_reachable(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .head(url)
        .send()
        .await
        .map_err(|e| format!("HTTP HEAD {url}: {e}"))?;
    let status = resp.status();
    // 2xx, 3xx, 401, 403 — server is there, we just can't auth.
    // 404 is ambiguous (endpoint gone vs bad URL) but still counts
    // as "host reachable" for our purposes.
    if status.is_success() || status.is_redirection() || matches!(status.as_u16(), 401 | 403 | 404) {
        Ok(())
    } else {
        Err(format!("HTTP HEAD {url}: returned {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_passes_is_ready_check() {
        let s = FeatureState::Ready;
        assert!(s.is_ready());
        assert_eq!(s.label(), "READY");
    }

    #[test]
    fn disabled_is_not_ready() {
        let s = FeatureState::Disabled;
        assert!(!s.is_ready());
    }

    #[test]
    fn misconfigured_carries_missing_keys() {
        let s = FeatureState::Misconfigured {
            missing: vec!["HSM_PIN".into(), "HSM_LIB_PATH".into()],
        };
        assert!(!s.is_ready());
        assert!(s.reason().contains("HSM_PIN"));
        assert!(s.reason().contains("HSM_LIB_PATH"));
    }

    #[test]
    fn report_feature_ready_lookup() {
        let mut r = ReadinessReport::new();
        r.push(FeatureStatus::ready("staking.lido", "Lido ETH staking"));
        r.push(FeatureStatus::disabled("staking.marinade", "Marinade SOL"));
        assert!(r.feature_ready("staking.lido"));
        assert!(!r.feature_ready("staking.marinade"));
        assert!(!r.feature_ready("unknown.feature"));
    }

    #[test]
    fn report_summary_counts() {
        let mut r = ReadinessReport::new();
        r.push(FeatureStatus::ready("a", "x"));
        r.push(FeatureStatus::ready("b", "x"));
        r.push(FeatureStatus::disabled("c", "x"));
        r.push(FeatureStatus::misconfigured("d", "x", vec!["K1".into()]));
        r.push(FeatureStatus::degraded("e", "x", "bad"));
        let (ready, disabled, misc, degraded) = r.summary_counts();
        assert_eq!((ready, disabled, misc, degraded), (2, 1, 1, 1));
    }

    #[test]
    fn serialized_json_includes_state_tag() {
        let s = FeatureStatus::misconfigured("x", "y", vec!["Z".into()]);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"state\":\"misconfigured\""));
        assert!(json.contains("\"missing\""));
    }

    #[tokio::test]
    async fn probe_smtp_tcp_times_out_on_closed_port() {
        // Loopback on a refused port. Connect should fail fast with
        // a non-empty reason string; it must NOT succeed.
        let res = probe_smtp_tcp("127.0.0.1", 1).await;
        assert!(res.is_err());
        assert!(!res.unwrap_err().is_empty());
    }

    #[tokio::test]
    async fn probe_ethereum_rpc_errors_on_unresolvable_host() {
        // `.invalid` TLD is guaranteed to never resolve per RFC 6761.
        let res = probe_ethereum_rpc("http://never-resolves.invalid/rpc").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn probe_http_reachable_errors_on_unresolvable_host() {
        let res = probe_http_reachable("https://never-resolves.invalid/").await;
        assert!(res.is_err());
    }
}
