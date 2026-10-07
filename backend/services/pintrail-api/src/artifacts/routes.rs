use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::model::{Artifact, ArtifactDetail, CreateArtifact, SyncEntry, TopicLink, UpdateArtifact};
use crate::authors::extractors::RequireEditor;
use crate::error::{AppError, AppResult};
use crate::identity::Identity;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/artifacts", get(list).post(create))
        .route("/artifacts/sync", get(sync))
        .route("/artifacts/{id}", get(detail).patch(update).delete(remove))
}

/// Resolves each artifact's effective coordinates by propagating downward
/// from the roots.
///
/// Written as a single downward pass rather than a per-row upward walk: the
/// sync manifest covers the whole campus, and one walk per artifact would be
/// a query per row. Because the `artifacts_latlng_paired` constraint keeps
/// lat and lng either both set or both null, coalescing them independently
/// cannot mix one artifact's latitude with another's longitude.
///
/// `published` is true when the artifact and every ancestor are approved and
/// not deleted: what the phone app may show (migration
/// `..._artifact_publication`, and [`ensure_visible`]).
pub(crate) const RESOLVED_COORDS_CTE: &str = r#"
WITH RECURSIVE resolved AS (
    SELECT id, parent_id, lat, lng,
           lat AS effective_lat,
           lng AS effective_lng,
           CASE WHEN lat IS NULL THEN NULL ELSE id END AS location_source_id,
           (status = 'approved' AND deleted_at IS NULL) AS published
    FROM artifacts
    WHERE parent_id IS NULL

    UNION ALL

    SELECT a.id, a.parent_id, a.lat, a.lng,
           COALESCE(a.lat, r.effective_lat),
           COALESCE(a.lng, r.effective_lng),
           CASE WHEN a.lat IS NOT NULL THEN a.id ELSE r.location_source_id END,
           r.published AND a.status = 'approved' AND a.deleted_at IS NULL
    FROM artifacts a
    JOIN resolved r ON a.parent_id = r.id
)
"#;

