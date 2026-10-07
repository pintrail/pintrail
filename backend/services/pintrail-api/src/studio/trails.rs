//! Trails in the Studio: an ordered walk through artifacts, with a title, a
//! description, a note at each stop, and a visibility.
//!
//! These are the curated, author-owned trails. Explorers build their own in
//! the app through the JSON API (`trails/`), and admins can take those down
//! from the admin panel; the Studio lists only trails owned by authors.
//!
//! Who may do what mirrors artifacts: an editor changes the trails they
//! created, an admin changes any. Making a trail public puts it in front of
//! everyone using the app, so only an admin may do that; editors keep their
//! trails private or share them by link while they build them.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use minijinja::{context, Value};
use serde::Deserialize;
use serde_json::{json, Value as Json};
use uuid::Uuid;

use super::routes::{base_ctx, environment, render, require_csrf, require_editor, respond, session_token, CsrfForm};
use crate::artifacts::routes::RESOLVED_COORDS_CTE;
use crate::audit;
use crate::authors::extractors::AuthenticatedAuthor;
use crate::authors::model::Author;
use crate::error::{AppError, AppResult};
use crate::identity::OwnerType;
use crate::state::AppState;
use crate::trails::model::TrailVisibility;
use crate::trails::routes::{new_share_token, MAX_STOPS};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio/trails", get(list).post(create))
        .route("/studio/trails/new", get(new_form))
        .route("/studio/trails/{id}", get(detail).post(update))
        .route("/studio/trails/{id}/edit", get(edit_form))
        .route("/studio/trails/{id}/delete", post(remove))
        .route("/studio/trails/{id}/stops", get(stops_fragment).post(add_stop))
        .route("/studio/trails/{id}/stops/order", post(reorder_stops))
        .route("/studio/trail-stops/{id}", post(update_stop))
        .route("/studio/trail-stops/{id}/delete", post(delete_stop))
}

// --- loading -----------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TrailRow {
    id: Uuid,
    title: String,
    description: String,
    owner_id: Uuid,
    visibility: TrailVisibility,
    share_token: Option<String>,
    updated_at: DateTime<Utc>,
}

/// An author-owned trail, or 404.
async fn load_trail(state: &AppState, id: Uuid) -> AppResult<TrailRow> {
    sqlx::query_as::<_, TrailRow>(
        "SELECT id, title, description, owner_id, visibility, share_token, updated_at \
         FROM trails WHERE id = $1 AND owner_type = 'author'",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("trail"))
}

fn may_modify(author: &Author, trail: &TrailRow) -> bool {
    audit::is_admin(author) || (require_editor(author).is_ok() && trail.owner_id == author.id)
}

async fn load_owned(state: &AppState, author: &Author, id: Uuid) -> AppResult<TrailRow> {
    let trail = load_trail(state, id).await?;
    if may_modify(author, &trail) { Ok(trail) } else { Err(AppError::Forbidden) }
}

fn visibility_str(v: TrailVisibility) -> &'static str {
    match v {
        TrailVisibility::Private => "private",
        TrailVisibility::Unlisted => "unlisted",
        TrailVisibility::Public => "public",
    }
}

fn parse_visibility(s: &str) -> AppResult<TrailVisibility> {
    match s {
        "private" => Ok(TrailVisibility::Private),
        "unlisted" => Ok(TrailVisibility::Unlisted),
        "public" => Ok(TrailVisibility::Public),
        _ => Err(AppError::BadRequest("choose who can see the trail".into())),
    }
}

/// Marks the trail as changed now, so the list sorts by recent work.
async fn touch(state: &AppState, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE trails SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

// --- the list ----------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ListRow {
    id: Uuid,
    title: String,
    description: String,
    owner_id: Uuid,
    visibility: TrailVisibility,
    stop_count: i64,
    unpublished: i64,
    updated_at: DateTime<Utc>,
}

