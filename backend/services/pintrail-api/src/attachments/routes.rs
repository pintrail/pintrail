use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use uuid::Uuid;

use super::mime::{classify, sanitize_filename};
use super::model::{
    Attachment, AttachmentView, ProcessingStatus, UpdateAttachmentRequest, UploadIntentRequest,
};
use crate::authors::extractors::RequireEditor;
use crate::error::{AppError, AppResult};
use crate::identity::Identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/artifacts/{id}/attachments",
            get(list_for_artifact_route),
        )
        .route(
            "/artifacts/{id}/attachments/upload-intent",
            post(upload_intent),
        )
        .route("/attachments/{id}/complete", post(complete_upload))
        .route(
            "/attachments/{id}",
            get(detail).patch(update).delete(remove),
        )
}

const SELECT_COLUMNS: &str = r#"
    id, artifact_id, kind, position, caption, original_filename,
    original_mime_type, original_storage_key, status, processed_storage_key,
    processed_mime_type, width, height, duration_seconds, size_bytes,
    error_message, created_at, updated_at
"#;

/// Turns a row into its wire form, minting a presigned URL for whichever
/// object is currently the best one to show.
///
/// Called per attachment because each URL is individually signed. Fine for the
/// handful an artifact carries; if a listing ever spans hundreds, these should
/// be generated concurrently.
async fn to_view(state: &AppState, row: Attachment) -> AppResult<AttachmentView> {
    // Prefer processed output, but fall back to the original so a photo shows
    // up straight after upload rather than as a gap until the worker runs.
    let (key, mime) = match (&row.processed_storage_key, &row.processed_mime_type) {
        (Some(key), Some(mime)) => (Some(key.clone()), mime.clone()),
        _ => match row.status {
            // A pending upload has no bytes yet, and a failed one has nothing
            // worth showing.
            ProcessingStatus::PendingUpload | ProcessingStatus::Failed => {
                (None, row.original_mime_type.clone())
            }
            _ => (
                Some(row.original_storage_key.clone()),
                row.original_mime_type.clone(),
            ),
        },
    };

    let url = match key {
        Some(key) => Some(state.storage.presigned_get(&key).await?),
        None => None,
    };

    Ok(AttachmentView {
        id: row.id,
        artifact_id: row.artifact_id,
        kind: row.kind,
        position: row.position,
        caption: row.caption,
        original_filename: row.original_filename,
        status: row.status,
        mime_type: mime,
        width: row.width,
        height: row.height,
        duration_seconds: row.duration_seconds,
        size_bytes: row.size_bytes,
        error_message: row.error_message,
        url,
        created_at: row.created_at,
    })
}

/// Attachments for one artifact, ready to embed.
///
/// Exposed for `artifacts::routes` so artifact detail can include media
/// without reaching into the `attachments` table itself -- each module owns
/// its own tables (docs/DESIGN.md §2.3).
pub async fn list_for_artifact(
    state: &AppState,
    artifact_id: Uuid,
    include_pending: bool,
) -> AppResult<Vec<AttachmentView>> {
    let rows = sqlx::query_as::<_, Attachment>(&format!(
        r#"
        SELECT {SELECT_COLUMNS}
        FROM attachments
        WHERE artifact_id = $1
          AND ($2 OR status <> 'pending_upload')
        ORDER BY position, created_at
        "#
    ))
    .bind(artifact_id)
    .bind(include_pending)
    .fetch_all(&state.db)
    .await?;

    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        views.push(to_view(state, row).await?);
    }
    Ok(views)
}

async fn list_for_artifact_route(
    identity: Identity,
    State(state): State<AppState>,
    Path(artifact_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    // Readers see finished media; authors also see intents still awaiting
    // bytes, which they need in order to manage a half-finished upload.
    let attachments = list_for_artifact(&state, artifact_id, identity.is_author()).await?;
    Ok(Json(json!({ "attachments": attachments })))
}

/// Step one of an upload: reserve a row and hand back a presigned PUT URL.
///
/// The bytes go straight from the client to the bucket, never through this
/// process (DESIGN.md §2.5).
async fn upload_intent(
    editor: RequireEditor,
    State(state): State<AppState>,
    Path(artifact_id): Path<Uuid>,
    Json(body): Json<UploadIntentRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let media = classify(&body.mime_type)?;
    let filename = sanitize_filename(&body.filename);

    let artifact_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
            .bind(artifact_id)
            .fetch_optional(&state.db)
            .await?;

    if artifact_exists.is_none() {
        return Err(AppError::NotFound("artifact"));
    }

    let attachment_id = Uuid::new_v4();

    // The key is derived entirely from server-generated values. Building it
    // from the client's filename would invite both collisions and traversal.
    let storage_key = format!(
        "originals/{artifact_id}/{attachment_id}.{}",
        media.extension
    );

    // Append by default rather than colliding at position 0.
    let position = match body.position {
        Some(p) if p >= 0 => p,
        _ => sqlx::query_scalar::<_, Option<i32>>(
            "SELECT max(position) FROM attachments WHERE artifact_id = $1",
        )
        .bind(artifact_id)
        .fetch_one(&state.db)
        .await?
        .map(|max| max + 1)
        .unwrap_or(0),
    };

    sqlx::query(
        r#"
        INSERT INTO attachments (
            id, artifact_id, kind, position, caption, original_filename,
            original_mime_type, original_storage_key, status
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending_upload')
        "#,
    )
    .bind(attachment_id)
    .bind(artifact_id)
    .bind(media.kind)
    .bind(position)
    .bind(body.caption.as_deref().map(str::trim))
    .bind(&filename)
    .bind(media.canonical_mime)
    .bind(&storage_key)
    .execute(&state.db)
    .await?;

    let upload_url = state
        .storage
        .presigned_put(&storage_key, media.canonical_mime)
        .await?;

    tracing::info!(
        actor = %editor.0.id,
        %artifact_id,
        %attachment_id,
        "upload intent issued"
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "attachment_id": attachment_id,
            "upload_url": upload_url,
            "expires_in_seconds": state.settings.presign_ttl.as_secs(),
            // The PUT must carry this exact header: it is signed into the URL,
            // so an upload claiming a different type will be rejected by the
            // bucket rather than silently stored as something else.
            "required_headers": { "content-type": media.canonical_mime },
        })),
    ))
}

