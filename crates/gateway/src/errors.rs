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
    }
}