async fn list(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    let rows = sqlx::query_as::<_, ListRow>(
        r#"
        SELECT t.id, t.title, t.description, t.owner_id, t.visibility, t.updated_at,
               (SELECT count(*) FROM trail_stops s WHERE s.trail_id = t.id) AS stop_count,
               (SELECT count(*) FROM trail_stops s WHERE s.trail_id = t.id
                  AND NOT artifact_is_published(s.artifact_id)) AS unpublished
        FROM trails t
        WHERE t.owner_type = 'author'
        ORDER BY t.updated_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.owner_id).collect();
    let people = super::profile::load_people(&state.db, &ids).await?;
    let trails: Vec<Json> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(), "title": r.title, "description": r.description,
                "owner": people.get(&r.owner_id), "mine": r.owner_id == author.0.id,
                "visibility": visibility_str(r.visibility), "stop_count": r.stop_count,
                "unpublished": r.unpublished, "updated_at": r.updated_at.to_rfc3339(),
            })
        })
        .collect();
    let ctx = context! {
        title => "Trails",
        trails => Value::from_serialize(&trails),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_trails.html", ctx)
}

// --- one trail -------------------------------------------------------------------

async fn detail(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let ctx = detail_ctx(&state, &author.0, &jar, id).await?;
    respond(&environment(), &headers, "studio_trail.html", ctx)
}

async fn detail_ctx(state: &AppState, author: &Author, jar: &CookieJar, id: Uuid) -> AppResult<Value> {
    let t = load_trail(state, id).await?;
    let people = super::profile::load_people(&state.db, &[t.owner_id]).await?;
    let trail = json!({
        "id": t.id.to_string(), "title": t.title, "description": t.description,
        "owner": people.get(&t.owner_id), "visibility": visibility_str(t.visibility),
        "share_token": t.share_token, "updated_at": t.updated_at.to_rfc3339(),
    });
    Ok(context! {
        title => t.title.clone(),
        trail => Value::from_serialize(&trail),
        can_edit => may_modify(author, &t),
        can_create => require_editor(author).is_ok(),
        ..base_ctx(author, &session_token(jar))
    })
}

/// The trail and its effects: one success response for every change.
async fn after_change(state: &AppState, author: &Author, jar: &CookieJar, headers: &HeaderMap, id: Uuid) -> AppResult<Response> {
    let ctx = detail_ctx(state, author, jar, id).await?;
    let body = respond(&environment(), headers, "studio_trail.html", ctx)?;
    Ok(([("HX-Push-Url", format!("/studio/trails/{id}"))], body).into_response())
}

// --- create / edit ------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TrailForm {
    csrf_token: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    visibility: String,
}

async fn new_form(author: AuthenticatedAuthor, jar: CookieJar, headers: HeaderMap) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    let form = context! { title => "", description => "", visibility => "private" };
    render_form(&author.0, &jar, &headers, None, form, None)
}

async fn edit_form(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let t = load_owned(&state, &author.0, id).await?;
    let form = context! {
        title => t.title, description => t.description, visibility => visibility_str(t.visibility),
    };
    render_form(&author.0, &jar, &headers, Some(id), form, None)
}

fn render_form(
    author: &Author,
    jar: &CookieJar,
    headers: &HeaderMap,
    edit_id: Option<Uuid>,
    form: Value,
    error: Option<&str>,
) -> AppResult<Html<String>> {
    let (title, action, submit) = match edit_id {
        Some(id) => ("Edit trail", format!("/studio/trails/{id}"), "Save changes"),
        None => ("New trail", "/studio/trails".to_string(), "Create trail"),
    };
    let ctx = context! {
        title => title, heading => title, action => action, submit_label => submit,
        cancel_id => edit_id.map(|u| u.to_string()),
        form => form, error => error,
        ..base_ctx(author, &session_token(jar))
    };
    respond(&environment(), headers, "studio_trail_form.html", ctx)
}

