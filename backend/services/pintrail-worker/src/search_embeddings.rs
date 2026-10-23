//! Thin worker integration. The feature and runtime setting are opt-in until
//! the database extension rollout is agreed; ordinary workers stay unchanged.
use anyhow::Context;
use sqlx::PgPool;

#[cfg(feature = "semantic-search")]
pub struct Worker {
    indexer: pintrail_embeddings::indexer::Indexer<pintrail_embeddings::LocalProvider>,
}
#[cfg(not(feature = "semantic-search"))]
pub struct Worker;

impl Worker {
    pub async fn configured(db: &PgPool) -> anyhow::Result<Option<Self>> {
        let enabled: bool = std::env::var("SEARCH_EMBEDDINGS_ENABLED")
            .unwrap_or_else(|_| "false".into())
            .parse()
            .context("SEARCH_EMBEDDINGS_ENABLED must be true or false")?;
        if enabled {
            Ok(Some(Self::load(db).await?))
        } else {
            Ok(None)
        }
    }

    #[cfg(feature = "semantic-search")]
    pub async fn load(db: &PgPool) -> anyhow::Result<Self> {
        use pintrail_embeddings::{indexer::Indexer, Config, LocalProvider};
        let installed: bool =
            sqlx::query_scalar("SELECT to_regclass('search_embedding_jobs') IS NOT NULL")
                .fetch_one(db)
                .await?;
        anyhow::ensure!(installed,"Embedding schema is missing; explicitly initialize the search lab before enabling embeddings");
        let config = Config::from_env()?;
        let provider = tokio::task::spawn_blocking(move || LocalProvider::new(config))
            .await
            .context("model initialization task failed")??;
        let indexer = Indexer::new(provider)?;
        tracing::info!(model=%indexer.model_info().name,dimension=indexer.model_info().dimension,"local embedding model loaded once");
        Ok(Self { indexer })
    }

    #[cfg(not(feature = "semantic-search"))]
    pub async fn load(_db: &PgPool) -> anyhow::Result<Self> {
        anyhow::bail!(
            "Rebuild pintrail-worker with --features semantic-search to enable local embeddings"
        )
    }

    #[cfg(feature = "semantic-search")]
    pub async fn poll_once(&mut self, db: &PgPool, batch: i64) -> anyhow::Result<()> {
        let stats = self.indexer.poll_once(db, batch).await?;
        if stats.queued
            + stats.indexed
            + stats.skipped
            + stats.retrying
            + stats.failed
            + stats.superseded
            > 0
        {
            tracing::info!(?stats, "embedding cycle finished");
        }
        Ok(())
    }

    #[cfg(not(feature = "semantic-search"))]
    pub async fn poll_once(&mut self, _db: &PgPool, _batch: i64) -> anyhow::Result<()> {
        anyhow::bail!("semantic-search feature is not enabled")
    }
}
