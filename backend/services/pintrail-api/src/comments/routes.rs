use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::model::{display_name_for, CommentRow, CommentView, CreateComment};
use crate::error::{AppError, AppResult};
use crate::identity::Identity;
use crate::readers::extractors::VerifiedReader;
use crate::state::AppState;

/// Comments a reader may post per window. Generous for a real person on a
/// walk, useless for flooding a thread.
const MAX_COMMENTS: i64 = 10;
const WINDOW_MINUTES: i64 = 10;

const MAX_BODY_CHARS: usize = 4000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/artifacts/{id}/comments",
            get(list).post(create),
        )
        .route("/comments/{id}", delete(remove))
}

const SELECT_COLUMNS: &str = "c.id, c.artifact_id, c.reader_id, c.body, c.status, \
                              r.display_name, c.created_at, c.updated_at";

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Visible comments on one artifact, newest first.
///
/// Hidden and flagged comments are absent for everyone, moderators included —
/// the moderation queue is its own admin route, and a hidden comment should
/// not reappear in the public thread just because an admin is reading it.
async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Path(artifact_id): Path<Uuid>,
    Query(params): Query<ListParams>,
) -> AppResult<Json<Value>> {
    let limit = params.limit.clamp(1, 200);
    let offset = params.offset.max(0);

    let rows = sqlx::query_as::<_, CommentRow>(&format!(
        r#"
        SELECT {SELECT_COLUMNS}
        FROM comments c
        JOIN readers r ON r.id = c.reader_id
        WHERE c.artifact_id = $1 AND c.status = 'visible'
        ORDER BY c.created_at DESC
        LIMIT $2 OFFSET $3
        "#
    ))
    .bind(artifact_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let caller = identity.id();
    let comments: Vec<CommentView> = rows.into_iter().map(|r| to_view(r, caller)).collect();

    Ok(Json(json!({ "comments": comments })))
}

/// Posts a comment.
///
/// `VerifiedReader` rather than `Identity`: comments are a reader concept —
/// `comments.reader_id` references `readers` — and §2.6 requires a confirmed
/// address before posting.
async fn create(
    reader: VerifiedReader,
    State(state): State<AppState>,
    Path(artifact_id): Path<Uuid>,
    Json(body): Json<CreateComment>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let text = body.body.trim();

    if text.is_empty() {
        return Err(AppError::BadRequest("comment body is required".into()));
    }
    if text.chars().count() > MAX_BODY_CHARS {
        return Err(AppError::BadRequest(format!(
            "comment must be at most {MAX_BODY_CHARS} characters"
        )));
    }

    let artifact_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
            .bind(artifact_id)
            .fetch_optional(&state.db)
            .await?;

    if artifact_exists.is_none() {
        return Err(AppError::NotFound("artifact"));
    }

    // Counted from the table rather than an in-memory limiter: exact, shared
    // across replicas, and it survives a restart, none of which the
    // process-local limiter guarding the auth routes can offer. The
    // `comments_reader_created_idx` index exists for this query.
    // `mins` is an integer parameter -- only make_interval's `secs` accepts a
    // double, which is why the worker's sweep can pass one and this cannot.
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM comments \
         WHERE reader_id = $1 AND created_at > now() - make_interval(mins => $2)",
    )
    .bind(reader.0.id)
    .bind(WINDOW_MINUTES as i32)
    .fetch_one(&state.db)
    .await?;

    if recent >= MAX_COMMENTS {
        tracing::warn!(reader_id = %reader.0.id, recent, "comment rate limit hit");
        return Err(AppError::TooManyRequests);
    }

    let row = sqlx::query_as::<_, CommentRow>(&format!(
        r#"
        WITH inserted AS (
            INSERT INTO comments (artifact_id, reader_id, body)
            VALUES ($1, $2, $3)
            RETURNING id, artifact_id, reader_id, body, status, created_at, updated_at
        )
        SELECT c.id, c.artifact_id, c.reader_id, c.body, c.status,
               r.display_name, c.created_at, c.updated_at
        FROM inserted c
        JOIN readers r ON r.id = c.reader_id
        "#
    ))
    .bind(artifact_id)
    .bind(reader.0.id)
    .bind(text)
    .fetch_one(&state.db)
    .await?;

    tracing::info!(comment_id = %row.id, reader_id = %reader.0.id, "comment posted");

    let caller = reader.0.id;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "comment": to_view(row, caller) })),
    ))
}

/// Deletes one's own comment.
///
/// Admin takedown is a separate route: this one only ever matches the caller's
/// own rows, so it cannot be turned into a moderation tool by guessing ids.
async fn remove(
    reader: VerifiedReader,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let deleted = sqlx::query("DELETE FROM comments WHERE id = $1 AND reader_id = $2")
        .bind(id)
        .bind(reader.0.id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if deleted == 0 {
        // Someone else's comment reports as missing rather than forbidden:
        // a 403 would confirm the id exists.
        return Err(AppError::NotFound("comment"));
    }

    Ok(Json(json!({ "deleted": true })))
}

fn to_view(row: CommentRow, caller: Uuid) -> CommentView {
    CommentView {
        id: row.id,
        artifact_id: row.artifact_id,
        author: display_name_for(row.reader_id, row.display_name.as_deref()),
        body: row.body,
        is_mine: row.reader_id == caller,
        created_at: row.created_at,
    }
}
