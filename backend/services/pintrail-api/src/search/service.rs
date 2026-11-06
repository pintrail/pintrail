use sqlx::PgPool;

use super::{SearchMode, SearchResponse};
use crate::error::{AppError, AppResult};

#[cfg(feature = "semantic-search")]
use {
    super::SearchResult,
    pintrail_embeddings::{
        retrieval, storage, EmbeddingError, EmbeddingProvider, InputKind, ModelInfo,
    },
    std::{
        sync::{Arc, Mutex},
        time::Duration,
    },
    tokio::sync::Semaphore,
};

#[derive(Clone)]
pub struct SearchService {
    #[cfg(feature = "semantic-search")]
    provider: Arc<Mutex<Box<dyn EmbeddingProvider>>>,
    #[cfg(feature = "semantic-search")]
    model: ModelInfo,
    #[cfg(feature = "semantic-search")]
    slots: Arc<Semaphore>,
    #[cfg(feature = "semantic-search")]
    deadline: Duration,
}

impl std::fmt::Debug for SearchService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchService").finish_non_exhaustive()
    }
}

impl SearchService {
    pub async fn configured(db: &PgPool, enabled: bool) -> anyhow::Result<Option<Self>> {
        if !enabled {
            return Ok(None);
        }
        #[cfg(not(feature = "semantic-search"))]
        {
            let _ = db;
            anyhow::bail!(
                "SEARCH_ENABLED requires rebuilding pintrail-api with --features semantic-search"
            );
        }
        #[cfg(feature = "semantic-search")]
        {
            let installed: bool =
                sqlx::query_scalar("SELECT to_regclass('search_embeddings') IS NOT NULL")
                    .fetch_one(db)
                    .await?;
            anyhow::ensure!(installed, "Search embedding schema is missing; install the reviewed pgvector schema before setting SEARCH_ENABLED");
            let config = pintrail_embeddings::Config::from_env()?;
            let provider = tokio::task::spawn_blocking(move || {
                pintrail_embeddings::LocalProvider::new(config)
            })
            .await??;
            let service = Self::from_provider(provider)?;
            tracing::info!(model=%service.model.name, dimension=service.model.dimension, "search model loaded once");
            Ok(Some(service))
        }
    }

    #[cfg(feature = "semantic-search")]
    pub(super) fn from_provider(
        provider: impl EmbeddingProvider + 'static,
    ) -> anyhow::Result<Self> {
        let model = provider.model_info().clone();
        // Fail at initialization if vector storage cannot identify this model.
        storage::fingerprint(&model)?;
        Ok(Self {
            provider: Arc::new(Mutex::new(Box::new(provider))),
            model,
            slots: Arc::new(Semaphore::new(1)),
            deadline: Duration::from_secs(10),
        })
    }

    pub async fn query(
        &self,
        db: &PgPool,
        query: String,
        limit: i64,
        mode: SearchMode,
    ) -> AppResult<SearchResponse> {
        #[cfg(not(feature = "semantic-search"))]
        {
            let _ = (db, query, limit, mode);
            Err(AppError::ServiceUnavailable("search is not enabled"))
        }
        #[cfg(feature = "semantic-search")]
        {
            // No unbounded queue of CPU jobs. Hold the permit through retrieval.
            // The blocking task keeps its own reference so request cancellation
            // or timeout cannot release capacity while inference is still running.
            let permit =
                Arc::new(self.slots.clone().try_acquire_owned().map_err(|_| {
                    AppError::ServiceUnavailable("search is busy; try again shortly")
                })?);
            let operation = async {
                let vector = if mode == SearchMode::Keyword {
                    None
                } else {
                    let cpu_permit = permit.clone();
                    let provider = self.provider.clone();
                    let text = query.clone();
                    Some(
                        tokio::task::spawn_blocking(move || {
                            let _keep_capacity = cpu_permit;
                            let mut provider = provider.lock().map_err(|_| {
                                EmbeddingError::Inference("embedding session is unavailable".into())
                            })?;
                            let mut vectors = provider.embed(&[&text], InputKind::Query)?;
                            if vectors.len() != 1 {
                                return Err(EmbeddingError::InvalidOutput(
                                    "expected one query vector".into(),
                                ));
                            }
                            Ok(vectors.remove(0))
                        })
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?
                        .map_err(embedding_error)?,
                    )
                };

                let mut tx = db.begin().await?;
                // Both candidate lists must see the same document/publication
                // snapshot, even when an author edits during retrieval.
                sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
                    .execute(&mut *tx)
                    .await?;
                // A client-side timeout alone does not stop PostgreSQL work.
                sqlx::query("SET LOCAL statement_timeout = '3s'")
                    .execute(&mut *tx)
                    .await?;
                let hits = match mode {
                    SearchMode::Keyword => retrieval::keyword_on(&mut tx, &query, limit).await,
                    SearchMode::Semantic => storage::search_on(
                        &mut tx,
                        &self.model,
                        vector.as_deref().expect("semantic query vector"),
                        limit,
                    )
                    .await
                    .map(retrieval::from_semantic),
                    SearchMode::Hybrid => {
                        retrieval::hybrid_on(
                            &mut tx,
                            &self.model,
                            vector.as_deref().expect("hybrid query vector"),
                            &query,
                            limit,
                        )
                        .await
                    }
                }
                .map_err(storage_error)?;
                tx.commit().await?;
                let results: Vec<_> = hits
                    .into_iter()
                    .map(|hit| SearchResult {
                        artifact_id: hit.artifact_id,
                        name: hit.name,
                        is_topic: hit.is_topic,
                        snippet: hit.snippet,
                        score: hit.score,
                        semantic_score: hit.semantic_score,
                        keyword_score: hit.keyword_score,
                    })
                    .collect();
                Ok(SearchResponse {
                    query,
                    mode,
                    model: (mode != SearchMode::Keyword).then(|| self.model.name.clone()),
                    score_kind: mode.score_kind(),
                    count: results.len(),
                    results,
                })
            };
            let result = tokio::time::timeout(self.deadline, operation)
                .await
                .map_err(|_| AppError::ServiceUnavailable("search timed out; try again shortly"))?;
            drop(permit);
            result
        }
    }
}

#[cfg(feature = "semantic-search")]
fn embedding_error(error: EmbeddingError) -> AppError {
    match error {
        EmbeddingError::InputTooLong { limit, .. } => AppError::BadRequest(format!(
            "q exceeds the embedding model's {limit}-token input limit; shorten the question"
        )),
        EmbeddingError::EmptyInput { .. } => AppError::BadRequest("q must contain text".into()),
        other => AppError::Internal(other.into()),
    }
}

#[cfg(feature = "semantic-search")]
fn storage_error(error: storage::StorageError) -> AppError {
    if let storage::StorageError::Database(sqlx::Error::Database(ref db)) = error {
        if db.code().as_deref() == Some("57014") {
            return AppError::ServiceUnavailable("search timed out; try again shortly");
        }
    }
    AppError::Internal(error.into())
}

#[cfg(all(test, feature = "semantic-search"))]
mod tests;