/// 404s a reader asking about an artifact that isn't published. Authors see
/// every artifact, drafts included. 404 rather than 403, so a draft's
/// existence isn't disclosed.
pub(crate) async fn ensure_visible(state: &AppState, identity: &Identity, id: Uuid) -> AppResult<()> {
    if identity.is_author() {
        return Ok(());
    }
    let published: bool = sqlx::query_scalar("SELECT artifact_is_published($1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if published { Ok(()) } else { Err(AppError::NotFound("artifact")) }
}

#[derive(Debug, Deserialize)]
pub struct SyncParams {
    /// Cursor from the previous response. Absent or 0 means a full download.
    #[serde(default)]
    pub since: i64,
}

/// The manifest the phone caches for on-device geofencing (DESIGN.md §2.4).
///
/// Returns every artifact changed since the client's cursor, with coordinates
/// already resolved so the device never has to walk the parent chain itself.
async fn sync(
    identity: Identity,
    State(state): State<AppState>,
    Query(params): Query<SyncParams>,
) -> AppResult<Json<Value>> {
    let since = params.since.max(0);

    // On a first sync there is nothing cached to evict, so shipping tombstones
    // for everything ever deleted would be pure waste. Incremental syncs need
    // them.
    let include_deleted = since > 0;

    // Readers see only published artifacts. For them, an artifact that stops
    // being published (sent back, withdrawn, or under an unapproved parent)
    // goes out as a tombstone so the phone evicts it, exactly like a delete.
    let reader = !identity.is_author();

    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.kind, a.name, a.parent_id,
               r.effective_lat AS lat,
               r.effective_lng AS lng,
               a.beacon_id, a.is_topic,
               ARRAY(SELECT l.topic_id FROM artifact_topics l JOIN artifacts t ON t.id = l.topic_id
                     WHERE l.artifact_id = a.id AND t.deleted_at IS NULL
                       AND (NOT $3 OR (t.status = 'approved'))
                     ORDER BY lower(t.name), t.id) AS topic_ids,
               a.sync_version,
               (a.deleted_at IS NOT NULL OR ($3 AND NOT r.published)) AS deleted
        FROM artifacts a
        JOIN resolved r ON r.id = a.id
        WHERE a.sync_version > $1
          AND ($2 OR (a.deleted_at IS NULL AND (NOT $3 OR r.published)))
        ORDER BY a.sync_version
        "#
    );

    let entries = sqlx::query_as::<_, SyncEntry>(&sql)
        .bind(since)
        .bind(include_deleted)
        .bind(reader)
        .fetch_all(&state.db)
        .await?;

    // The cursor is the highest version actually returned, not the sequence's
    // current value. Taking the sequence value could skip a row committed by
    // a slower concurrent transaction that holds a lower version.
    let version = entries
        .iter()
        .map(|e| e.sync_version)
        .max()
        .unwrap_or(since);

    Ok(Json(json!({
        "version": version,
        "count": entries.len(),
        "artifacts": entries,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub parent_id: Option<Uuid>,
    /// List direct children of nothing, i.e. top-level artifacts only.
    /// Topics have no parent but aren't places, so they're left out.
    #[serde(default)]
    pub roots_only: bool,
    /// `true` lists only topics, `false` only places; absent lists both.
    pub topics: Option<bool>,
    /// Only the artifacts linked to this topic.
    pub topic_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list(
    identity: Identity,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> AppResult<Json<Value>> {
    // Bounded so a client cannot ask for the entire table in one response.
    let limit = params.limit.clamp(1, 200);
    let offset = params.offset.max(0);

    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.kind, a.name, a.description,
               a.lat, a.lng,
               r.effective_lat, r.effective_lng, r.location_source_id,
               a.parent_id, a.beacon_id, a.is_topic, a.created_at, a.updated_at
        FROM artifacts a
        JOIN resolved r ON r.id = a.id
        WHERE a.deleted_at IS NULL
          AND ($1::uuid IS NULL OR a.parent_id = $1)
          AND (NOT $2 OR (a.parent_id IS NULL AND NOT a.is_topic))
          AND (NOT $5 OR r.published)
          AND ($6::bool IS NULL OR a.is_topic = $6)
          AND ($7::uuid IS NULL OR EXISTS (
                SELECT 1 FROM artifact_topics l WHERE l.artifact_id = a.id AND l.topic_id = $7))
        ORDER BY a.name, a.id
        LIMIT $3 OFFSET $4
        "#
    );

    let artifacts = sqlx::query_as::<_, ArtifactDetail>(&sql)
        .bind(params.parent_id)
        .bind(params.roots_only)
        .bind(limit)
        .bind(offset)
        .bind(!identity.is_author())
        .bind(params.topics)
        .bind(params.topic_id)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(json!({ "artifacts": artifacts })))
}

async fn detail(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.kind, a.name, a.description,
               a.lat, a.lng,
               r.effective_lat, r.effective_lng, r.location_source_id,
               a.parent_id, a.beacon_id, a.is_topic, a.created_at, a.updated_at
        FROM artifacts a
        JOIN resolved r ON r.id = a.id
        WHERE a.id = $1 AND a.deleted_at IS NULL
          AND (NOT $2 OR r.published)
        "#
    );

    let artifact = sqlx::query_as::<_, ArtifactDetail>(&sql)
        .bind(id)
        .bind(!identity.is_author())
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("artifact"))?;

    // Media comes back with the artifact so opening one is a single round
    // trip (DESIGN.md §2.7). Fetched through the attachments module rather
    // than by querying its table here, so table ownership stays intact.
    let attachments =
        crate::attachments::list_for_artifact(&state, id, identity.is_author()).await?;

    // The other ends of its topic links: an artifact's topics, or a topic's
    // artifacts. A reader sees only the published ones.
    let reader = !identity.is_author();
    let sql = if artifact.is_topic {
        "SELECT a.id, a.name, l.note FROM artifact_topics l JOIN artifacts a ON a.id = l.artifact_id \
         WHERE l.topic_id = $1 AND a.deleted_at IS NULL AND (NOT $2 OR artifact_is_published(a.id)) \
         ORDER BY lower(a.name), a.id"
    } else {
        "SELECT t.id, t.name, l.note FROM artifact_topics l JOIN artifacts t ON t.id = l.topic_id \
         WHERE l.artifact_id = $1 AND t.deleted_at IS NULL AND (NOT $2 OR artifact_is_published(t.id)) \
         ORDER BY lower(t.name), t.id"
    };
    let links = sqlx::query_as::<_, TopicLink>(sql)
        .bind(id)
        .bind(reader)
        .fetch_all(&state.db)
        .await?;
    let key = if artifact.is_topic { "linked_artifacts" } else { "topics" };

    Ok(Json(json!({ "artifact": artifact, "attachments": attachments, key: links })))
}

async fn create(
    editor: RequireEditor,
    State(state): State<AppState>,
    Json(body): Json<CreateArtifact>,
) -> AppResult<(StatusCode, Json<Value>)> {
    validate_coords(body.lat, body.lng)?;

    let mut tx = crate::audit::begin_as(&state.db, editor.0.id).await?;
    let artifact = sqlx::query_as::<_, Artifact>(
        r#"
        INSERT INTO artifacts (kind, name, description, lat, lng, parent_id, beacon_id, is_topic)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, kind, name, description, lat, lng, parent_id, beacon_id,
                  sync_version, deleted_at, created_at, updated_at
        "#,
    )
    .bind(body.kind)
    .bind(body.name.trim())
    .bind(body.description.trim())
    .bind(body.lat)
    .bind(body.lng)
    .bind(body.parent_id)
    .bind(body.beacon_id.as_deref())
    .bind(body.is_topic)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_artifact_error)?;
    tx.commit().await?;

    tracing::info!(actor = %editor.0.id, artifact_id = %artifact.id, "artifact created");

    Ok((StatusCode::CREATED, Json(json!({ "artifact_id": artifact.id }))))
}

async fn update(
    editor: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateArtifact>,
) -> AppResult<Json<Value>> {
    // Coordinates move as a pair. Setting one while clearing the other would
    // trip the database constraint as a 500; catching it here makes it a 400
    // that explains itself.
    match (&body.lat, &body.lng) {
        (Some(lat), Some(lng)) => validate_coords(*lat, *lng)?,
        (Some(_), None) | (None, Some(_)) => {
            return Err(AppError::BadRequest(
                "lat and lng must be updated together".into(),
            ))
        }
        (None, None) => {}
    }

    if body.parent_id.map(|p| p == Some(id)).unwrap_or(false) {
        return Err(AppError::BadRequest(
            "an artifact cannot be its own parent".into(),
        ));
    }

    let mut tx = crate::audit::begin_as(&state.db, editor.0.id).await?;
    crate::audit::ensure_can_modify(&mut *tx, &editor.0, id).await?;
    let artifact = sqlx::query_as::<_, Artifact>(
        r#"
        UPDATE artifacts SET
            kind        = COALESCE($2, kind),
            name        = COALESCE($3, name),
            description = COALESCE($4, description),
            lat         = CASE WHEN $5 THEN $6 ELSE lat END,
            lng         = CASE WHEN $5 THEN $7 ELSE lng END,
            parent_id   = CASE WHEN $8 THEN $9 ELSE parent_id END,
            beacon_id   = CASE WHEN $10 THEN $11 ELSE beacon_id END
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING id, kind, name, description, lat, lng, parent_id, beacon_id,
                  sync_version, deleted_at, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(body.kind)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.lat.is_some())
    .bind(body.lat.flatten())
    .bind(body.lng.flatten())
    .bind(body.parent_id.is_some())
    .bind(body.parent_id.flatten())
    .bind(body.beacon_id.is_some())
    .bind(body.beacon_id.clone().flatten())
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_artifact_error)?
    .ok_or(AppError::NotFound("artifact"))?;
    crate::audit::reopen_if_approved(&mut tx, &editor.0, id).await?;
    tx.commit().await?;

    tracing::info!(actor = %editor.0.id, artifact_id = %artifact.id, "artifact updated");

    Ok(Json(json!({ "artifact_id": artifact.id, "sync_version": artifact.sync_version })))
}

