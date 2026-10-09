use super::*;
use chrono::{DateTime, Utc};

async fn artifact(db: &PgPool, name: &str, parent: Option<Uuid>) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO artifacts (name, kind, description, parent_id) \
         VALUES ($1, 'installation', 'Collects rainwater runoff.', $2) RETURNING id",
    )
    .bind(name)
    .bind(parent)
    .fetch_one(db)
    .await?)
}

async fn document(
    db: &PgPool,
    artifact_id: Uuid,
) -> anyhow::Result<(Uuid, String, String, DateTime<Utc>)> {
    Ok(sqlx::query_as(
        "SELECT id, content, content_hash, updated_at FROM search_documents WHERE artifact_id = $1",
    )
    .bind(artifact_id)
    .fetch_one(db)
    .await?)
}

#[sqlx::test(migrations = "../../migrations")]
async fn creation_and_updates_are_indexed_without_duplicate_documents(
    db: PgPool,
) -> anyhow::Result<()> {
    let id = artifact(&db, "Rain Garden", None).await?;
    poll_once(&db, 100, 3, 300).await?;
    let first = document(&db, id).await?;
    assert!(first.1.contains("Name: Rain Garden"));
    assert!(first.1.contains("Collects rainwater runoff."));
    assert_eq!(first.2.len(), 64);

    sqlx::query("UPDATE artifacts SET description = 'Reduces flooding.' WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?;
    poll_once(&db, 100, 3, 300).await?;
    let updated = document(&db, id).await?;
    assert_eq!(first.0, updated.0, "citations retain a stable document ID");
    assert_ne!(first.2, updated.2);
    assert!(updated.1.contains("Reduces flooding."));
    assert!(!updated.1.contains("Collects rainwater runoff."));

    // Raw input changed, but canonical content did not. Do not rewrite the document.
    sqlx::query("UPDATE artifacts SET description = '  Reduces flooding.  ' WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?;
    let jobs = claim(&db, 100).await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(process(&db, &jobs[0]).await?, Outcome::Unchanged);
    assert_eq!(updated, document(&db, id).await?);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn rolled_back_artifacts_do_not_leave_jobs(db: PgPool) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO artifacts (name) VALUES ('Rolled back') RETURNING id")
            .fetch_one(&mut *tx)
            .await?;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM search_index_jobs WHERE artifact_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(queued, 1, "trigger runs within the artifact transaction");
    tx.rollback().await?;
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM search_index_jobs")
        .fetch_one(&db)
        .await?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn ancestor_renames_and_reparenting_refresh_descendants(db: PgPool) -> anyhow::Result<()> {
    let root = artifact(&db, "Campus", None).await?;
    let building = artifact(&db, "North Building", Some(root)).await?;
    let child = artifact(&db, "Garden", Some(building)).await?;
    let other = artifact(&db, "South Building", None).await?;
    poll_once(&db, 100, 3, 300).await?;
    assert!(document(&db, child)
        .await?
        .1
        .contains("Parents: Campus > North Building"));

    sqlx::query("UPDATE artifacts SET name = 'Renamed Campus' WHERE id = $1")
        .bind(root)
        .execute(&db)
        .await?;
    poll_once(&db, 100, 3, 300).await?;
    assert!(document(&db, child)
        .await?
        .1
        .contains("Parents: Renamed Campus > North Building"));

    sqlx::query("UPDATE artifacts SET parent_id = $2 WHERE id = $1")
        .bind(building)
        .bind(other)
        .execute(&db)
        .await?;
    poll_once(&db, 100, 3, 300).await?;
    let text = document(&db, child).await?.1;
    assert!(text.contains("Parents: South Building > North Building"));
    assert!(!text.contains("Renamed Campus"));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn soft_deletion_removes_text_and_pending_jobs_cannot_restore_it(
    db: PgPool,
) -> anyhow::Result<()> {
    let id = artifact(&db, "Deleted Garden", None).await?;
    poll_once(&db, 100, 3, 300).await?;
    sqlx::query("UPDATE artifacts SET description = 'Waiting to index' WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?;
    sqlx::query("UPDATE artifacts SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&db)
        .await?;
    let immediate: i64 =
        sqlx::query_scalar("SELECT count(*) FROM search_documents WHERE artifact_id = $1")
            .bind(id)
            .fetch_one(&db)
            .await?;
    assert_eq!(immediate, 0, "deletion does not wait for the worker");
    poll_once(&db, 100, 3, 300).await?;
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM search_documents")
        .fetch_one(&db)
        .await?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn hard_deletion_cascades_documents_and_jobs(db: PgPool) -> anyhow::Result<()> {
    let root = artifact(&db, "Building", None).await?;
    artifact(&db, "Garden", Some(root)).await?;
    poll_once(&db, 100, 3, 300).await?;
    sqlx::query("DELETE FROM artifacts WHERE id = $1")
        .bind(root)
        .execute(&db)
        .await?;
    let documents: i64 = sqlx::query_scalar("SELECT count(*) FROM search_documents")
        .fetch_one(&db)
        .await?;
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM search_index_jobs")
        .fetch_one(&db)
        .await?;
    assert_eq!((documents, jobs), (0, 0));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_claims_do_not_take_the_same_job(db: PgPool) -> anyhow::Result<()> {
    artifact(&db, "First Garden", None).await?;
    artifact(&db, "Second Garden", None).await?;
    let (first, second) = tokio::try_join!(claim(&db, 1), claim(&db, 1))?;
    assert_eq!((first.len(), second.len()), (1, 1));
    assert_ne!(first[0].id, second[0].id);
    tokio::try_join!(process(&db, &first[0]), process(&db, &second[0]))?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_claims_cannot_complete_or_fail_a_reclaimed_job(db: PgPool) -> anyhow::Result<()> {
    let id = artifact(&db, "Garden", None).await?;
    let old = claim(&db, 1).await?.remove(0);
    sqlx::query(
        "UPDATE search_index_jobs SET claimed_at = now() - interval '10 minutes' WHERE id = $1",
    )
    .bind(old.id)
    .execute(&db)
    .await?;
    requeue_stuck(&db, 300, 3).await?;
    let current = claim(&db, 1).await?.remove(0);
    assert_eq!(old.id, current.id);
    assert_ne!(old.claim_token, current.claim_token);
    assert_eq!(process(&db, &old).await?, Outcome::Superseded);
    mark_failed(&db, &old, 3, "late failure from old worker").await?;
    assert_eq!(process(&db, &current).await?, Outcome::Indexed);
    assert!(document(&db, id).await?.1.contains("Name: Garden"));
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_jobs_retry_with_backoff_and_stop_at_the_attempt_limit(
    db: PgPool,
) -> anyhow::Result<()> {
    artifact(&db, "Garden", None).await?;
    for attempt in 1..=3 {
        let job = claim(&db, 1).await?.remove(0);
        assert_eq!(job.attempts, attempt);
        mark_failed(&db, &job, 3, "simulated temporary error").await?;
        let (status, reason, delayed): (String, String, bool) = sqlx::query_as(
            "SELECT status, error_message, available_at > now() FROM search_index_jobs WHERE id = $1",
        ).bind(job.id).fetch_one(&db).await?;
        assert_eq!(reason, "simulated temporary error");
        assert!(delayed);
        if attempt < 3 {
            assert_eq!(status, "queued");
            assert!(claim(&db, 1).await?.is_empty());
            sqlx::query("UPDATE search_index_jobs SET available_at = now() WHERE id = $1")
                .bind(job.id)
                .execute(&db)
                .await?;
        } else {
            assert_eq!(status, "failed");
        }
    }
    assert!(claim(&db, 1).await?.is_empty());
    Ok(())
}
