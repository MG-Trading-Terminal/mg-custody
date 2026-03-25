//! Request ID middleware.
//!
//! Extracts X-Request-ID from incoming requests or generates a new UUID.
//! Propagates the ID to all tracing spans and returns it in the response.

use axum::{
    body::Body,
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// Header name for request ID.
pub const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Middleware that ensures every request has a unique ID.
///
/// - If X-Request-ID header is present, use it.
/// - Otherwise, generate a new UUID.
/// - Add the ID to the response headers.
/// - Create a tracing span with the request ID.
pub async fn request_id_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request.headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let span = tracing::info_span!(
        "request",
        rid = %request_id,
        method = %method,
        path = %path,
    );

    let _guard = span.enter();

    let mut response = next.run(request).await;

    // Add request ID to response
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, val);
    }

    response
}