/// Validates a submitted form; on failure returns the message to show.
fn validate(author: &Author, f: &TrailForm, current: Option<TrailVisibility>) -> Result<TrailVisibility, String> {
    let title = f.title.trim();
    if title.is_empty() {
        return Err("Give the trail a title.".into());
    }
    if title.chars().count() > 200 {
        return Err("The title can be at most 200 characters.".into());
    }
    let vis = parse_visibility(&f.visibility).map_err(|e| e.to_string())?;
    if vis == TrailVisibility::Public && current != Some(TrailVisibility::Public) && !audit::is_admin(author) {
        return Err("Only an admin can make a trail public. Keep it private or share it by link, and ask your instructor to publish it.".into());
    }
    Ok(vis)
}

async fn create(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Form(f): Form<TrailForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &f.csrf_token)?;
    let vis = match validate(&author.0, &f, None) {
        Ok(v) => v,
        Err(msg) => {
            let form = context! { title => f.title, description => f.description, visibility => f.visibility };
            return Ok(render_form(&author.0, &jar, &headers, None, form, Some(&msg))?.into_response());
        }
    };
    let token = (vis == TrailVisibility::Unlisted).then(new_share_token);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO trails (title, description, owner_type, owner_id, visibility, share_token) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(f.title.trim())
    .bind(f.description.trim())
    .bind(OwnerType::Author)
    .bind(author.0.id)
    .bind(vis)
    .bind(token)
    .fetch_one(&state.db)
    .await?;
    tracing::info!(trail_id = %id, actor = %author.0.id, "trail created in the studio");
    after_change(&state, &author.0, &jar, &headers, id).await
}

async fn update(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(f): Form<TrailForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    let t = load_owned(&state, &author.0, id).await?;
    let vis = match validate(&author.0, &f, Some(t.visibility)) {
        Ok(v) => v,
        Err(msg) => {
            let form = context! { title => f.title, description => f.description, visibility => f.visibility };
            return Ok(render_form(&author.0, &jar, &headers, Some(id), form, Some(&msg))?.into_response());
        }
    };
    // Becoming unlisted needs a link; leaving unlisted keeps it, so a trail
    // flipped back later doesn't break links already handed out.
    let token = match (vis, t.share_token) {
        (TrailVisibility::Unlisted, None) => Some(new_share_token()),
        (_, existing) => existing,
    };
    sqlx::query(
        "UPDATE trails SET title = $2, description = $3, visibility = $4, share_token = $5 WHERE id = $1",
    )
    .bind(id)
    .bind(f.title.trim())
    .bind(f.description.trim())
    .bind(vis)
    .bind(token)
    .execute(&state.db)
    .await?;
    after_change(&state, &author.0, &jar, &headers, id).await
}

async fn remove(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(f): Form<CsrfForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    load_owned(&state, &author.0, id).await?;
    sqlx::query("DELETE FROM trails WHERE id = $1").bind(id).execute(&state.db).await?;
    tracing::info!(trail_id = %id, actor = %author.0.id, "trail deleted in the studio");
    let body = list(author, State(state), jar, headers).await?;
    Ok(([("HX-Push-Url", "/studio/trails")], body).into_response())
}

// --- stops ------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct StopRow {
    id: Uuid,
    artifact_id: Uuid,
    note: Option<String>,
    name: String,
    kind: String,
    status: String,
    deleted: bool,
    published: bool,
    lat: Option<f64>,
    lng: Option<f64>,
    parent_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PickRow {
    id: Uuid,
    name: String,
    kind: String,
    parent_id: Option<Uuid>,
    status: String,
}

