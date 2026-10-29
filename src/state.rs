use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::{config::Config, keys::SigningKeyStore, rate_limit::RequestLimiter};

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub config: Arc<Config>,
    pub signing_keys: SigningKeyStore,
    pub registration_limiter: Arc<RequestLimiter>,
}
