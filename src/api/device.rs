use std::{net::SocketAddr, time::Duration};

use anchor_protocol::{
    DeviceStatusResponse, HeartbeatRequest, RegisterDeviceRequest, RegisterDeviceResponse,
};
use axum::{
    extract::{rejection::JsonRejection, ConnectInfo, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use uuid::Uuid;

use crate::{
    domain::device::{self, DeviceError},
    rate_limit::client_ip,
    state::AppState,
};

use super::error::ApiError;

const REGISTRATION_WINDOW: Duration = Duration::from_secs(10 * 60);
const REGISTRATION_LIMIT_PER_IP: usize = 30;
const REGISTRATION_LIMIT_PER_INSTALLATION: usize = 5;

pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    payload: Result<Json<RegisterDeviceRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<RegisterDeviceResponse>), ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    let installation_id = Uuid::parse_str(&request.installation_id)
        .map_err(|_| ApiError::bad_request("installation_id must be a UUID"))?;
    let ip = client_ip(&headers, peer, state.config.trust_proxy_headers);
    if !state.registration_limiter.check(
        format!("ip:{ip}"),
        REGISTRATION_LIMIT_PER_IP,
        REGISTRATION_WINDOW,
    ) || !state.registration_limiter.check(
        format!("installation:{installation_id}"),
        REGISTRATION_LIMIT_PER_INSTALLATION,
        REGISTRATION_WINDOW,
    ) {
        return Err(rate_limited());
    }

    let registered = device::register_device(&state.db, &request)
        .await
        .map_err(map_device_error)?;
    Ok((
        StatusCode::CREATED,
        Json(RegisterDeviceResponse {
            device_id: registered.device_id.to_string(),
            status: registered.status,
            device_token: Some(registered.token),
        }),
    ))
}

pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DeviceStatusResponse>, ApiError> {
    let device = authenticate_device(&state, &headers).await?;
    Ok(Json(DeviceStatusResponse {
        status: device::status_of(&device).map_err(map_device_error)?,
        approved_at: device.approved_at,
    }))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<HeartbeatRequest>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let Json(request) = payload.map_err(ApiError::from)?;
    let device = authenticate_device(&state, &headers).await?;
    device::heartbeat(
        &state.db,
        &device,
        &request.client_version,
        &request.inventory_hash,
    )
    .await
    .map_err(map_device_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn authenticate_device(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::entity::device::Model, ApiError> {
    let token = bearer_token(headers)?;
    device::find_device_by_token(&state.db, &token)
        .await
        .map_err(map_device_error)
}

fn bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(token_invalid)?;
    let (scheme, token) = value.split_once(' ').ok_or_else(token_invalid)?;
    if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() || token.contains(' ') {
        return Err(token_invalid());
    }
    Ok(token.into())
}

fn token_invalid() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        "token_invalid",
        "device token is invalid",
    )
}

fn rate_limited() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "registration rate limit exceeded",
    )
}

pub(super) fn map_device_error(error: DeviceError) -> ApiError {
    match error {
        DeviceError::InvalidRequest(message) => ApiError::bad_request(message),
        DeviceError::AlreadyActive => {
            ApiError::conflict("already_active", "device is already active")
        }
        DeviceError::Rejected => ApiError::new(
            StatusCode::FORBIDDEN,
            "device_rejected",
            "device registration was rejected",
        ),
        DeviceError::Disabled => ApiError::new(
            StatusCode::FORBIDDEN,
            "device_disabled",
            "device is disabled",
        ),
        DeviceError::Pending => ApiError::new(
            StatusCode::FORBIDDEN,
            "device_pending",
            "device approval is pending",
        ),
        DeviceError::NotFound => ApiError::new(
            StatusCode::NOT_FOUND,
            "device_not_found",
            "device was not found",
        ),
        DeviceError::TokenInvalid => token_invalid(),
        DeviceError::InvalidState => ApiError::conflict(
            "invalid_device_state",
            "device state does not allow this operation",
        ),
        DeviceError::InventoryChanged => ApiError::conflict(
            "inventory_changed",
            "device inventory changed; register the full inventory again",
        ),
        DeviceError::Internal(error) => ApiError::internal(error),
        DeviceError::Database(error) => ApiError::internal(error),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn bearer_token_requires_the_authorization_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc"),
        );
        assert_eq!(bearer_token(&headers).unwrap(), "abc");

        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert!(bearer_token(&headers).is_err());

        headers.remove(header::AUTHORIZATION);
        assert!(bearer_token(&headers).is_err());
    }
}
