#![cfg(feature = "postgres")]
use pintrail_embeddings::{storage, ModelInfo};
use sqlx::PgPool;
use uuid::Uuid;

fn model(revision: &str) -> ModelInfo {
    ModelInfo {
        name: "test/model".into(),
        repository: "test/export".into(),
        revision: Some(revision.into()),
        dimension: 3,
        max_tokens: 32,
        pipeline_version: "test-v1".into(),
    }
}

async fn document(pool: &PgPool, name: &str, status: &str) -> storage::Document {
    let artifact: Uuid = sqlx::query_scalar(
        "INSERT INTO artifacts(kind,name,status) VALUES ('other',$1,$2::artifact_status) RETURNING id"
    ).bind(name).bind(status).fetch_one(pool).await.unwrap();
    sqlx::query_as("INSERT INTO search_documents(artifact_id,content,content_hash) VALUES ($1,$2,$3) RETURNING id,artifact_id,content,content_hash")
        .bind(artifact).bind(name).bind("a".repeat(64)).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn cosine_ranking_filters_drafts_before_limit_and_isolates_models(pool: PgPool) {
    storage::migrate(&pool).await.unwrap();
    let draft = document(&pool, "hidden", "draft").await;
    let first = document(&pool, "first", "approved").await;
    let second = document(&pool, "second", "approved").await;
    let m = model("revision-a");
    for (d, v) in [
        (&draft, [1.0, 0.0, 0.0]),
        (&first, [0.8, 0.6, 0.0]),
        (&second, [0.0, 1.0, 0.0]),
    ] {
        storage::save(&pool, d, &m, &v).await.unwrap();
    }
    let hits = storage::search(&pool, &m, &[1.0, 0.0, 0.0], 1)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].artifact_id, first.artifact_id);
    assert!((hits[0].similarity - 0.8).abs() < 0.00001);
    assert!(
        storage::search(&pool, &model("revision-b"), &[1.0, 0.0, 0.0], 3)
            .await
            .unwrap()
            .is_empty()
    );
    // An approved child below a draft building is also private.
    sqlx::query("UPDATE artifacts SET parent_id=$1 WHERE id=$2")
        .bind(draft.artifact_id)
        .bind(first.artifact_id)
        .execute(&pool)
        .await
        .unwrap();
    let hits = storage::search(&pool, &m, &[1.0, 0.0, 0.0], 1)
        .await
        .unwrap();
    assert_eq!(hits[0].artifact_id, second.artifact_id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_documents_invalidate_vectors_and_reject_stale_writes(pool: PgPool) {
    storage::migrate(&pool).await.unwrap();
    let d = document(&pool, "rain garden", "approved").await;
    let m = model("a");
    storage::save(&pool, &d, &m, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    storage::save(&pool, &d, &m, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "saving twice must not duplicate a vector");
    sqlx::query("UPDATE search_documents SET content='changed', content_hash=$1 WHERE id=$2")
        .bind("b".repeat(64))
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(storage::search(&pool, &m, &[1.0, 0.0, 0.0], 3)
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        storage::save(&pool, &d, &m, &[1.0, 0.0, 0.0]).await,
        Err(storage::StorageError::StaleSource)
    ));
    let updated = storage::documents(&pool).await.unwrap().remove(0);
    storage::save(&pool, &updated, &m, &[1.0, 0.0, 0.0])
        .await
        .unwrap();
    sqlx::query("DELETE FROM search_documents WHERE id=$1")
        .bind(d.id)
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "deleting a document removes its vectors");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_vectors_and_unidentified_models_never_reach_storage(pool: PgPool) {
    storage::migrate(&pool).await.unwrap();
    let d = document(&pool, "test", "approved").await;
    let mut m = model("a");
    for v in [vec![1.0], vec![0.0, 0.0, 0.0], vec![f32::NAN, 1.0, 0.0]] {
        assert!(storage::save(&pool, &d, &m, &v).await.is_err());
    }
    m.revision = None;
    assert!(matches!(
        storage::save(&pool, &d, &m, &[1.0, 0.0, 0.0]).await,
        Err(storage::StorageError::MissingIdentity)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
