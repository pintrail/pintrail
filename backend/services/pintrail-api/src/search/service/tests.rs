use super::*;
use pintrail_embeddings::test_provider::DeterministicProvider;

struct BlockingProvider {
    inner: DeterministicProvider,
    started: Option<tokio::sync::oneshot::Sender<()>>,
    release: Option<std::sync::mpsc::Receiver<()>>,
}
impl EmbeddingProvider for BlockingProvider {
    fn model_info(&self) -> &ModelInfo {
        self.inner.model_info()
    }
    fn embed(&mut self, texts: &[&str], kind: InputKind) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if let Some(release) = self.release.take() {
            let _ = self.started.take().unwrap().send(());
            release
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| EmbeddingError::Inference("test release timeout".into()))?;
        }
        self.inner.embed(texts, kind)
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn timeout_keeps_capacity_reserved_until_cpu_work_really_finishes(db: PgPool) {
    storage::migrate(&db).await.unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut service = SearchService::from_provider(BlockingProvider {
        inner: DeterministicProvider::default(),
        started: Some(started_tx),
        release: Some(release_rx),
    })
    .unwrap();
    service.deadline = Duration::from_millis(100);
    let service_clone = service.clone();
    let pool = db.clone();
    let first = tokio::spawn(async move {
        service_clone
            .query(&pool, "solar".into(), 3, SearchMode::Semantic)
            .await
    });
    started_rx.await.unwrap();
    assert!(matches!(
        service
            .query(&db, "solar".into(), 3, SearchMode::Semantic)
            .await,
        Err(AppError::ServiceUnavailable(
            "search is busy; try again shortly"
        ))
    ));
    assert!(matches!(
        first.await.unwrap(),
        Err(AppError::ServiceUnavailable(
            "search timed out; try again shortly"
        ))
    ));
    assert_eq!(
        service.slots.available_permits(),
        0,
        "timeout cannot permit overlapping CPU jobs"
    );
    release_tx.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(2),
        service.slots.clone().acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    service.deadline = Duration::from_secs(2);
    assert_eq!(
        service
            .query(&db, "solar".into(), 3, SearchMode::Semantic)
            .await
            .unwrap()
            .count,
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelling_request_does_not_release_a_still_running_cpu_job(db: PgPool) {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let service = SearchService::from_provider(BlockingProvider {
        inner: DeterministicProvider::default(),
        started: Some(started_tx),
        release: Some(release_rx),
    })
    .unwrap();
    let service_clone = service.clone();
    let pool = db.clone();
    let request = tokio::spawn(async move {
        service_clone
            .query(&pool, "solar".into(), 3, SearchMode::Hybrid)
            .await
    });
    started_rx.await.unwrap();
    request.abort();
    let _ = request.await;
    assert_eq!(service.slots.available_permits(), 0);
    release_tx.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(2),
        service.slots.clone().acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
}
