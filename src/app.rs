use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::{api::admin, state::AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/api/admin/setup", post(admin::setup))
        .route("/api/admin/login", post(admin::login))
        .route("/api/admin/logout", post(admin::logout))
        .route("/api/admin/me", get(admin::me))
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
