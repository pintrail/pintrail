//! The Postgres-as-queue mechanism (docs/DESIGN.md §2.5).
//!
//! `attachments.status` is the queue column. `FOR UPDATE SKIP LOCKED` lets any
//! number of worker replicas poll the same table with zero coordination and no
//! double-processing — which is the whole reason this system needs no Redis.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "attachment_kind", rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
    Pdf,
    Text,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub kind: AttachmentKind,
    pub original_mime_type: String,
    pub original_storage_key: String,
    pub attempts: i32,
    pub created_at: DateTime<Utc>,
}

/// Claims up to `limit` queued attachments, marking them `processing`.
///
/// The UPDATE and the SELECT are one statement so a claim cannot be lost
/// between them. `SKIP LOCKED` means a replica steps over rows another replica
/// has locked rather than blocking on them.
pub async fn claim(db: &PgPool, limit: i64) -> anyhow::Result<Vec<Job>> {
    let jobs = sqlx::query_as::<_, Job>(
        r#"
        WITH claimed AS (
            SELECT id
            FROM attachments
            WHERE status = 'queued'
            ORDER BY created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE attachments a
        SET status = 'processing',
            claimed_at = now(),
            attempts = a.attempts + 1
        FROM claimed c
        WHERE a.id = c.id
        RETURNING a.id, a.artifact_id, a.kind, a.original_mime_type,
                  a.original_storage_key, a.attempts, a.created_at
        "#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    Ok(jobs)
}

pub struct Success {
    pub processed_storage_key: String,
    pub processed_mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

pub async fn mark_processed(db: &PgPool, id: Uuid, result: Success) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE attachments
        SET status = 'processed',
            processed_storage_key = $2,
            processed_mime_type = $3,
            width = $4,
            height = $5,
            error_message = NULL,
            claimed_at = NULL
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(result.processed_storage_key)
    .bind(result.processed_mime_type)
    .bind(result.width)
    .bind(result.height)
    .execute(db)
    .await?;

    Ok(())
}

/// Records a failure, retrying until `max_attempts` is spent.
///
/// A transient fault (storage blip) deserves another try; a corrupt file never
/// will succeed. Rather than trying to classify, this retries a bounded number
/// of times and then stops — an attachment that fails three times is a
/// content problem an author needs to see, not something to retry forever.
pub async fn mark_failed(
    db: &PgPool,
    id: Uuid,
    attempts: i32,
    max_attempts: i32,
    error: &str,
) -> anyhow::Result<bool> {
    let give_up = attempts >= max_attempts;

    // Truncated: an error chain from a subprocess can be long, and this is
    // shown to authors.
    let message: String = error.chars().take(1000).collect();

    if give_up {
        sqlx::query(
            "UPDATE attachments SET status = 'failed', error_message = $2, claimed_at = NULL \
             WHERE id = $1",
        )
        .bind(id)
        .bind(&message)
        .execute(db)
        .await?;
    } else {
        // Back to the queue. `attempts` is already incremented by the claim,
        // so this cannot loop forever.
        sqlx::query(
            "UPDATE attachments SET status = 'queued', error_message = $2, claimed_at = NULL \
             WHERE id = $1",
        )
        .bind(id)
        .bind(&message)
        .execute(db)
        .await?;
    }

    Ok(give_up)
}

/// Returns rows stranded in `processing` to the queue.
///
/// A worker killed mid-job leaves its claim behind, and nothing else would
/// ever pick it up — `SKIP LOCKED` only skips *locked* rows, and the lock died
/// with the connection. This is the crash-recovery half of the queue design.
pub async fn requeue_stuck(db: &PgPool, timeout_secs: i64, max_attempts: i32) -> anyhow::Result<u64> {
    let requeued = sqlx::query(
        r#"
        UPDATE attachments
        -- The casts are required: a bare CASE yields text, which Postgres
        -- will not assign to an enum column.
        SET status = CASE
                WHEN attempts >= $2 THEN 'failed'::processing_status
                ELSE 'queued'::processing_status
            END,
            claimed_at = NULL,
            error_message = CASE
                WHEN attempts >= $2
                THEN 'abandoned by a worker and out of retries'
                ELSE error_message
            END
        WHERE status = 'processing'
          AND claimed_at < now() - make_interval(secs => $1)
        "#,
    )
    .bind(timeout_secs as f64)
    .bind(max_attempts)
    .execute(db)
    .await?
    .rows_affected();

    Ok(requeued)
}
