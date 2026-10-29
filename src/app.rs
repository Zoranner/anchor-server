use std::path::PathBuf;

use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    routing::{any, get, post},
    Json, Router,
};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    api::{admin, device, error::ApiError, policy, protocol},
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    let web = web_service(state.config.web_dir.clone());
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
        .route(
            "/api/admin/policies",
            get(policy::list_policies).post(policy::create_policy),
        )
        .route(
            "/api/admin/policies/{policy_id}",
            get(policy::policy_detail_endpoint),
        )
        .route(
            "/api/admin/policies/{policy_id}/sign",
            post(policy::sign_policy),
        )
        .route(
            "/api/admin/policies/{policy_id}/assign",
            post(policy::assign_policy),
        )
        .route(
            "/api/admin/policies/{policy_id}/export",
            get(policy::export_policy),
        )
        .route("/api/device/register", post(device::register))
        .route("/api/device/status", get(device::status))
        .route("/api/device/heartbeat", post(device::heartbeat))
        .route("/api/device/keyring", get(policy::device_keyring))
        .route("/api/device/policy", get(policy::device_policy))
        .route_layer(middleware::from_fn(protocol::require_protocol));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .merge(api)
        .route("/api", any(api_not_found))
        .route("/api/{*rest}", any(api_not_found))
        .fallback_service(web)
        .with_state(state)
}

fn web_service(web_dir: PathBuf) -> ServeDir<ServeFile> {
    let index_file = web_dir.join("index.html");
    ServeDir::new(web_dir).fallback(ServeFile::new(index_file))
}

async fn api_not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "API endpoint was not found",
    )
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

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn web_fallback_serves_index_for_spa_routes() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.html"), "spa-index").unwrap();

        let response = web_service(directory.path().to_path_buf())
            .oneshot(
                Request::builder()
                    .uri("/devices")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(Body::new(response.into_body()), 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], b"spa-index");
    }

    #[tokio::test]
    async fn web_service_serves_existing_assets() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.html"), "spa-index").unwrap();
        fs::write(directory.path().join("app.js"), "console.log('ok')").unwrap();

        let response = web_service(directory.path().to_path_buf())
            .oneshot(
                Request::builder()
                    .uri("/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(Body::new(response.into_body()), 1024)
            .await
            .unwrap();
        assert_eq!(&body[..], b"console.log('ok')");
    }
}
