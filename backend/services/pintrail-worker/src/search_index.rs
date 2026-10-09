//! Builds artifact text in the background. No AI service is called in this stage.
//! All document writes and job completion happen together in one transaction.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct Job {
    id: Uuid,
    artifact_id: Uuid,
    attempts: i32,
    claim_token: Uuid,
}

#[derive(sqlx::FromRow)]
struct Source {
    name: String,
    kind: String,
    description: String,
    ancestors: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Indexed,
    Unchanged,
    Removed,
    Superseded,
}

pub async fn poll_once(
    db: &PgPool,
    batch_size: i64,
    max_attempts: i32,
    stuck_timeout_secs: i64,
) -> anyhow::Result<()> {
    requeue_stuck(db, stuck_timeout_secs, max_attempts).await?;
    for job in claim(db, batch_size).await? {
        match process(db, &job).await {
            Ok(outcome) => {
                tracing::info!(job_id = %job.id, artifact_id = %job.artifact_id,
                    ?outcome, "artifact search indexing finished");
            }
            Err(err) => {
                let message = format!("{err:#}");
                mark_failed(db, &job, max_attempts, &message).await?;
                tracing::warn!(job_id = %job.id, artifact_id = %job.artifact_id,
                    attempts = job.attempts, error = %message, "artifact search indexing failed");
            }
        }
    }
    Ok(())
}

async fn claim(db: &PgPool, limit: i64) -> anyhow::Result<Vec<Job>> {
    Ok(sqlx::query_as::<_, Job>(
        r#"
        WITH next_jobs AS (
            SELECT id FROM search_index_jobs
            WHERE status = 'queued' AND available_at <= now()
            ORDER BY available_at, created_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE search_index_jobs j
        SET status = 'processing', attempts = j.attempts + 1,
            claimed_at = now(), claim_token = gen_random_uuid()
        FROM next_jobs n WHERE j.id = n.id
        RETURNING j.id, j.artifact_id, j.attempts, j.claim_token
        "#,
    )
    .bind(limit)
    .fetch_all(db)
    .await?)
}

async fn process(db: &PgPool, job: &Job) -> anyhow::Result<Outcome> {
    let mut tx = db.begin().await?;

    // A reclaimed job receives a new token. An old worker must not finish it.
    let owns_claim: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM search_index_jobs \
         WHERE id = $1 AND status = 'processing' AND claim_token = $2 FOR UPDATE",
    )
    .bind(job.id)
    .bind(job.claim_token)
    .fetch_optional(&mut *tx)
    .await?;
    if owns_claim.is_none() {
        tx.commit().await?;
        return Ok(Outcome::Superseded);
    }

    // Different jobs for the same artifact may be claimed by different replicas.
    // Serialize them before reading, so older text cannot overwrite newer text.
    // Hash collisions only serialize unrelated artifacts; they cannot mix data.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(job.artifact_id.to_string())
        .execute(&mut *tx)
        .await?;

    let source = sqlx::query_as::<_, Source>(
        r#"
        WITH RECURSIVE ancestors AS (
            SELECT p.name, p.parent_id, 1 AS depth
            FROM artifacts a JOIN artifacts p ON p.id = a.parent_id
            WHERE a.id = $1 AND p.deleted_at IS NULL
            UNION ALL
            SELECT p.name, p.parent_id, c.depth + 1
            FROM artifacts p JOIN ancestors c ON p.id = c.parent_id
            WHERE p.deleted_at IS NULL
        )
        SELECT a.name, a.kind::text AS kind, a.description,
            COALESCE((SELECT array_agg(name ORDER BY depth DESC) FROM ancestors),
                     ARRAY[]::text[]) AS ancestors
        FROM artifacts a WHERE a.id = $1 AND a.deleted_at IS NULL
        FOR SHARE OF a
        "#,
    )
    .bind(job.artifact_id)
    .fetch_optional(&mut *tx)
    .await?;

    let outcome = if let Some(source) = source {
        let content = canonical_text(&source);
        let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        let changed = sqlx::query(
            r#"
            INSERT INTO search_documents (artifact_id, content, content_hash)
            VALUES ($1, $2, $3)
            ON CONFLICT (artifact_id, attachment_id, source_kind, chunk_index)
            DO UPDATE SET content = EXCLUDED.content, content_hash = EXCLUDED.content_hash
            WHERE search_documents.content_hash IS DISTINCT FROM EXCLUDED.content_hash
            "#,
        )
        .bind(job.artifact_id)
        .bind(content)
        .bind(hash)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            Outcome::Unchanged
        } else {
            Outcome::Indexed
        }
    } else {
        sqlx::query("DELETE FROM search_documents WHERE artifact_id = $1")
            .bind(job.artifact_id)
            .execute(&mut *tx)
            .await?;
        Outcome::Removed
    };

    sqlx::query(
        "UPDATE search_index_jobs SET status = 'processed', \
         claimed_at = NULL, claim_token = NULL, error_message = NULL WHERE id = $1",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(outcome)
}

fn canonical_text(source: &Source) -> String {
    let mut lines = vec![
        format!("Name: {}", source.name.trim()),
        format!("Kind: {}", source.kind),
    ];
    let parents: Vec<&str> = source
        .ancestors
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if !parents.is_empty() {
        lines.push(format!("Parents: {}", parents.join(" > ")));
    }
    if !source.description.trim().is_empty() {
        lines.push(format!("Description: {}", source.description.trim()));
    }
    lines.join("\n")
}

async fn mark_failed(db: &PgPool, job: &Job, max_attempts: i32, error: &str) -> anyhow::Result<()> {
    let message: String = error.chars().take(1000).collect();
    sqlx::query(
        r#"
        UPDATE search_index_jobs
        SET status = CASE WHEN attempts >= $3 THEN 'failed' ELSE 'queued' END,
            available_at = now() + make_interval(secs => LEAST(300, power(2, attempts))::double precision),
            claimed_at = NULL, claim_token = NULL, error_message = $4
        WHERE id = $1 AND status = 'processing' AND claim_token = $2
        "#,
    )
    .bind(job.id)
    .bind(job.claim_token)
    .bind(max_attempts)
    .bind(message)
    .execute(db)
    .await?;
    Ok(())
}

async fn requeue_stuck(db: &PgPool, timeout_secs: i64, max_attempts: i32) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE search_index_jobs
        SET status = CASE WHEN attempts >= $2 THEN 'failed' ELSE 'queued' END,
            claimed_at = NULL, claim_token = NULL, available_at = now(),
            error_message = 'worker claim expired before completion'
        WHERE status = 'processing'
            AND claimed_at < now() - make_interval(secs => $1)
        "#,
    )
    .bind(timeout_secs as f64)
    .bind(max_attempts)
    .execute(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