/// Step two: confirm the bytes landed, then hand the row to the worker.
///
/// The object is verified against the bucket rather than taken on trust. A
/// client that skipped the PUT would otherwise queue a processing job for an
/// object that does not exist, and the worker would fail it.
async fn complete_upload(
    editor: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let row = sqlx::query_as::<_, Attachment>(&format!(
        "SELECT {SELECT_COLUMNS} FROM attachments WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attachment"))?;

    // Idempotent: a client retrying after a dropped response should not
    // re-queue work that is already underway or finished.
    if row.status != ProcessingStatus::PendingUpload {
        return Ok(Json(json!({
            "attachment_id": row.id,
            "status": row.status,
            "note": "already completed",
        })));
    }

    let Some(size) = state.storage.head(&row.original_storage_key).await? else {
        return Err(AppError::BadRequest(
            "no object found at the upload URL; PUT the file before completing".into(),
        ));
    };

    // A presigned PUT cannot reject an oversized body while it streams, so the
    // limit is enforced here -- and the object is removed rather than left to
    // occupy the bucket.
    let max = state.settings.max_upload_bytes;
    if size > max {
        state.storage.delete(&row.original_storage_key).await?;

        sqlx::query(
            "UPDATE attachments SET status = 'failed', error_message = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(format!("upload of {size} bytes exceeds the {max} byte limit"))
        .execute(&state.db)
        .await?;

        return Err(AppError::BadRequest(format!(
            "upload of {size} bytes exceeds the {max} byte limit"
        )));
    }

    // Only now does the row become visible to the worker's SKIP LOCKED poll.
    sqlx::query("UPDATE attachments SET status = 'queued', size_bytes = $2 WHERE id = $1")
        .bind(id)
        .bind(size)
        .execute(&state.db)
        .await?;

    tracing::info!(actor = %editor.0.id, attachment_id = %id, size, "upload completed, queued");

    Ok(Json(json!({
        "attachment_id": id,
        "status": "queued",
        "size_bytes": size,
    })))
}

async fn detail(
    _identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let row = sqlx::query_as::<_, Attachment>(&format!(
        "SELECT {SELECT_COLUMNS} FROM attachments WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attachment"))?;

    Ok(Json(json!({ "attachment": to_view(&state, row).await? })))
}

async fn update(
    _editor: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAttachmentRequest>,
) -> AppResult<Json<Value>> {
    if body.caption.is_none() && body.position.is_none() {
        return Err(AppError::BadRequest(
            "provide at least one of caption or position".into(),
        ));
    }

    if matches!(body.position, Some(p) if p < 0) {
        return Err(AppError::BadRequest("position must not be negative".into()));
    }

    let row = sqlx::query_as::<_, Attachment>(&format!(
        r#"
        UPDATE attachments SET
            caption  = CASE WHEN $2 THEN $3 ELSE caption END,
            position = COALESCE($4, position)
        WHERE id = $1
        RETURNING {SELECT_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(body.caption.is_some())
    .bind(body.caption.clone().flatten())
    .bind(body.position)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attachment"))?;

    Ok(Json(json!({ "attachment": to_view(&state, row).await? })))
}

/// Deletes the row and both objects.
///
/// The row goes first: a failed object delete leaves an orphan in the bucket,
/// which is wasted space, while the reverse order would leave a row pointing
/// at nothing, which is a broken image for every client.
async fn remove(
    editor: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let row = sqlx::query_as::<_, Attachment>(&format!(
        "DELETE FROM attachments WHERE id = $1 RETURNING {SELECT_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("attachment"))?;

    for key in [Some(row.original_storage_key), row.processed_storage_key]
        .into_iter()
        .flatten()
    {
        if let Err(err) = state.storage.delete(&key).await {
            tracing::warn!(error = ?err, %key, "orphaned object: row deleted but object remains");
        }
    }

    tracing::info!(actor = %editor.0.id, attachment_id = %id, "attachment deleted");

    Ok(Json(json!({ "deleted": true })))
}
