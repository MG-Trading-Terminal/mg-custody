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

/// Shared handle around a `ReadinessReport` that lets a background
/// task hot-swap the snapshot as probes refresh, while handler-side
/// readers stay sync and lock-free on the fast path. Internally
/// `Arc<std::sync::RwLock<Arc<ReadinessReport>>>` — the critical
/// section is a single `clone()` of an `Arc` so read contention is
/// negligible.
#[derive(Clone)]
pub struct ReadinessHandle {
    inner: std::sync::Arc<std::sync::RwLock<std::sync::Arc<ReadinessReport>>>,
    /// Wake-up signal for the background refresh task. An operator
    /// hitting `POST /v1/health/features/refresh` calls `notify_one()`
    /// here; the task catches up and re-runs every probe ahead of
    /// the periodic tick.
    refresh_notify: std::sync::Arc<tokio::sync::Notify>,
}

impl ReadinessHandle {
    pub fn new(report: ReadinessReport) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(report))),
            refresh_notify: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Read-cheap snapshot. Takes the read lock just long enough to
    /// clone an `Arc<ReadinessReport>`, then releases it. Callers get
    /// a frozen view that stays valid even if the background refresh
    /// swaps in a new report mid-request.
    pub fn snapshot(&self) -> std::sync::Arc<ReadinessReport> {
        self.inner
            .read()
            .expect("readiness lock poisoned")
            .clone()
    }

    /// Hot-swap the report. The background refresh task calls this
    /// after rebuilding the report; in-flight request handlers keep
    /// the old snapshot they captured at entry.
    pub fn replace(&self, report: ReadinessReport) {
        *self.inner.write().expect("readiness lock poisoned") =
            std::sync::Arc::new(report);
    }

    /// Ask the background refresh task to re-run every probe now.
    /// Safe to call from anywhere — if no task is listening (e.g.
    /// test fixtures built with a static `new()`), the notification
    /// is simply dropped.
    pub fn request_refresh(&self) {
        self.refresh_notify.notify_one();
    }

    /// Await a refresh request. The background refresh task awaits
    /// on this between periodic ticks; `select!` it against a
    /// `tokio::time::interval` tick to get "rebuild every N seconds
    /// OR whenever an operator pokes the endpoint".
    pub async fn refresh_requested(&self) {
        self.refresh_notify.notified().await
    }

    pub fn feature_ready(&self, name: &str) -> bool {
        self.snapshot().feature_ready(name)
    }

    pub fn feature_reason(&self, name: &str) -> Option<String> {
        self.snapshot().feature(name).map(|f| f.state.reason())
    }

    pub fn summary_counts(&self) -> (usize, usize, usize, usize) {
        self.snapshot().summary_counts()
    }
}

impl std::fmt::Debug for ReadinessHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ReadinessHandle({} features)", self.snapshot().features.len())
    }
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

    /// Refresh the `custody_readiness_feature_state` gauge and the
    /// `custody_readiness_last_refresh_seconds` timestamp. Called by
    /// the background refresh task after every successful rebuild so
    /// Prometheus scrapes see live state.
    ///
    /// Value mapping matches the gauge docs in `prometheus_metrics`:
    /// disabled=0, ready=1, misconfigured=2, degraded=3.
    pub fn export_prometheus_gauges(&self) {
        use crate::prometheus_metrics::{
            READINESS_FEATURE_STATE, READINESS_LAST_REFRESH_SECONDS,
        };
        for f in &self.features {
            let value: f64 = match f.state {
                FeatureState::Disabled => 0.0,
                FeatureState::Ready => 1.0,
                FeatureState::Misconfigured { .. } => 2.0,
                FeatureState::Degraded { .. } => 3.0,
            };
            READINESS_FEATURE_STATE
                .with_label_values(&[&f.name])
                .set(value);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        READINESS_LAST_REFRESH_SECONDS.set(now);
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

/// `getblockchaininfo` probe for a Bitcoin Core-compatible RPC. Sends
/// the request with any basic-auth credentials embedded in the URL
/// (`http://user:pass@host:port`), which is how operators supply
/// `rpcuser`/`rpcpassword` in this project's config.
///
/// Bitcoin Core returns 401 until you authenticate, so an HTTP 401
/// back from a bare URL is actually a positive signal — the daemon is
/// there and speaking RPC. Treat it as reachable.
pub async fn probe_bitcoin_rpc(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let body = serde_json::json!({
        "jsonrpc": "1.0", "id": "probe", "method": "getblockchaininfo", "params": [],
    });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("getblockchaininfo request: {e}"))?;
    let status = resp.status();
    // 401 = auth required but reachable; treat as OK at the network
    // layer. Operators see the auth gap via other logs.
    if status.as_u16() == 401 {
        return Ok(());
    }
    if !status.is_success() {
        return Err(format!("getblockchaininfo returned HTTP {status}"));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("getblockchaininfo JSON: {e}"))?;
    if let Some(err) = json.get("error") {
        if !err.is_null() {
            return Err(format!("getblockchaininfo RPC error: {err}"));
        }
    }
    if json.get("result").is_none() {
        return Err("getblockchaininfo result missing".into());
    }
    Ok(())
}

/// TronGrid/Java-Tron `/wallet/getnowblock` probe. It's an HTTP POST
/// with an empty body and returns the current block header. Cheapest
/// reachability check for a Tron full node.
pub async fn probe_tron_api(base_url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let url = format!("{}/wallet/getnowblock", base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| format!("getnowblock request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("getnowblock returned HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("getnowblock JSON: {e}"))?;
    // Java-Tron returns `block_header` on success; an error payload
    // contains `Error` or `code`. Require block_header to be present.
    if json.get("block_header").is_none() {
        return Err(format!("getnowblock missing block_header: {json}"));
    }
    Ok(())
}

/// Solana JSON-RPC `getHealth` probe. Returns `"ok"` when the node is
/// caught up; otherwise an error whose message tells you how far
/// behind. Either way, getting a structured response means the
/// endpoint is reachable and speaks RPC.
pub async fn probe_solana_rpc(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "getHealth", "params": [],
    });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("getHealth request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("getHealth returned HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("getHealth JSON: {e}"))?;
    // `result == "ok"` is caught up. An error with a `data.numSlotsBehind`
    // also means reachable but degraded — surface that as the reason so
    // operators know the node is syncing.
    if json.get("result").and_then(|v| v.as_str()) == Some("ok") {
        return Ok(());
    }
    if let Some(err) = json.get("error") {
        return Err(format!("getHealth: {err}"));
    }
    Err(format!("getHealth unexpected response: {json}"))
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

    #[tokio::test]
    async fn probe_bitcoin_rpc_errors_on_unresolvable_host() {
        let res = probe_bitcoin_rpc("http://never-resolves.invalid/").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn probe_tron_api_errors_on_unresolvable_host() {
        let res = probe_tron_api("http://never-resolves.invalid").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn probe_solana_rpc_errors_on_unresolvable_host() {
        let res = probe_solana_rpc("http://never-resolves.invalid/").await;
        assert!(res.is_err());
    }
}
