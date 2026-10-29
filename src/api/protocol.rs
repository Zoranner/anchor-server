use anchor_protocol::PROTOCOL_VERSION;
use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};

use super::error::ApiError;

pub async fn require_protocol(request: Request, next: Next) -> Result<Response, ApiError> {
    let protocol = request
        .headers()
        .get("x-anchor-protocol")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok());
    if protocol != Some(PROTOCOL_VERSION) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "protocol_version_unsupported",
            "X-Anchor-Protocol must be 1",
        ));
    }
    Ok(next.run(request).await)
}