/// The stops card: the ordered list, the map, and (for editors) the picker.
async fn render_stops(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    trail_id: Uuid,
    error: Option<&str>,
    edit_id: Option<Uuid>,
) -> AppResult<Html<String>> {
    let t = load_trail(state, trail_id).await?;
    let can_edit = may_modify(author, &t);
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT s.id, s.artifact_id, s.note, a.name, a.kind::text AS kind, a.status::text AS status,
               (a.deleted_at IS NOT NULL) AS deleted, artifact_is_published(a.id) AS published,
               r.effective_lat AS lat, r.effective_lng AS lng,
               (SELECT p.name FROM artifacts p WHERE p.id = a.parent_id) AS parent_name
        FROM trail_stops s
        JOIN artifacts a ON a.id = s.artifact_id
        JOIN resolved r ON r.id = a.id
        WHERE s.trail_id = $1
        ORDER BY s.position
        "#
    );
    let rows = sqlx::query_as::<_, StopRow>(&sql).bind(trail_id).fetch_all(&state.db).await?;
    let stops: Vec<Json> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "id": r.id.to_string(), "n": i + 1, "artifact_id": r.artifact_id.to_string(),
                "note": r.note.clone().unwrap_or_default(), "name": r.name, "kind": r.kind,
                "status": r.status, "deleted": r.deleted, "published": r.published,
                "lat": r.lat, "lng": r.lng, "parent_name": r.parent_name,
            })
        })
        .collect();
    let map_points: Vec<Json> = stops
        .iter()
        .filter(|s| !s["deleted"].as_bool().unwrap_or(false) && s["lat"].is_number())
        .map(|s| json!({ "n": s["n"], "name": s["name"], "lat": s["lat"], "lng": s["lng"], "artifact_id": s["artifact_id"] }))
        .collect();
    let picks = if can_edit { load_pick_options(state).await? } else { Vec::new() };
    let warn = stops.iter().filter(|s| !s["published"].as_bool().unwrap_or(false)).count();
    render(
        &environment(),
        "studio_trail_stops.html",
        context! {
            trail_id => trail_id.to_string(),
            stops => Value::from_serialize(&stops),
            map_data => super::routes::script_json(&json!({ "points": map_points })),
            picks => Value::from_serialize(&picks),
            can_edit => can_edit,
            unpublished => warn,
            error => error,
            edit_id => edit_id.map(|u| u.to_string()),
            csrf_token => crate::admin::csrf::token_for_session(&session_token(jar)),
        },
    )
}

/// Every artifact, in tree order, labelled with its place in the tree
/// ("Building › Room"), for the add-a-stop picker.
async fn load_pick_options(state: &AppState) -> AppResult<Vec<Json>> {
    let rows = sqlx::query_as::<_, PickRow>(
        "SELECT id, name, kind::text AS kind, parent_id, status::text AS status \
         FROM artifacts WHERE deleted_at IS NULL ORDER BY lower(name), id",
    )
    .fetch_all(&state.db)
    .await?;
    let mut kids: HashMap<Option<Uuid>, Vec<&PickRow>> = HashMap::new();
    for r in &rows {
        kids.entry(r.parent_id).or_default().push(r);
    }
    fn walk(parent: Option<Uuid>, prefix: &str, depth: usize, kids: &HashMap<Option<Uuid>, Vec<&PickRow>>, out: &mut Vec<Json>) {
        if depth > 64 {
            return;
        }
        for r in kids.get(&parent).map(Vec::as_slice).unwrap_or(&[]) {
            let path = if prefix.is_empty() { r.name.clone() } else { format!("{prefix} › {}", r.name) };
            let note = if r.status == "approved" { String::new() } else { format!(" · {}", match r.status.as_str() {
                "draft" => "draft", "ready" => "waiting for review", other => other }) };
            out.push(json!({ "id": r.id.to_string(), "label": format!("{path} ({}){note}", r.kind) }));
            walk(Some(r.id), &path, depth + 1, kids, out);
        }
    }
    let mut out = Vec::new();
    walk(None, "", 0, &kids, &mut out);
    Ok(out)
}

async fn stops_fragment(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    render_stops(&state, &author.0, &jar, id, None, None).await
}

#[derive(Debug, Deserialize)]
pub struct AddStopForm {
    csrf_token: String,
    #[serde(default)]
    artifact_id: String,
    #[serde(default)]
    note: String,
}

fn clean_note(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.chars().take(1000).collect())
}

