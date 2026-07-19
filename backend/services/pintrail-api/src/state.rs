use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::Settings;

/// Shared handle passed to every route via `State`.
///
/// Cheap to clone: `PgPool` is internally reference-counted, and `Settings` is
/// small. Storage (S3) lands here in stage 6.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: PgPool,
    pub settings: Settings,
}

impl AppState {
    pub async fn connect(settings: Settings) -> anyhow::Result<Self> {
        let db = PgPoolOptions::new()
            .max_connections(settings.db_max_connections)
            .acquire_timeout(settings.db_acquire_timeout)
            .connect(&settings.database_url)
            .await?;

        Ok(Self { db, settings })
    }
}
