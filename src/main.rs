mod api;
mod app;
mod auth;
mod config;
mod database;
mod entity;
mod keys;
mod schema;
mod state;

use std::sync::Arc;

use tracing::info;

use crate::{keys::MasterKey, keys::SigningKeyStore, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::from_env()?;
    info!(
        data_dir = %config.data_dir.display(),
        master_key_file = %config.master_key_file.display(),
        public_url = %config.public_url,
        "anchor-server configuration loaded"
    );
    let db = database::connect(&config).await?;
    schema::initialize(&db).await?;

    let master_key = MasterKey::load(&config.master_key_file)?;
    let signing_keys = SigningKeyStore::new(config.data_dir.join("signing-keys"), master_key);
    let listen_addr = config.listen_addr.clone();
    let state = AppState {
        db,
        config: Arc::new(config),
        signing_keys,
    };
    let app = app::router(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!(address = %listen_addr, "anchor-server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
