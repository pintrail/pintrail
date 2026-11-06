use super::*;

#[sqlx::test(migrations = "../../migrations")]
async fn tag_moves_and_hard_topic_deletion_refresh_without_stale_terms(db: PgPool) {
    let a = place(&db, "First place").await;
    let b = place(&db, "Second place").await;
    let t = topic(&db, "Shared topic", "approved").await;
    sqlx::query("INSERT INTO artifact_tags(artifact_id,tag) VALUES ($1,'oldtag')")
        .bind(a)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO artifact_topics(artifact_id,topic_id) VALUES ($1,$2),($3,$2)")
        .bind(a)
        .bind(t)
        .bind(b)
        .execute(&db)
        .await
        .unwrap();
    text(&db, a).await;
    text(&db, b).await;
    sqlx::query("UPDATE artifact_tags SET artifact_id=$2,tag='newtag' WHERE artifact_id=$1")
        .bind(a)
        .bind(b)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, a).await;
    absent(&db, b).await;
    assert!(!text(&db, a).await.contains("oldtag"));
    assert!(text(&db, b).await.contains("newtag"));
    sqlx::query("DELETE FROM artifacts WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, a).await;
    absent(&db, b).await;
    assert!(!text(&db, a).await.contains("Shared topic"));
    assert!(!text(&db, b).await.contains("Shared topic"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn metadata_committing_while_worker_waits_is_read_from_fresh_snapshot(db: PgPool) {
    let p = place(&db, "Race example").await;
    let mut tx = db.begin().await.unwrap();
    sqlx::query("INSERT INTO artifact_tags(artifact_id,tag) VALUES ($1,'newly committed tag')")
        .bind(p)
        .execute(&mut *tx)
        .await
        .unwrap();
    let pool = db.clone();
    let running = tokio::spawn(async move { poll_once(&pool, 1000, 3, 300).await });
    let mut waiting = false;
    for _ in 0..100 {
        waiting=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND pid<>pg_backend_pid())")
            .fetch_one(&db).await.unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(waiting, "worker must wait for the metadata transaction");
    tx.commit().await.unwrap();
    running.await.unwrap().unwrap();
    let content: String =
        sqlx::query_scalar("SELECT content FROM search_documents WHERE artifact_id=$1")
            .bind(p)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(content.contains("newly committed tag"));
}

async fn place(db: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO artifacts(name,status) VALUES ($1,'approved') RETURNING id")
        .bind(name)
        .fetch_one(db)
        .await
        .unwrap()
}
async fn topic(db: &PgPool, name: &str, status: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO artifacts(name,is_topic,status) VALUES ($1,true,$2::artifact_status) RETURNING id")
        .bind(name).bind(status).fetch_one(db).await.unwrap()
}
async fn text(db: &PgPool, id: Uuid) -> String {
    poll_once(db, 1000, 3, 300).await.unwrap();
    sqlx::query_scalar("SELECT content FROM search_documents WHERE artifact_id=$1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap()
}
async fn absent(db: &PgPool, id: Uuid) {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM search_documents WHERE artifact_id=$1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(
        n, 0,
        "source withdrawal invalidates old text before a worker runs"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn tags_topics_notes_are_sorted_and_noop_refresh_keeps_hash(db: PgPool) {
    let p = place(&db, "Example").await;
    let t = topic(&db, "LEED certification", "approved").await;
    sqlx::query("INSERT INTO artifact_tags(artifact_id,tag) VALUES ($1,' Water '),($1,'solar'),($1,'water')").bind(p).execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO artifact_topics(artifact_id,topic_id,note) VALUES ($1,$2,'Gold, 2019')",
    )
    .bind(p)
    .bind(t)
    .execute(&db)
    .await
    .unwrap();
    let original = text(&db, p).await;
    assert!(original.contains("Tags: solar, water"));
    assert!(original.contains("Topics: LEED certification (Gold, 2019)"));
    let original_hash: String =
        sqlx::query_scalar("SELECT content_hash FROM search_documents WHERE artifact_id=$1")
            .bind(p)
            .fetch_one(&db)
            .await
            .unwrap();
    sqlx::query("INSERT INTO search_index_jobs(artifact_id) VALUES ($1)")
        .bind(p)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(text(&db, p).await, original);
    let hash: String =
        sqlx::query_scalar("SELECT content_hash FROM search_documents WHERE artifact_id=$1")
            .bind(p)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(hash, original_hash);
    sqlx::query("DELETE FROM artifact_tags WHERE artifact_id=$1 AND tag='solar'")
        .bind(p)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, p).await;
    assert!(!text(&db, p).await.contains("solar"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn moved_links_notes_and_topic_rename_refresh_all_affected_artifacts(db: PgPool) {
    let a = place(&db, "A").await;
    let b = place(&db, "B").await;
    let t = topic(&db, "Old topic", "approved").await;
    sqlx::query("INSERT INTO artifact_topics(artifact_id,topic_id,note) VALUES ($1,$2,'Gold')")
        .bind(a)
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    assert!(text(&db, a).await.contains("Old topic (Gold)"));
    text(&db, b).await;
    sqlx::query("UPDATE artifact_topics SET artifact_id=$2,note='Silver' WHERE artifact_id=$1")
        .bind(a)
        .bind(b)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, a).await;
    absent(&db, b).await;
    assert!(!text(&db, a).await.contains("Old topic"));
    assert!(text(&db, b).await.contains("Old topic (Silver)"));
    sqlx::query("UPDATE artifacts SET name='New topic' WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, b).await;
    assert!(text(&db, b).await.contains("New topic (Silver)"));
    sqlx::query("DELETE FROM artifact_topics WHERE artifact_id=$1")
        .bind(b)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, b).await;
    assert!(!text(&db, b).await.contains("New topic"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unpublished_or_deleted_topics_never_enter_public_text(db: PgPool) {
    let p = place(&db, "Published place").await;
    let t = topic(&db, "Private topic name", "draft").await;
    sqlx::query(
        "INSERT INTO artifact_topics(artifact_id,topic_id,note) VALUES ($1,$2,'Private note')",
    )
    .bind(p)
    .bind(t)
    .execute(&db)
    .await
    .unwrap();
    assert!(!text(&db, p).await.contains("Private"));
    sqlx::query("UPDATE artifacts SET status='approved' WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    assert!(text(&db, p)
        .await
        .contains("Private topic name (Private note)"));
    sqlx::query("UPDATE artifacts SET status='draft' WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, p).await;
    assert!(!text(&db, p).await.contains("Private"));
    sqlx::query("UPDATE artifacts SET status='approved' WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    text(&db, p).await;
    sqlx::query("UPDATE artifacts SET deleted_at=now() WHERE id=$1")
        .bind(t)
        .execute(&db)
        .await
        .unwrap();
    absent(&db, p).await;
    assert!(!text(&db, p).await.contains("Private"));
}
