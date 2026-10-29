use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::config::Config;

pub async fn connect(config: &Config) -> anyhow::Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(config.database_url.clone());
    options
        .max_connections(config.database_max_connections)
        .sqlx_logging(false);
    Ok(Database::connect(options).await?)
}