/// Soft-deletes an artifact and everything beneath it.
///
/// Soft rather than hard so `/artifacts/sync` can hand clients a tombstone;
/// a row that simply vanished would leave every phone geofencing it forever.
/// Descendants go too, because a child whose parent is gone would inherit
/// coordinates from a deleted ancestor.
async fn remove(
    editor: RequireEditor,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let mut tx = crate::audit::begin_as(&state.db, editor.0.id).await?;

    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

    if exists.is_none() {
        return Err(AppError::NotFound("artifact"));
    }
    if !crate::audit::can_delete(&mut *tx, &editor.0, id).await? {
        return Err(AppError::Forbidden);
    }

    let deleted = sqlx::query(
        r#"
        WITH RECURSIVE subtree AS (
            SELECT id FROM artifacts WHERE id = $1
            UNION ALL
            SELECT a.id FROM artifacts a JOIN subtree s ON a.parent_id = s.id
        )
        UPDATE artifacts SET deleted_at = now()
        WHERE id IN (SELECT id FROM subtree) AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;

    tracing::info!(actor = %editor.0.id, artifact_id = %id, deleted, "artifact subtree deleted");

    Ok(Json(json!({ "deleted": deleted })))
}

pub(crate) fn validate_coords(lat: Option<f64>, lng: Option<f64>) -> AppResult<()> {
    match (lat, lng) {
        (None, None) => Ok(()),
        (Some(lat), Some(lng)) => {
            if !(-90.0..=90.0).contains(&lat) {
                return Err(AppError::BadRequest(
                    "lat must be between -90 and 90".into(),
                ));
            }
            if !(-180.0..=180.0).contains(&lng) {
                return Err(AppError::BadRequest(
                    "lng must be between -180 and 180".into(),
                ));
            }
            Ok(())
        }
        _ => Err(AppError::BadRequest(
            "lat and lng must be provided together, or both omitted to inherit \
             from the parent"
                .into(),
        )),
    }
}

/// Turns the schema's integrity errors into responses that say what to fix.
/// Without this the cycle trigger and the parent foreign key both surface as
/// an opaque 500.
fn map_artifact_error(err: sqlx::Error) -> AppError {
    let sqlx::Error::Database(db) = &err else {
        return AppError::from(err);
    };

    match db.code().as_deref() {
        // check_violation -- raised by the acyclic and coordinate constraints
        Some("23514") => AppError::BadRequest(db.message().to_string()),
        // foreign_key_violation -- parent_id points at nothing
        Some("23503") => AppError::BadRequest("parent_id does not exist".into()),
        // unique_violation -- beacon_id already claimed
        Some("23505") => {
            AppError::Conflict("that beacon_id is already assigned to another artifact".into())
        }
        _ => AppError::from(err),
    }
}
