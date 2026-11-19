use std::{env, net::SocketAddr, path::PathBuf};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for environment variable {name}: {value:?}")]
    Invalid { name: &'static str, value: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Directory containing the built frontend. Unset in development (Vite serves it).
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;

        let bind_raw = env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned());
        let bind_addr = bind_raw.parse().map_err(|_| ConfigError::Invalid {
            name: "BIND_ADDR",
            value: bind_raw.clone(),
        })?;

        let static_dir = env::var_os("STATIC_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);

        Ok(Self {
            database_url,
            bind_addr,
            static_dir,
        })
    }
}
