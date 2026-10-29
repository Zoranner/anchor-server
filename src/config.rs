use std::path::PathBuf;

use anyhow::{bail, Context};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub master_key_file: PathBuf,
    pub listen_addr: String,
    pub public_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        if !database_url.starts_with("postgres://") && !database_url.starts_with("postgresql://") {
            bail!("DATABASE_URL must use postgres:// or postgresql://");
        }

        let database_max_connections = std::env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .map(|value| {
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|limit| *limit > 0)
                    .with_context(|| format!("invalid DATABASE_MAX_CONNECTIONS: {value}"))
            })
            .transpose()?
            .unwrap_or(10);

        Ok(Self {
            database_url,
            database_max_connections,
            master_key_file: std::env::var_os("ANCHOR_MASTER_KEY_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./data/master.key")),
            listen_addr: std::env::var("ANCHOR_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into()),
            public_url: std::env::var("ANCHOR_PUBLIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
        })
    }
}
