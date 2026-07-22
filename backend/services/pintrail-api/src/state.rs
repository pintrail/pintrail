use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Settings;
use crate::mail::SharedMailer;
use crate::rate_limit::RateLimiter;
use pintrail_storage::{Storage, StorageConfig};

/// Shared handle passed to every route via `State`.
///
/// Cheap to clone: `PgPool` is internally reference-counted, and `Settings` is
/// small. Storage (S3) lands here in stage 6.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: PgPool,
    pub settings: Settings,
    pub mailer: SharedMailer,
    pub storage: Storage,
    pub limiter: RateLimiter,
}

impl AppState {
    pub async fn connect(settings: Settings) -> anyhow::Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(settings.db_max_connections)
            .acquire_timeout(settings.db_acquire_timeout)
            .connect(&settings.database_url)
            .await?;

        let mailer = crate::mail::build_mailer(&settings).map_err(|e| anyhow::anyhow!("{e}"))?;

        let storage = Storage::connect(&StorageConfig {
            endpoint: settings.s3_endpoint.clone(),
            public_endpoint: settings.s3_public_endpoint.clone(),
            region: settings.s3_region.clone(),
            bucket: settings.s3_bucket.clone(),
            access_key_id: settings.s3_access_key_id.clone(),
            secret_access_key: settings.s3_secret_access_key.clone(),
            presign_ttl: settings.presign_ttl,
        })
        .await?;

        // Fail at startup rather than on an author's first upload.
        storage.check().await?;

        Ok(Self {
            db,
            settings,
            mailer,
            storage,
            limiter: RateLimiter::new(),
        })
    }
}
