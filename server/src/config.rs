use std::{env, net::SocketAddr, path::PathBuf};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_CONTENT_DIR: &str = "../content";
const MIN_JWT_SECRET_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for environment variable {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Directory containing the built frontend. Unset in development (Vite serves it).
    pub static_dir: Option<PathBuf>,
    /// Directory holding `rooms/*.yaml`.
    pub content_dir: PathBuf,
    /// Secret used to sign player/host WebSocket tokens.
    pub jwt_secret: String,
    /// Add the `Secure` flag to the session cookie (set in production, behind HTTPS).
    pub cookie_secure: bool,
    /// Running behind one trusted reverse proxy: take the client IP from `X-Forwarded-For`.
    pub trust_proxy: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;

        let bind_raw = env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned());
        let bind_addr = bind_raw.parse().map_err(|_| ConfigError::Invalid {
            name: "BIND_ADDR",
            reason: format!("{bind_raw:?} is not a socket address"),
        })?;

        let static_dir = non_empty_var("STATIC_DIR").map(PathBuf::from);
        let content_dir = non_empty_var("CONTENT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTENT_DIR));

        let jwt_secret = env::var("JWT_SECRET").map_err(|_| ConfigError::Missing("JWT_SECRET"))?;
        if jwt_secret.len() < MIN_JWT_SECRET_LEN {
            return Err(ConfigError::Invalid {
                name: "JWT_SECRET",
                reason: format!("must be at least {MIN_JWT_SECRET_LEN} bytes"),
            });
        }

        Ok(Self {
            database_url,
            bind_addr,
            static_dir,
            content_dir,
            jwt_secret,
            cookie_secure: bool_var("COOKIE_SECURE")?,
            trust_proxy: bool_var("TRUST_PROXY")?,
        })
    }
}

fn non_empty_var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.is_empty())
}

/// An optional boolean variable; unset means false.
fn bool_var(name: &'static str) -> Result<bool, ConfigError> {
    match non_empty_var(name).as_deref() {
        None | Some("false" | "0") => Ok(false),
        Some("true" | "1") => Ok(true),
        Some(other) => Err(ConfigError::Invalid {
            name,
            reason: format!("{other:?} is not true/false"),
        }),
    }
}
