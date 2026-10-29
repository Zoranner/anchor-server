mod config;
mod database;
mod entity;
mod schema;

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use sea_orm::DatabaseConnection;
use serde_json::json;
use tracing::info;

#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::from_env()?;
    info!(
        master_key_file = %config.master_key_file.display(),
        public_url = %config.public_url,
        "anchor-server configuration loaded"
    );
    let db = database::connect(&config).await?;
    schema::initialize(&db).await?;

    let state = AppState { db };
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    info!(address = %config.listen_addr, "anchor-server listening");
    axum::serve(listener, app).await?;
    Ok(())
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