async fn add_stop(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<AddStopForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    load_owned(&state, &author.0, id).await?;
    let Ok(artifact_id) = f.artifact_id.parse::<Uuid>() else {
        return Ok(render_stops(&state, &author.0, &jar, id, Some("Choose an artifact to add."), None).await?.into_response());
    };
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
        .bind(artifact_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Ok(render_stops(&state, &author.0, &jar, id, Some("That artifact no longer exists."), None).await?.into_response());
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM trail_stops WHERE trail_id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if count as usize >= MAX_STOPS {
        let msg = format!("A trail can have at most {MAX_STOPS} stops.");
        return Ok(render_stops(&state, &author.0, &jar, id, Some(&msg), None).await?.into_response());
    }
    sqlx::query(
        "INSERT INTO trail_stops (trail_id, artifact_id, position, note) \
         VALUES ($1, $2, (SELECT COALESCE(max(position) + 1, 0) FROM trail_stops WHERE trail_id = $1), $3)",
    )
    .bind(id)
    .bind(artifact_id)
    .bind(clean_note(&f.note))
    .execute(&state.db)
    .await?;
    touch(&state, id).await?;
    Ok(render_stops(&state, &author.0, &jar, id, None, None).await?.into_response())
}

/// The trail a stop belongs to, after checking the author may change it.
async fn stop_trail(state: &AppState, author: &Author, stop_id: Uuid) -> AppResult<Uuid> {
    let trail_id: Uuid = sqlx::query_scalar("SELECT trail_id FROM trail_stops WHERE id = $1")
        .bind(stop_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("stop"))?;
    load_owned(state, author, trail_id).await?;
    Ok(trail_id)
}

#[derive(Debug, Deserialize)]
pub struct StopNoteForm {
    csrf_token: String,
    #[serde(default)]
    note: String,
}

async fn update_stop(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<StopNoteForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    let trail_id = stop_trail(&state, &author.0, id).await?;
    sqlx::query("UPDATE trail_stops SET note = $2 WHERE id = $1")
        .bind(id)
        .bind(clean_note(&f.note))
        .execute(&state.db)
        .await?;
    touch(&state, trail_id).await?;
    render_stops(&state, &author.0, &jar, trail_id, None, None).await
}

async fn delete_stop(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<CsrfForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    let trail_id = stop_trail(&state, &author.0, id).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SET CONSTRAINTS trail_stops_trail_position_key DEFERRED").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM trail_stops WHERE id = $1").bind(id).execute(&mut *tx).await?;
    // Close the gap, so positions stay 0..n-1.
    sqlx::query(
        "UPDATE trail_stops s SET position = o.rn - 1 FROM \
         (SELECT id, row_number() OVER (ORDER BY position) AS rn FROM trail_stops WHERE trail_id = $1) o \
         WHERE s.id = o.id AND s.position <> o.rn - 1",
    )
    .bind(trail_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    touch(&state, trail_id).await?;
    render_stops(&state, &author.0, &jar, trail_id, None, None).await
}

#[derive(Debug, Deserialize)]
pub struct OrderForm {
    csrf_token: String,
    ids: String,
}

async fn reorder_stops(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<OrderForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    load_owned(&state, &author.0, id).await?;
    let ids: Vec<Uuid> = f
        .ids
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().parse::<Uuid>())
        .collect::<Result<_, _>>()
        .map_err(|_| AppError::BadRequest("bad stop order".into()))?;
    let current: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM trail_stops WHERE trail_id = $1 ORDER BY position")
        .bind(id)
        .fetch_all(&state.db)
        .await?;
    // The new order must be a rearrangement of exactly the stops there now,
    // so a stale page can't drop or duplicate one.
    let mut a = ids.clone();
    let mut b = current.clone();
    a.sort();
    b.sort();
    if a != b {
        return render_stops(&state, &author.0, &jar, id, Some("The trail changed while you were reordering it. Here is the current order; try again."), None).await;
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("SET CONSTRAINTS trail_stops_trail_position_key DEFERRED").execute(&mut *tx).await?;
    for (pos, sid) in ids.iter().enumerate() {
        sqlx::query("UPDATE trail_stops SET position = $2 WHERE id = $1 AND position <> $2")
            .bind(sid)
            .bind(pos as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    touch(&state, id).await?;
    render_stops(&state, &author.0, &jar, id, None, None).await
}
