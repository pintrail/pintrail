use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::model::{
    CreateTrail, ReplaceStops, StopInput, Trail, TrailDetail, TrailStopRow, TrailStopView,
    TrailSummary, TrailVisibility, UpdateTrail,
};
use crate::crypto::generate_token;
use crate::error::{AppError, AppResult};
use crate::identity::{Identity, OwnerType};
use crate::state::AppState;

/// A trail with more stops than this is not a walk anyone is taking; the cap
/// exists so one request cannot insert unbounded rows.
const MAX_STOPS: usize = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/trails", get(list).post(create))
        .route("/trails/{id}", get(detail).patch(update).delete(remove))
        .route("/trails/{id}/stops", put(replace_stops))
        .route("/trails/shared/{share_token}", get(resolve_share_token))
}

const TRAIL_COLUMNS: &str = "id, title, description, owner_type, owner_id, \
                             visibility, share_token, created_at, updated_at";

/// Stops joined to their artifacts, with coordinates resolved through the
/// parent chain the same way `/artifacts/sync` does — a stop at an indoor
/// artifact still needs a position to walk to.
const STOPS_QUERY: &str = r#"
WITH RECURSIVE resolved AS (
    SELECT id, parent_id, lat, lng, lat AS effective_lat, lng AS effective_lng
    FROM artifacts WHERE parent_id IS NULL
    UNION ALL
    SELECT a.id, a.parent_id, a.lat, a.lng,
           COALESCE(a.lat, r.effective_lat),
           COALESCE(a.lng, r.effective_lng)
    FROM artifacts a JOIN resolved r ON a.parent_id = r.id
)
SELECT s.id, s.artifact_id, s.position, s.note,
       a.name AS artifact_name,
       a.kind AS artifact_kind,
       r.effective_lat AS lat,
       r.effective_lng AS lng,
       (a.deleted_at IS NOT NULL) AS artifact_deleted
FROM trail_stops s
JOIN artifacts a ON a.id = s.artifact_id
JOIN resolved r ON r.id = a.id
WHERE s.trail_id = $1
ORDER BY s.position
"#;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    /// `me` for the caller's own trails; otherwise public trails are listed.
    pub owner: Option<String>,
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
    let limit = params.limit.clamp(1, 200);
    let offset = params.offset.max(0);
    let mine = params.owner.as_deref() == Some("me");

    // Two shapes, one query: either everything this caller owns (any
    // visibility), or the public catalogue.
    let rows = sqlx::query_as::<_, (Uuid, String, String, OwnerType, TrailVisibility, i64, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT t.id, t.title, t.description, t.owner_type, t.visibility,
               (SELECT count(*) FROM trail_stops s WHERE s.trail_id = t.id) AS stop_count,
               t.created_at, t.updated_at
        FROM trails t
        WHERE CASE
                WHEN $1 THEN t.owner_type = $2 AND t.owner_id = $3
                ELSE t.visibility = 'public'
              END
        ORDER BY t.updated_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(mine)
    .bind(identity.owner_type())
    .bind(identity.id())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let trails: Vec<TrailSummary> = rows
        .into_iter()
        .map(
            |(id, title, description, owner_type, visibility, stop_count, created_at, updated_at)| {
                TrailSummary {
                    id,
                    title,
                    description,
                    owner_type,
                    visibility,
                    stop_count,
                    created_at,
                    updated_at,
                }
            },
        )
        .collect();

    Ok(Json(json!({ "trails": trails })))
}

