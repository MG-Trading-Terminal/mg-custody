//! Structured JSON error responses.
//!
//! All API errors return consistent JSON format:
//! {"error": "error_code", "message": "Human-readable description", "request_id": "uuid"}

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use uuid::Uuid;

/// Structured API error response.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    pub request_id: String,
    #[serde(skip)]
    pub status: StatusCode,
}

impl ApiError {
    pub fn not_found(msg: &str) -> Self {
        Self {
            error: "not_found".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::NOT_FOUND,
        }
    }

    pub fn bad_request(msg: &str) -> Self {
        Self {
            error: "bad_request".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn internal(msg: &str) -> Self {
        Self {
            error: "internal_error".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn conflict(msg: &str) -> Self {
        Self {
            error: "conflict".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::CONFLICT,
        }
    }

    pub fn unauthorized(msg: &str) -> Self {
        Self {
            error: "unauthorized".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::UNAUTHORIZED,
        }
    }

    pub fn forbidden(msg: &str) -> Self {
        Self {
            error: "forbidden".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::FORBIDDEN,
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            error: "rate_limited".into(),
            message: "Too many requests. Please retry later.".into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::TOO_MANY_REQUESTS,
        }
    }

    /// 422 Unprocessable Entity — the request was well-formed but failed
    /// semantic validation (e.g. batch with invalid items).
    pub fn unprocessable_entity(msg: &str) -> Self {
        Self {
            error: "unprocessable_entity".into(),
            message: msg.into(),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    /// 503 Service Unavailable — a required feature (scanner, HSM,
    /// provider) isn't ready. The message carries the operator-facing
    /// reason from the readiness report so the caller sees exactly
    /// which config knob or network dependency is missing.
    pub fn feature_unavailable(feature: &str, reason: &str) -> Self {
        Self {
            error: "feature_unavailable".into(),
            message: format!("{feature} is not available: {reason}"),
            request_id: Uuid::new_v4().to_string(),
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// Check a readiness report for a named feature; return `Ok(())` when
/// it's `Ready`, or an `ApiError::feature_unavailable` with the exact
/// reason when not. Handler-level gate for "feature wired vs. feature
/// usable" mismatches. Snapshots the current report under a cheap
/// read lock — the background refresh task can swap the inner
/// report without disturbing in-flight handlers.
pub fn require_feature(
    readiness: &custody_common::readiness::ReadinessHandle,
    feature: &str,
) -> Result<(), ApiError> {
    let snap = readiness.snapshot();
    if snap.feature_ready(feature) {
        return Ok(());
    }
    let reason = snap
        .feature(feature)
        .map(|f| f.state.reason())
        .unwrap_or_else(|| format!("{feature} not reported in readiness"));
    Err(ApiError::feature_unavailable(feature, &reason))
}

/// Map a `Chain` to its readiness feature name and require it to be
/// `Ready`. Used by endpoints that need an actively-monitored chain
/// (address generation — can't receive a deposit we don't scan;
/// withdrawal submission — can't broadcast without an RPC).
pub fn require_chain_ready(
    readiness: &custody_common::readiness::ReadinessHandle,
    chain: custody_common::Chain,
) -> Result<(), ApiError> {
    use custody_common::Chain;
    let feature = match chain {
        Chain::Bitcoin => "scanner.bitcoin",
        Chain::Ethereum => "scanner.ethereum",
        Chain::Tron => "scanner.tron",
        Chain::Solana => "scanner.solana",
    };
    require_feature(readiness, feature)
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        (status, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_serialization() {
        let err = ApiError::not_found("Withdrawal not found");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("not_found"));
        assert!(json.contains("Withdrawal not found"));
        assert!(json.contains("request_id"));
    }

    #[test]
    fn all_error_types() {
        assert_eq!(ApiError::not_found("").status, StatusCode::NOT_FOUND);
        assert_eq!(ApiError::bad_request("").status, StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::internal("").status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ApiError::conflict("").status, StatusCode::CONFLICT);
        assert_eq!(ApiError::unauthorized("").status, StatusCode::UNAUTHORIZED);
        assert_eq!(ApiError::rate_limited().status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(ApiError::unprocessable_entity("").status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
