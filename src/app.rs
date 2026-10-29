use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::{
    api::{admin, device, protocol},
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/api/admin/setup", post(admin::setup))
        .route("/api/admin/login", post(admin::login))
        .route("/api/admin/logout", post(admin::logout))
        .route("/api/admin/me", get(admin::me))
        .route("/api/admin/devices", get(admin::list_devices))
        .route("/api/admin/devices/{device_id}", get(admin::device_detail))
        .route(
            "/api/admin/devices/{device_id}/approve",
            post(admin::approve_device),
        )
        .route(
            "/api/admin/devices/{device_id}/reject",
            post(admin::reject_device),
        )
        .route(
            "/api/admin/devices/{device_id}/disable",
            post(admin::disable_device),
        )
        .route(
            "/api/admin/devices/{device_id}/reset-token",
            post(admin::reset_device_token),
        )
        .route("/api/device/register", post(device::register))
        .route("/api/device/status", get(device::status))
        .route("/api/device/heartbeat", post(device::heartbeat))
        .route_layer(middleware::from_fn(protocol::require_protocol));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .merge(api)
        .with_state(state)
}

async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.db.ping().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(error) => {
            tracing::warn!(%error, "database health check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
    }
}