async fn create(
    identity: Identity,
    State(state): State<AppState>,
    Json(body): Json<CreateTrail>,
) -> AppResult<(StatusCode, Json<Value>)> {
    identity.ensure_verified()?;

    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    if body.stops.len() > MAX_STOPS {
        return Err(AppError::BadRequest(format!(
            "a trail may have at most {MAX_STOPS} stops"
        )));
    }

    // An unlisted trail is reachable only by its link, so it must have one --
    // the schema enforces this too.
    let share_token = match body.visibility {
        TrailVisibility::Unlisted => Some(new_share_token()),
        _ => None,
    };

    let mut tx = state.db.begin().await?;

    let trail = sqlx::query_as::<_, Trail>(&format!(
        r#"
        INSERT INTO trails (title, description, owner_type, owner_id, visibility, share_token)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING {TRAIL_COLUMNS}
        "#
    ))
    .bind(title)
    .bind(body.description.trim())
    .bind(identity.owner_type())
    .bind(identity.id())
    .bind(body.visibility)
    .bind(share_token.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    if !body.stops.is_empty() {
        insert_stops(&mut tx, trail.id, &body.stops).await?;
    }

    tx.commit().await?;

    tracing::info!(trail_id = %trail.id, owner = %identity.id(), "trail created");

    Ok((
        StatusCode::CREATED,
        Json(json!({ "trail_id": trail.id, "share_token": trail.share_token })),
    ))
}

async fn detail(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let trail = load_trail(&state, id).await?;
    let is_owner = trail.is_owned_by(identity.owner_type(), identity.id());

    // A private trail is invisible to everyone else, and 404 rather than 403:
    // "you may not see this" still confirms it exists.
    //
    // An unlisted trail is equally invisible here — it is reachable only
    // through /trails/shared/{token}, which is the whole point of the link.
    if !is_owner && trail.visibility != TrailVisibility::Public {
        return Err(AppError::NotFound("trail"));
    }

    Ok(Json(json!({ "trail": build_detail(&state, trail, is_owner).await? })))
}

/// Resolves an unlisted share link.
///
/// The token is looked up on its own rather than alongside a trail id, so a
/// link cannot be pointed at a different trail than the one it was minted for.
async fn resolve_share_token(
    _identity: Identity,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Json<Value>> {
    let trail = sqlx::query_as::<_, Trail>(&format!(
        "SELECT {TRAIL_COLUMNS} FROM trails WHERE share_token = $1"
    ))
    .bind(token.trim())
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("trail"))?;

    // A token that outlived its visibility grants nothing. Flipping a trail
    // back to private must actually revoke the links already handed out.
    if trail.visibility == TrailVisibility::Private {
        return Err(AppError::NotFound("trail"));
    }

    Ok(Json(json!({ "trail": build_detail(&state, trail, false).await? })))
}

async fn update(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTrail>,
) -> AppResult<Json<Value>> {
    let trail = load_owned_trail(&state, id, &identity).await?;

    if let Some(title) = &body.title {
        if title.trim().is_empty() {
            return Err(AppError::BadRequest("title must not be blank".into()));
        }
    }

    // Becoming unlisted needs a link; leaving unlisted keeps the existing one
    // so a trail flipped public and back does not break URLs already shared.
    let share_token = match body.visibility {
        Some(TrailVisibility::Unlisted) if trail.share_token.is_none() => Some(new_share_token()),
        _ => trail.share_token.clone(),
    };

    let updated = sqlx::query_as::<_, Trail>(&format!(
        r#"
        UPDATE trails SET
            title       = COALESCE($2, title),
            description = COALESCE($3, description),
            visibility  = COALESCE($4, visibility),
            share_token = $5
        WHERE id = $1
        RETURNING {TRAIL_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(body.title.as_deref().map(str::trim))
    .bind(body.description.as_deref().map(str::trim))
    .bind(body.visibility)
    .bind(share_token.as_deref())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(json!({ "trail": build_detail(&state, updated, true).await? })))
}

/// Replaces the whole ordered stop list.
///
/// A full replace rather than per-stop patching: the client is a drag-to-
/// reorder list (DESIGN.md §1.7), so it already knows the final order, and
/// sending it whole avoids a reorder protocol where every intermediate state
/// has to satisfy the position uniqueness constraint. Stop rows are not
/// referenced by anything else, so replacing them loses nothing.
async fn replace_stops(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReplaceStops>,
) -> AppResult<Json<Value>> {
    identity.ensure_verified()?;
    let trail = load_owned_trail(&state, id, &identity).await?;

    if body.stops.len() > MAX_STOPS {
        return Err(AppError::BadRequest(format!(
            "a trail may have at most {MAX_STOPS} stops"
        )));
    }

    let mut tx = state.db.begin().await?;

    sqlx::query("DELETE FROM trail_stops WHERE trail_id = $1")
        .bind(trail.id)
        .execute(&mut *tx)
        .await?;

    insert_stops(&mut tx, trail.id, &body.stops).await?;

    // Reordering stops is a change to the trail, and a listing sorted by
    // updated_at should reflect it.
    sqlx::query("UPDATE trails SET updated_at = now() WHERE id = $1")
        .bind(trail.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let stops = load_stops(&state, trail.id).await?;
    Ok(Json(json!({ "stops": stops })))
}

async fn remove(
    identity: Identity,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let trail = load_owned_trail(&state, id, &identity).await?;

    // Stops go with it via ON DELETE CASCADE.
    sqlx::query("DELETE FROM trails WHERE id = $1")
        .bind(trail.id)
        .execute(&state.db)
        .await?;

    tracing::info!(trail_id = %trail.id, actor = %identity.id(), "trail deleted");

    Ok(Json(json!({ "deleted": true })))
}

// --- helpers ---------------------------------------------------------------

/// 256 bits from the OS CSPRNG.
///
/// Stored in plaintext, unlike session tokens: it has to be reconstructible
/// into a URL to be shared at all. It is a capability, not a credential — it
/// grants read of one non-private trail and nothing else.
fn new_share_token() -> String {
    generate_token(chrono::Duration::days(1)).raw
}

async fn load_trail(state: &AppState, id: Uuid) -> AppResult<Trail> {
    sqlx::query_as::<_, Trail>(&format!(
        "SELECT {TRAIL_COLUMNS} FROM trails WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("trail"))
}

/// Loads a trail the caller owns, or reports it as missing.
///
/// 404 rather than 403 for someone else's trail: a 403 confirms the id exists,
/// which is a disclosure on private content.
async fn load_owned_trail(state: &AppState, id: Uuid, identity: &Identity) -> AppResult<Trail> {
    let trail = load_trail(state, id).await?;

    if !trail.is_owned_by(identity.owner_type(), identity.id()) {
        return Err(AppError::NotFound("trail"));
    }

    Ok(trail)
}

async fn load_stops(state: &AppState, trail_id: Uuid) -> AppResult<Vec<TrailStopView>> {
    let rows = sqlx::query_as::<_, TrailStopRow>(STOPS_QUERY)
        .bind(trail_id)
        .fetch_all(&state.db)
        .await?;

    Ok(rows.into_iter().map(TrailStopView::from).collect())
}

async fn build_detail(state: &AppState, trail: Trail, is_owner: bool) -> AppResult<TrailDetail> {
    let stops = load_stops(state, trail.id).await?;

    Ok(TrailDetail {
        id: trail.id,
        title: trail.title,
        description: trail.description,
        owner_type: trail.owner_type,
        visibility: trail.visibility,
        // Anyone holding this can read the trail, so it goes only to the owner.
        share_token: is_owner.then_some(trail.share_token).flatten(),
        is_owner,
        stops,
        created_at: trail.created_at,
        updated_at: trail.updated_at,
    })
}

/// Inserts stops at sequential positions, rejecting anything that does not
/// point at a live artifact.
async fn insert_stops(
    conn: &mut PgConnection,
    trail_id: Uuid,
    stops: &[StopInput],
) -> AppResult<()> {
    for (index, stop) in stops.iter().enumerate() {
        // Checked per stop so the error names the offending artifact rather
        // than surfacing an opaque foreign key violation. Soft-deleted
        // artifacts are rejected here too: they still satisfy the FK, but
        // adding one would create a stop that is unavailable on arrival.
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
                .bind(stop.artifact_id)
                .fetch_optional(&mut *conn)
                .await?;

        if exists.is_none() {
            return Err(AppError::BadRequest(format!(
                "stop {}: artifact {} does not exist",
                index + 1,
                stop.artifact_id
            )));
        }

        sqlx::query(
            "INSERT INTO trail_stops (trail_id, artifact_id, position, note) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(trail_id)
        .bind(stop.artifact_id)
        .bind(index as i32)
        .bind(stop.note.as_deref().map(str::trim))
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}
