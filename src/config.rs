use std::{fmt, path::PathBuf};

use anyhow::{bail, Context};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub data_dir: PathBuf,
    pub master_key_file: PathBuf,
    pub listen_addr: String,
    pub public_url: String,
    pub setup_token: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("database_url", &"[redacted]")
            .field("database_max_connections", &self.database_max_connections)
            .field("data_dir", &self.data_dir)
            .field("master_key_file", &self.master_key_file)
            .field("listen_addr", &self.listen_addr)
            .field("public_url", &self.public_url)
            .field(
                "setup_token",
                &if self.setup_token.is_some() {
                    "configured"
                } else {
                    "unset"
                },
            )
            .finish()
    }
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

        let setup_token = match std::env::var("ANCHOR_SETUP_TOKEN") {
            Ok(value) if !value.trim().is_empty() => {
                if value.len() < 32 || value.len() > 1024 {
                    bail!("ANCHOR_SETUP_TOKEN must be between 32 and 1024 bytes");
                }
                Some(value)
            }
            _ => None,
        };

        Ok(Self {
            database_url,
            database_max_connections,
            data_dir: std::env::var_os("ANCHOR_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./data")),
            master_key_file: std::env::var_os("ANCHOR_MASTER_KEY_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./data/master.key")),
            listen_addr: std::env::var("ANCHOR_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into()),
            public_url: std::env::var("ANCHOR_PUBLIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            setup_token,
        })
    }
}
