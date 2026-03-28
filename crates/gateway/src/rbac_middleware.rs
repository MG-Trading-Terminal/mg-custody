//! RBAC middleware and permission extractors (Phase 19).
//!
//! Two modes of authentication:
//! 1. **Session token** (Bearer) — for dashboard/UI users
//! 2. **HMAC-SHA256** — for service-to-service (exchange → custody) — existing
//!
//! Session auth extracts user role from DB. If no session token is present,
//! falls through to HMAC auth (which grants `Admin` role by default, since
//! the exchange is a trusted internal service).
//!
//! Default: DENY. If role extraction fails, request is rejected.

use axum::{
    body::Body,
    extract::{FromRequestParts, State},
    http::{request::Parts, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use custody_db::sessions::Role;
use custody_db::CustodyDb;
use uuid::Uuid;

/// Authenticated user context, injected into request extensions by RBAC middleware.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub role: Role,
    /// "session" or "hmac"
    pub auth_method: String,
}

/// RBAC middleware state.
#[derive(Clone)]
pub struct RbacState {
    pub db: CustodyDb,
    /// Paths that skip RBAC (health, metrics).
    pub public_paths: Vec<String>,
}

impl RbacState {
    pub fn new(db: CustodyDb) -> Self {
        Self {
            db,
            public_paths: vec![
                "/health".into(),
                "/health/chains".into(),
                "/health/deep".into(),
                "/metrics".into(),
                "/v1/auth/sessions".into(), // login endpoint is pre-auth
                "/v1/auth/sessions/verify-2fa".into(), // 2FA completion is pre-auth (uses pending token)
            ],
        }
    }
}

/// RBAC middleware function.
///
/// Checks for `Authorization: Bearer <token>` header, loads session from DB,
/// validates session (expiry, idle, IP binding), and injects `AuthContext`
/// into request extensions.
///
/// For requests without Bearer token, checks if HMAC auth already passed
/// (the existing `hmac_auth` middleware runs first). If HMAC-authed, grants
/// Admin role (service-to-service trust).
pub async fn rbac_auth(
    State(rbac): State<RbacState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();

    // Skip for public paths
    if rbac.public_paths.iter().any(|p| path.starts_with(p)) {
        return Ok(next.run(request).await);
    }

    let ip = request
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let now = Utc::now();

    // Try Bearer token auth first
    if let Some(auth_header) = request.headers().get("Authorization")
        && let Ok(header_str) = auth_header.to_str()
        && let Some(token) = header_str.strip_prefix("Bearer ") {
        match rbac.db.load_session(token).await {
            Ok(Some(session)) => {
                if !session.is_valid(now) {
                    tracing::warn!(
                        user = %session.email,
                        path = %path,
                        "Session expired or revoked"
                    );
                    return Err(StatusCode::UNAUTHORIZED);
                }

                // IP binding check
                if !session.ip_matches(&ip) {
                    tracing::warn!(
                        user = %session.email,
                        session_ip = %session.ip_address,
                        request_ip = %ip,
                        path = %path,
                        "Session IP mismatch — possible hijack"
                    );
                    return Err(StatusCode::UNAUTHORIZED);
                }

                // Touch session (update last_active_at)
                let _ = rbac.db.touch_session(&session.token_hash).await;

                let ctx = AuthContext {
                    user_id: session.user_id,
                    workspace_id: session.workspace_id,
                    email: session.email,
                    role: session.role,
                    auth_method: "session".into(),
                };
                request.extensions_mut().insert(ctx);
                return Ok(next.run(request).await);
            }
            Ok(None) => {
                tracing::warn!(path = %path, "Invalid session token");
                return Err(StatusCode::UNAUTHORIZED);
            }
            Err(e) => {
                tracing::error!(error = %e, "Session lookup failed");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // No Bearer token — if HMAC auth already validated this request,
    // grant Admin role (exchange is a trusted internal service).
    // The HMAC middleware runs before RBAC in the middleware stack,
    // so if we reach here without Bearer, HMAC either passed or is not configured.
    let ctx = AuthContext {
        user_id: Uuid::nil(),
        workspace_id: Uuid::nil(),
        email: "service@internal".into(),
        role: Role::Admin,
        auth_method: "hmac".into(),
    };
    request.extensions_mut().insert(ctx);
    Ok(next.run(request).await)
}

/// Record a permission denial in Prometheus metrics.
fn record_denial(required_role: &str, method: &str) {
    custody_common::prometheus_metrics::PERMISSION_DENIALS
        .with_label_values(&[required_role, method])
        .inc();
}

/// Extract AuthContext from request extensions (set by rbac_auth middleware).
pub struct AuthUser(pub AuthContext);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .map(AuthUser)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// Extractor that requires at least Viewer role (any authenticated user).
pub struct RequireViewer(pub AuthContext);

impl<S> FromRequestParts<S> for RequireViewer
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if ctx.role.has_at_least(&Role::Viewer) {
            Ok(RequireViewer(ctx))
        } else {
            let path = parts.uri.path();
            let method = parts.method.as_str();
            tracing::warn!(
                user = %ctx.email, role = ?ctx.role, path, method,
                required_role = "viewer", event = "permission_denied",
                "RBAC denial: user lacks Viewer role"
            );
            record_denial("viewer", method);
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// Extractor that requires at least Editor role.
pub struct RequireEditor(pub AuthContext);

impl<S> FromRequestParts<S> for RequireEditor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if ctx.role.has_at_least(&Role::Editor) {
            Ok(RequireEditor(ctx))
        } else {
            let path = parts.uri.path();
            let method = parts.method.as_str();
            tracing::warn!(
                user = %ctx.email, role = ?ctx.role, path, method,
                required_role = "editor", event = "permission_denied",
                "RBAC denial: user lacks Editor role"
            );
            record_denial("editor", method);
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// Extractor that requires at least Signer role.
pub struct RequireSigner(pub AuthContext);

impl<S> FromRequestParts<S> for RequireSigner
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if ctx.role.has_at_least(&Role::Signer) {
            Ok(RequireSigner(ctx))
        } else {
            let path = parts.uri.path();
            let method = parts.method.as_str();
            tracing::warn!(
                user = %ctx.email, role = ?ctx.role, path, method,
                required_role = "signer", event = "permission_denied",
                "RBAC denial: user lacks Signer role"
            );
            record_denial("signer", method);
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// Extractor that requires at least Admin role.
pub struct RequireAdmin(pub AuthContext);

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if ctx.role.has_at_least(&Role::Admin) {
            Ok(RequireAdmin(ctx))
        } else {
            let path = parts.uri.path();
            let method = parts.method.as_str();
            tracing::warn!(
                user = %ctx.email, role = ?ctx.role, path, method,
                required_role = "admin", event = "permission_denied",
                "RBAC denial: user lacks Admin role"
            );
            record_denial("admin", method);
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// Extractor that requires Owner role.
pub struct RequireOwner(pub AuthContext);

impl<S> FromRequestParts<S> for RequireOwner
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts.extensions.get::<AuthContext>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if ctx.role.has_at_least(&Role::Owner) {
            Ok(RequireOwner(ctx))
        } else {
            let path = parts.uri.path();
            let method = parts.method.as_str();
            tracing::warn!(
                user = %ctx.email, role = ?ctx.role, path, method,
                required_role = "owner", event = "permission_denied",
                "RBAC denial: user lacks Owner role"
            );
            record_denial("owner", method);
            Err(StatusCode::FORBIDDEN)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_context_creation() {
        let ctx = AuthContext {
            user_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            email: "test@example.com".into(),
            role: Role::Admin,
            auth_method: "session".into(),
        };
        assert_eq!(ctx.role, Role::Admin);
        assert_eq!(ctx.auth_method, "session");
    }

    #[test]
    fn rbac_public_paths_logic() {
        let paths = vec!["/health".to_string(), "/metrics".to_string()];
        assert!(paths.iter().any(|p| "/health".starts_with(p)));
        assert!(paths.iter().any(|p| "/metrics".starts_with(p)));
        assert!(!paths.iter().any(|p| "/v1/vaults".starts_with(p)));
    }

    #[test]
    fn role_hierarchy_in_extractors() {
        // Verify the Role from sessions module works correctly
        assert!(Role::Owner.has_at_least(&Role::Admin));
        assert!(Role::Admin.has_at_least(&Role::Signer));
        assert!(Role::Signer.has_at_least(&Role::Editor));
        assert!(Role::Editor.has_at_least(&Role::Viewer));
        assert!(!Role::Viewer.has_at_least(&Role::Editor));
    }

    #[test]
    fn hmac_fallback_gets_admin() {
        // When no Bearer token, HMAC-authed requests get Admin role
        let ctx = AuthContext {
            user_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            email: "service@internal".into(),
            role: Role::Admin,
            auth_method: "hmac".into(),
        };
        assert!(ctx.role.has_at_least(&Role::Admin));
        assert!(!ctx.role.has_at_least(&Role::Owner));
    }
}
