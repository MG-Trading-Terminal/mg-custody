//! API versioning — support v1/v2, deprecation, migration guides.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// API version info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiVersion {
    pub version: String,
    pub status: VersionStatus,
    pub released_at: DateTime<Utc>,
    pub deprecated_at: Option<DateTime<Utc>>,
    pub sunset_at: Option<DateTime<Utc>>,
    pub changelog_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus { Current, Deprecated, Sunset, Beta }

/// API versions supported.
pub fn supported_versions() -> Vec<ApiVersion> {
    vec![
        ApiVersion { version: "v1".into(), status: VersionStatus::Current, released_at: Utc::now(), deprecated_at: None, sunset_at: None, changelog_url: Some("/v1/openapi.json".into()) },
    ]
}

/// Deprecation notice headers.
pub fn deprecation_headers(version: &str) -> Vec<(String, String)> {
    match version {
        "v0" => vec![
            ("Deprecation".into(), "true".into()),
            ("Sunset".into(), "2026-06-01".into()),
            ("Link".into(), "</v1/openapi.json>; rel=\"successor-version\"".into()),
        ],
        _ => vec![],
    }
}

/// Rate limit response headers.
pub fn rate_limit_headers(remaining: u32, limit: u32, reset_at: u64) -> Vec<(String, String)> {
    vec![
        ("X-RateLimit-Limit".into(), limit.to_string()),
        ("X-RateLimit-Remaining".into(), remaining.to_string()),
        ("X-RateLimit-Reset".into(), reset_at.to_string()),
    ]
}

/// Pagination metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total_count: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
    pub next_cursor: Option<String>,
}

impl PaginationMeta {
    pub fn from_total(page: u32, per_page: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (per_page as f64)).ceil() as u32;
        Self {
            page, per_page, total_count: total, total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
            next_cursor: None,
        }
    }
}

/// Standard API response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_versions_has_v1() {
        let versions = supported_versions();
        assert!(versions.iter().any(|v| v.version == "v1"));
    }

    #[test]
    fn deprecation_headers_v0() {
        let headers = deprecation_headers("v0");
        assert!(!headers.is_empty());
        assert!(headers.iter().any(|(k, _)| k == "Deprecation"));
    }

    #[test]
    fn deprecation_headers_current() {
        let headers = deprecation_headers("v1");
        assert!(headers.is_empty());
    }

    #[test]
    fn rate_limit_headers_format() {
        let headers = rate_limit_headers(95, 100, 1711500000);
        assert_eq!(headers.len(), 3);
    }

    #[test]
    fn pagination_from_total() {
        let meta = PaginationMeta::from_total(1, 50, 200);
        assert_eq!(meta.total_pages, 4);
        assert!(meta.has_next);
        assert!(!meta.has_prev);
    }

    #[test]
    fn pagination_last_page() {
        let meta = PaginationMeta::from_total(4, 50, 200);
        assert!(!meta.has_next);
        assert!(meta.has_prev);
    }

    #[test]
    fn api_response_wrapper() {
        let resp = ApiResponse {
            data: vec!["item1", "item2"],
            pagination: Some(PaginationMeta::from_total(1, 10, 2)),
            warnings: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pagination"));
        assert!(!json.contains("warnings")); // skipped when None
    }
}
