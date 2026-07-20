use std::env;
use std::time::Duration;

/// Everything the API needs from the environment, read once at startup.
///
/// Deliberately fails fast: a missing `DATABASE_URL` should kill the process
/// on boot rather than surface as a connection error on the first request.
#[derive(Debug, Clone)]
pub struct Settings {
    pub database_url: String,
    pub bind_addr: String,
    pub db_max_connections: u32,
    pub db_acquire_timeout: Duration,
    /// Whether to set `Secure` on session cookies. Defaults to true; only a
    /// local http development server has any business turning it off.
    pub cookie_secure: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("required environment variable {0} is not set")]
    Missing(&'static str),
    #[error("environment variable {name} has invalid value {value:?}: {reason}")]
    Invalid {
        name: &'static str,
        value: String,
        reason: String,
    },
}

impl Settings {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind_addr: optional("BIND_ADDR", "0.0.0.0:8080"),
            db_max_connections: parsed("DB_MAX_CONNECTIONS", 10)?,
            db_acquire_timeout: Duration::from_secs(parsed("DB_ACQUIRE_TIMEOUT_SECS", 5)?),
            cookie_secure: parsed("COOKIE_SECURE", true)?,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn optional(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parsed<T>(name: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(raw) if raw.trim().is_empty() => Ok(default),
        Ok(raw) => raw.trim().parse().map_err(|e: T::Err| ConfigError::Invalid {
            name,
            value: raw,
            reason: e.to_string(),
        }),
    }
}
