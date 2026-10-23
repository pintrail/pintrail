//! Optional PostgreSQL storage. Inference must finish before calling `save`.
use crate::{validate_vectors, EmbeddingError, ModelInfo};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Embedding(#[from] EmbeddingError),
    #[error("model metadata must include the loaded revision, repository and pipeline")]
    MissingIdentity,
    #[error("source document changed or disappeared; regenerate its embedding")]
    StaleSource,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub artifact_id: Uuid,
    pub content: String,
    pub content_hash: String,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct Hit {
    pub artifact_id: Uuid,
    pub document_id: Uuid,
    pub name: String,
    pub is_topic: bool,
    pub similarity: f64,
}

pub async fn migrate(pool: &PgPool) -> Result<(), StorageError> {
    // This database already contains the normal API migrations.
    sqlx::migrate!("./migrations")
        .set_ignore_missing(true)
        .run(pool)
        .await?;
    Ok(())
}

pub fn fingerprint(model: &ModelInfo) -> Result<String, StorageError> {
    if model.name.is_empty()
        || model.repository.is_empty()
        || model.pipeline_version.is_empty()
        || model.revision.as_deref().is_none_or(str::is_empty)
        || !(1..=16000).contains(&model.dimension)
    {
        return Err(StorageError::MissingIdentity);
    }
    // Include every inference setting recorded in ModelInfo, including token limit.
    let identity = serde_json::to_vec(model).expect("model metadata is serializable");
    Ok(format!("{:x}", Sha256::digest(identity)))
}

pub(crate) fn vector_literal(vector: &[f32], dimension: usize) -> Result<String, StorageError> {
    validate_vectors(&[vector.to_vec()], 1, dimension)?;
    // Bind as data, then cast in PostgreSQL. Never interpolate it into SQL.
    Ok(format!(
        "[{}]",
        vector
            .iter()
            .map(f32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub async fn documents(pool: &PgPool) -> Result<Vec<Document>, StorageError> {
    Ok(sqlx::query_as(
        "SELECT id, artifact_id, content, content_hash FROM search_documents ORDER BY id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn save(
    pool: &PgPool,
    document: &Document,
    model: &ModelInfo,
    vector: &[f32],
) -> Result<(), StorageError> {
    let fingerprint = fingerprint(model)?;
    let value = vector_literal(vector, model.dimension)?;
    let mut tx = pool.begin().await?;
    save_on(&mut tx, document, model, &fingerprint, &value).await?;
    tx.commit().await?;
    Ok(())
}

// Saving a vector and completing its indexing job can share one transaction.
pub(crate) async fn save_on(
    connection: &mut sqlx::PgConnection,
    document: &Document,
    model: &ModelInfo,
    fingerprint: &str,
    value: &str,
) -> Result<(), StorageError> {
    // Locks only the short write, not the expensive model computation.
    let current: Option<String> =
        sqlx::query_scalar("SELECT content_hash FROM search_documents WHERE id=$1 FOR SHARE")
            .bind(document.id)
            .fetch_optional(&mut *connection)
            .await?;
    if current.as_deref() != Some(document.content_hash.as_str()) {
        return Err(StorageError::StaleSource);
    }
    sqlx::query(
        r#"
        INSERT INTO search_embeddings
            (document_id, model_fingerprint, model_name, model_repository, model_revision,
             pipeline_version, dimension, source_content_hash, embedding)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::text::vector)
        ON CONFLICT (document_id, model_fingerprint) DO UPDATE
        SET source_content_hash=EXCLUDED.source_content_hash, embedding=EXCLUDED.embedding
        WHERE search_embeddings.source_content_hash IS DISTINCT FROM EXCLUDED.source_content_hash
           OR search_embeddings.embedding <> EXCLUDED.embedding
    "#,
    )
    .bind(document.id)
    .bind(fingerprint)
    .bind(&model.name)
    .bind(&model.repository)
    .bind(&model.revision)
    .bind(&model.pipeline_version)
    .bind(model.dimension as i32)
    .bind(&document.content_hash)
    .bind(value)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Public retrieval: permission filtering happens before LIMIT. Exact cosine
/// scanning is intentional for the small initial dataset; no ANN index yet.
pub async fn search(
    pool: &PgPool,
    model: &ModelInfo,
    query: &[f32],
    limit: i64,
) -> Result<Vec<Hit>, StorageError> {
    let fingerprint = fingerprint(model)?;
    let value = vector_literal(query, model.dimension)?;
    Ok(sqlx::query_as(
        r#"
        SELECT d.artifact_id, d.id AS document_id, a.name, a.is_topic,
               1 - (e.embedding <=> $1::text::vector) AS similarity
        FROM search_embeddings e JOIN search_documents d ON d.id=e.document_id
        JOIN artifacts a ON a.id=d.artifact_id
        WHERE e.model_fingerprint=$2 AND e.dimension=$3
          AND e.source_content_hash=d.content_hash
          AND a.deleted_at IS NULL AND artifact_is_published(a.id)
        ORDER BY e.embedding <=> $1::text::vector, d.id
        LIMIT $4
    "#,
    )
    .bind(value)
    .bind(fingerprint)
    .bind(model.dimension as i32)
    .bind(limit.clamp(1, 100))
    .fetch_all(pool)
    .await?)
}
