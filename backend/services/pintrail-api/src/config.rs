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
    /// Public base URL used to build links in outbound email. This cannot be
    /// derived from the request, because an attacker controls the Host header
    /// and could redirect a verification link at their own domain.
    pub public_base_url: String,
    /// Which mailer implementation to use. See `crate::mail`.
    pub mailer: String,
    /// Whether to believe `X-Forwarded-For`. See `crate::client_ip` for why
    /// the default is false.
    pub trust_proxy_headers: bool,

    /// Object storage. `s3_endpoint` is None for real S3 and set for MinIO.
    pub s3_endpoint: Option<String>,
    pub s3_region: String,
    pub s3_bucket: String,
    pub s3_access_key_id: String,
    pub s3_secret_access_key: String,
    /// How long presigned upload and download URLs stay valid.
    pub presign_ttl: Duration,
    /// Enforced after upload: a presigned PUT cannot reject an oversized body
    /// while it streams.
    pub max_upload_bytes: i64,
}

impl Settings {
    pub fn verify_link(&self, token: &str) -> String {
        format!(
            "{}/verify-email?token={token}",
            self.public_base_url.trim_end_matches('/')
        )
    }

    pub fn reset_link(&self, token: &str) -> String {
        format!(
            "{}/reset-password?token={token}",
            self.public_base_url.trim_end_matches('/')
        )
    }
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
            public_base_url: optional("PUBLIC_BASE_URL", "http://localhost:8080"),
            mailer: optional("MAILER", "log"),
            trust_proxy_headers: parsed("TRUST_PROXY_HEADERS", false)?,
            s3_endpoint: env::var("S3_ENDPOINT").ok().filter(|v| !v.trim().is_empty()),
            s3_region: optional("S3_REGION", "us-east-1"),
            s3_bucket: required("S3_BUCKET")?,
            s3_access_key_id: required("S3_ACCESS_KEY_ID")?,
            s3_secret_access_key: required("S3_SECRET_ACCESS_KEY")?,
            presign_ttl: Duration::from_secs(parsed("PRESIGN_TTL_SECS", 900)?),
            max_upload_bytes: parsed("MAX_UPLOAD_BYTES", 104_857_600)?,
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
