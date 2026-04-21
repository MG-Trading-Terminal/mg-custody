//! Application state shared across all route handlers.
//!
//! Contains all the service dependencies needed by the API handlers,
//! wrapped in Arc for shared ownership across async tasks.

use std::sync::Arc;

use custody_address_factory::{AddressAllocator, SeedBytes};
use custody_common::email::EmailSender;
use custody_common::emergency::FreezeFlag;
use custody_common::events::EventBroadcaster;
use custody_common::readiness::ReadinessHandle;
use custody_common::readiness_ack::IncidentAcks;
use custody_common::readiness_chaos::ChaosOverrides;
use custody_compliance::screening::ScreeningService;
use custody_compliance::travel_rule::NotabeneProvider;
use custody_db::CustodyDb;
use custody_withdrawal_pipeline::WithdrawalPipeline;

use crate::rate_limit::UserRateLimiter;

/// Shared application state passed to all route handlers via axum's State.
#[derive(Clone)]
pub struct AppState {
    pub db: CustodyDb,
    pub address_allocator: Arc<AddressAllocator>,
    pub seed: Arc<SeedBytes>,
    pub withdrawal_pipeline: Arc<WithdrawalPipeline>,
    pub event_broadcaster: Arc<EventBroadcaster>,
    /// Per-user sliding window rate limiter (shared across all request handlers).
    pub user_rate_limiter: Arc<UserRateLimiter>,
    /// Emergency freeze flag — when true all outbound operations are halted.
    pub frozen: FreezeFlag,
    /// Outbound email transport. Console-printing in dev (`console`), real
    /// SMTP when wired. Never dispatch email via raw `tracing` — use this.
    pub email: Arc<EmailSender>,
    /// Travel Rule counterparty-VASP dispatcher (Notabene). `None`
    /// when no API key is configured — `submit_travel_rule` still
    /// persists the row, but flags the record `Required` (awaiting
    /// out-of-band exchange) instead of `Submitted`.
    pub travel_rule_provider: Option<Arc<NotabeneProvider>>,
    /// Feature readiness handle — background task refreshes the
    /// underlying report periodically so handlers see live state, not
    /// just the boot-time snapshot. Handlers gating on integrations
    /// should check `readiness.feature_ready("foo")` and respond with
    /// 503 + the current reason when not ready. `GET
    /// /v1/health/features` serialises the latest snapshot.
    pub readiness: ReadinessHandle,
    /// In-memory chaos overrides. Admin endpoints mutate; the
    /// refresh task applies after each probe so fake-Degraded
    /// feature states propagate through the whole alerting
    /// pipeline (transition detection, webhook, history row).
    /// Cleared on service restart — no persistence, because a
    /// forgotten chaos override in production is itself an incident.
    pub chaos_overrides: ChaosOverrides,
    /// In-memory "I'm on it" acknowledgments per degraded feature.
    /// Cleared on service restart and pruned when the refresh task
    /// sees the feature return to Ready.
    pub incident_acks: IncidentAcks,
    /// Per-user cooldown for admin-driven state mutations (chaos
    /// inject, silence create). 10s-per-op debounce so an accidental
    /// shell loop can't paint the audit log or page oncall every
    /// tick.
    pub admin_cooldown: Arc<crate::admin_cooldown::AdminCooldown>,
    /// AML screening service — wired when CUSTODY_CHAINALYSIS_API_KEY
    /// or CUSTODY_ELLIPTIC_API_KEY is set. `None` means no single-
    /// address screening endpoint; the withdrawal pipeline does its
    /// own screening regardless of this field (the pipeline keeps
    /// its own Arc<ScreeningService>).
    ///
    /// Handler-side uses this for the dashboard's
    /// "screen this address now" button (operations_routes::screen_address).
    pub screening_service: Option<Arc<ScreeningService>>,
}
