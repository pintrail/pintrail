#![cfg(feature = "postgres")]
use pintrail_embeddings::{
    indexer::Indexer, storage, EmbeddingError, EmbeddingProvider, InputKind, ModelInfo,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Fake {
    info: ModelInfo,
    calls: Arc<AtomicUsize>,
    failures: usize,
    permanent: bool,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    release: Option<std::sync::mpsc::Receiver<()>>,
}
impl Fake {
    fn new(calls: Arc<AtomicUsize>, revision: &str) -> Self {
        Self {
            info: ModelInfo {
                name: "test/indexer".into(),
                repository: "test/export".into(),
                revision: Some(revision.into()),
                dimension: 3,
                max_tokens: 32,
                pipeline_version: "test-v1".into(),
            },
            calls,
            failures: 0,
            permanent: false,
            entered: None,
            release: None,
        }
    }
}
impl EmbeddingProvider for Fake {
    fn model_info(&self) -> &ModelInfo {
        &self.info
    }
    fn embed(&mut self, texts: &[&str], _: InputKind) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release
                .take()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        }
        if self.permanent {
            return Err(EmbeddingError::InputTooLong {
                index: 0,
                tokens: 100,
                limit: 32,
            });
        }
        if self.failures > 0 {
            self.failures -= 1;
            return Err(EmbeddingError::Inference("temporary failure".into()));
        }
        Ok(texts
            .iter()
            .map(|t| vec![1.0, t.len() as f32, t.split_whitespace().count() as f32])
            .collect())
    }
}
async fn fixture(pool: &PgPool) -> storage::Document {
    storage::migrate(pool).await.unwrap();
    let artifact: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO artifacts(kind,name,status) VALUES ('other','test','approved') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query_as("INSERT INTO search_documents(artifact_id,content,content_hash) VALUES ($1,'old',$2) RETURNING id,artifact_id,content,content_hash")
        .bind(artifact).bind(format!("{:x}",Sha256::digest(b"old"))).fetch_one(pool).await.unwrap()
}
async fn edit(pool: &PgPool, d: &storage::Document) {
    sqlx::query("UPDATE search_documents SET content='new text',content_hash=$2 WHERE id=$1")
        .bind(d.id)
        .bind(format!("{:x}", Sha256::digest(b"new text")))
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn initial_backfill_skips_unchanged_text_refreshes_edits_and_handles_model_changes(
    pool: PgPool,
) {
    let d = fixture(&pool).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut indexer = Indexer::new(Fake::new(calls.clone(), "one")).unwrap();
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().indexed, 1);
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().indexed, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    sqlx::query("DELETE FROM search_embeddings WHERE document_id=$1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        indexer.poll_once(&pool, 10).await.unwrap().indexed,
        1,
        "a backfill repairs a missing vector even if its old job completed"
    );
    edit(&pool, &d).await;
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().indexed, 1);
    let mut other_model = Indexer::new(Fake::new(calls.clone(), "two")).unwrap();
    assert_eq!(other_model.poll_once(&pool, 10).await.unwrap().indexed, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    sqlx::query("DELETE FROM search_documents WHERE id=$1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embedding_jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_workers_do_not_compute_the_same_job(pool: PgPool) {
    fixture(&pool).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut a = Indexer::new(Fake::new(calls.clone(), "one")).unwrap();
    let mut b = Indexer::new(Fake::new(calls.clone(), "one")).unwrap();
    let (a, b) = tokio::join!(a.poll_once(&pool, 10), b.poll_once(&pool, 10));
    assert_eq!(a.unwrap().indexed + b.unwrap().indexed, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn queued_job_with_an_already_completed_vector_skips_inference(pool: PgPool) {
    fixture(&pool).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut indexer = Indexer::new(Fake::new(calls.clone(), "one")).unwrap();
    assert_eq!(indexer.poll_once(&pool, 1).await.unwrap().indexed, 1);
    // An enqueuer's snapshot can predate a competing worker's commit while
    // its ON CONFLICT update sees the newly completed job.
    sqlx::query("UPDATE search_embedding_jobs SET status='queued'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(indexer.poll_once(&pool, 1).await.unwrap().skipped, 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "do not compute a valid vector twice"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn transient_errors_retry_but_oversized_inputs_remain_inspectable(pool: PgPool) {
    let d = fixture(&pool).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut provider = Fake::new(calls.clone(), "one");
    provider.failures = 1;
    let mut indexer = Indexer::new(provider).unwrap();
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().retrying, 1);
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().indexed, 0);
    sqlx::query("UPDATE search_embedding_jobs SET available_at=now()")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(indexer.poll_once(&pool, 10).await.unwrap().indexed, 1);
    let mut provider = Fake::new(calls.clone(), "oversized");
    provider.permanent = true;
    let mut oversized = Indexer::new(provider).unwrap();
    assert_eq!(oversized.poll_once(&pool, 10).await.unwrap().failed, 1);
    let before = calls.load(Ordering::SeqCst);
    assert_eq!(oversized.poll_once(&pool, 10).await.unwrap().failed, 0);
    assert_eq!(calls.load(Ordering::SeqCst), before);
    let error: String =
        sqlx::query_scalar("SELECT error_message FROM search_embedding_jobs WHERE status='failed'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(error.contains("split it into passages"));
    edit(&pool, &d).await;
    assert_eq!(
        oversized.poll_once(&pool, 10).await.unwrap().queued,
        1,
        "an edit resets a failed source"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn text_changed_during_inference_is_not_saved(pool: PgPool) {
    let d = fixture(&pool).await;
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let mut provider = Fake::new(Arc::new(AtomicUsize::new(0)), "one");
    provider.entered = Some(entered);
    provider.release = Some(wait);
    let mut indexer = Indexer::new(provider).unwrap();
    let db = pool.clone();
    let running = tokio::spawn(async move {
        let stats = indexer.poll_once(&db, 1).await.unwrap();
        (indexer, stats)
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), ready)
        .await
        .unwrap()
        .unwrap();
    edit(&pool, &d).await;
    release.send(()).unwrap();
    let (mut indexer, stats) = running.await.unwrap();
    assert_eq!(stats.superseded, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(indexer.poll_once(&pool, 1).await.unwrap().indexed, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_claim_is_recovered_and_old_worker_cannot_overwrite_it(pool: PgPool) {
    fixture(&pool).await;
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut provider = Fake::new(calls.clone(), "one");
    provider.entered = Some(entered);
    provider.release = Some(wait);
    let mut first = Indexer::new(provider).unwrap();
    let db = pool.clone();
    let running = tokio::spawn(async move { first.poll_once(&db, 1).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(10), ready)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE search_embedding_jobs SET claimed_at=now()-interval '10 minutes'")
        .execute(&pool)
        .await
        .unwrap();
    let mut second = Indexer::new(Fake::new(calls, "one")).unwrap();
    assert_eq!(second.poll_once(&pool, 1).await.unwrap().indexed, 1);
    release.send(()).unwrap();
    assert_eq!(running.await.unwrap().superseded, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
