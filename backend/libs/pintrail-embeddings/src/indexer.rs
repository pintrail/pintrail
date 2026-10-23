//! Automatic model-aware indexing. One blocking inference at a time per Indexer;
//! no database transaction stays open while the model computes.
use crate::{
    storage::{self, Document, StorageError},
    EmbeddingError, EmbeddingProvider, InputKind, ModelInfo,
};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IndexerError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Default, Debug, serde::Serialize)]
pub struct Stats {
    pub queued: u64,
    pub indexed: u64,
    pub skipped: u64,
    pub retrying: u64,
    pub failed: u64,
    pub superseded: u64,
}

#[derive(sqlx::FromRow)]
struct Job {
    job_id: Uuid,
    claim_token: Uuid,
    attempts: i32,
    id: Uuid,
    artifact_id: Uuid,
    content: String,
    content_hash: String,
    has_embedding: bool,
}

pub struct Indexer<P> {
    provider: Arc<Mutex<P>>,
    model: ModelInfo,
    fingerprint: String,
}

impl<P: EmbeddingProvider + 'static> Indexer<P> {
    pub fn new(provider: P) -> Result<Self, StorageError> {
        let model = provider.model_info().clone();
        let fingerprint = storage::fingerprint(&model)?;
        Ok(Self {
            provider: Arc::new(Mutex::new(provider)),
            model,
            fingerprint,
        })
    }

    pub fn model_info(&self) -> &ModelInfo {
        &self.model
    }

    pub async fn poll_once(
        &mut self,
        pool: &PgPool,
        batch_size: i64,
    ) -> Result<Stats, IndexerError> {
        let mut stats = Stats::default();
        // Recover crashed workers, fencing off their old claim tokens.
        sqlx::query(
            r#"
            UPDATE search_embedding_jobs
            SET status=CASE WHEN attempts >= 3 THEN 'failed' ELSE 'queued' END,
                claimed_at=NULL, claim_token=NULL, available_at=now(), updated_at=now(),
                error_message='worker claim expired'
            WHERE model_fingerprint=$1 AND status='processing'
              AND claimed_at < now() - interval '5 minutes'
        "#,
        )
        .bind(&self.fingerprint)
        .execute(pool)
        .await?;
        sqlx::query(
            r#"
            DELETE FROM search_embedding_jobs WHERE id IN (
                SELECT id FROM search_embedding_jobs WHERE status='processed'
                AND updated_at < now() - interval '7 days'
                ORDER BY updated_at, id LIMIT 100 FOR UPDATE SKIP LOCKED
            )
        "#,
        )
        .execute(pool)
        .await?;

        // Failed jobs for unchanged text stay failed. Editing text resets them.
        // The selected model fingerprint makes model changes a fresh backfill.
        stats.queued = sqlx::query(r#"
            INSERT INTO search_embedding_jobs(document_id,model_fingerprint,source_content_hash)
            SELECT d.id,$1,d.content_hash FROM search_documents d
            WHERE NOT EXISTS (SELECT 1 FROM search_embeddings e
                WHERE e.document_id=d.id AND e.model_fingerprint=$1
                  AND e.source_content_hash=d.content_hash)
              AND NOT EXISTS (SELECT 1 FROM search_embedding_jobs j
                WHERE j.document_id=d.id AND j.model_fingerprint=$1
                  AND j.source_content_hash=d.content_hash AND j.status <> 'processed')
            ORDER BY d.id LIMIT 1000
            ON CONFLICT(document_id,model_fingerprint) DO UPDATE
            SET source_content_hash=EXCLUDED.source_content_hash, status='queued', attempts=0,
                available_at=now(), claimed_at=NULL, claim_token=NULL, error_message=NULL, updated_at=now()
            WHERE search_embedding_jobs.source_content_hash <> EXCLUDED.source_content_hash
               OR search_embedding_jobs.status='processed'
        "#).bind(&self.fingerprint).execute(pool).await?.rows_affected();

        // Claim each job immediately before computing it, not an entire batch
        // whose last claims could expire while waiting for CPU.
        for _ in 0..batch_size.clamp(1, 64) {
            let job = sqlx::query_as::<_,Job>(r#"
                WITH candidate AS (
                    SELECT j.id FROM search_embedding_jobs j
                    JOIN search_documents d ON d.id=j.document_id AND d.content_hash=j.source_content_hash
                    WHERE j.model_fingerprint=$1 AND j.status='queued' AND j.available_at<=now()
                    ORDER BY j.available_at,j.id LIMIT 1 FOR UPDATE OF j SKIP LOCKED
                ), claimed AS (
                    UPDATE search_embedding_jobs j SET status='processing',attempts=j.attempts+1,
                        claimed_at=now(),claim_token=gen_random_uuid(),updated_at=now()
                    FROM candidate c WHERE j.id=c.id
                    RETURNING j.id AS job_id,j.document_id,j.claim_token,j.attempts,j.source_content_hash
                ) SELECT c.job_id,c.claim_token,c.attempts,d.id,d.artifact_id,d.content,d.content_hash,
                    EXISTS(SELECT 1 FROM search_embeddings e WHERE e.document_id=d.id
                        AND e.model_fingerprint=$1 AND e.source_content_hash=d.content_hash) AS has_embedding
                  FROM claimed c JOIN search_documents d ON d.id=c.document_id
                    AND d.content_hash=c.source_content_hash
            "#).bind(&self.fingerprint).fetch_optional(pool).await?;
            let Some(job) = job else {
                break;
            };
            // A competing poll may have selected a missing vector just before
            // another worker finished it. Recheck after enqueueing, in this new
            // statement snapshot, instead of repeating expensive inference.
            if job.has_embedding {
                let completed=sqlx::query("UPDATE search_embedding_jobs SET status='processed',claimed_at=NULL,claim_token=NULL,error_message=NULL,updated_at=now() WHERE id=$1 AND claim_token=$2 AND status='processing'")
                    .bind(job.job_id).bind(job.claim_token).execute(pool).await?.rows_affected();
                if completed == 1 {
                    stats.skipped += 1;
                } else {
                    stats.superseded += 1;
                }
                continue;
            }
            let provider = Arc::clone(&self.provider);
            let text = job.content.clone();
            let result = tokio::task::spawn_blocking(move || {
                provider
                    .lock()
                    .map_err(|_| EmbeddingError::Inference("provider lock poisoned".into()))?
                    .embed(&[&text], InputKind::Document)
            })
            .await
            .unwrap_or_else(|e| {
                Err(EmbeddingError::Inference(format!(
                    "inference task failed: {e}"
                )))
            })
            .and_then(|vectors| {
                crate::validate_vectors(&vectors, 1, self.model.dimension)?;
                Ok(vectors.into_iter().next().expect("validated one vector"))
            });
            self.finish(pool, &job, result, &mut stats).await?;
        }
        Ok(stats)
    }

    async fn finish(
        &self,
        pool: &PgPool,
        job: &Job,
        result: Result<Vec<f32>, EmbeddingError>,
        stats: &mut Stats,
    ) -> Result<(), IndexerError> {
        let mut tx = pool.begin().await?;
        let owns: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM search_embedding_jobs WHERE id=$1 AND claim_token=$2 AND status='processing' FOR UPDATE)")
            .bind(job.job_id).bind(job.claim_token).fetch_one(&mut *tx).await?;
        if !owns {
            stats.superseded += 1;
            return Ok(());
        }
        let mut status = "processed";
        let mut error = None;
        if let Ok(vector) = &result {
            let document = Document {
                id: job.id,
                artifact_id: job.artifact_id,
                content: job.content.clone(),
                content_hash: job.content_hash.clone(),
            };
            let value = storage::vector_literal(vector, self.model.dimension)?;
            match storage::save_on(&mut tx, &document, &self.model, &self.fingerprint, &value).await
            {
                Ok(()) => stats.indexed += 1,
                Err(StorageError::StaleSource) => {
                    status = "queued";
                    stats.superseded += 1;
                }
                Err(e) => return Err(e.into()),
            }
        } else if let Err(e) = result {
            let permanent = matches!(
                e,
                EmbeddingError::EmptyInput { .. }
                    | EmbeddingError::InputTooLong { .. }
                    | EmbeddingError::InvalidOutput(_)
                    | EmbeddingError::Configuration(_)
            );
            status = if permanent || job.attempts >= 3 {
                stats.failed += 1;
                "failed"
            } else {
                stats.retrying += 1;
                "queued"
            };
            error = Some(e.to_string());
        }
        sqlx::query(r#"
            UPDATE search_embedding_jobs SET status=$2,error_message=$3,claimed_at=NULL,claim_token=NULL,
                available_at=now() + CASE WHEN $3::text IS NULL THEN interval '0 seconds'
                    ELSE interval '5 seconds' * power(2,LEAST(attempts-1,6)) END,updated_at=now()
            WHERE id=$1
        "#).bind(job.job_id).bind(status).bind(error).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
