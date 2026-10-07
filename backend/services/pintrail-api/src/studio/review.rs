//! Review status, change history, and recovering deleted artifacts.
//!
//! Status moves draft -> ready (the owner submits) -> approved (an admin
//! approves). An admin can send an artifact back to draft with a note, and
//! an owner can withdraw a submission. Every transition, like every other
//! change, lands in `artifact_history` through the database triggers; the
//! handlers here only make sure each one is attributed to whoever did it.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use minijinja::{context, Value};
use serde::Deserialize;
use serde_json::{json, Value as Json};
use std::collections::HashMap;
use uuid::Uuid;

use super::routes::{
    base_ctx, detail_after_change, environment, render, require_csrf, require_editor, respond,
    session_token, CsrfForm,
};
use crate::audit;
use crate::authors::extractors::AuthenticatedAuthor;
use crate::authors::model::Author;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio/artifacts/{id}/submit", post(submit))
        .route("/studio/artifacts/{id}/withdraw", post(withdraw))
        .route("/studio/artifacts/{id}/approve", post(approve))
        .route("/studio/artifacts/{id}/send-back", post(send_back))
        .route("/studio/artifacts/{id}/restore", post(restore))
        .route("/studio/artifacts/{id}/history", get(history))
        .route("/studio/review", get(review_queue))
        .route("/studio/review/count", get(review_count))
        .route("/studio/deleted", get(deleted))
}

fn require_admin(author: &Author) -> AppResult<()> {
    if audit::is_admin(author) { Ok(()) } else { Err(AppError::Forbidden) }
}

// --- status changes ------------------------------------------------------------

/// Owner (or admin) submits a draft for review.
async fn submit(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;
    audit::ensure_can_modify(&mut *tx, &author.0, id).await?;
    sqlx::query(
        "UPDATE artifacts SET status = 'ready', submitted_at = now() \
         WHERE id = $1 AND status = 'draft'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

/// Owner (or admin) takes a submission back to keep working on it.
async fn withdraw(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;
    audit::ensure_can_modify(&mut *tx, &author.0, id).await?;
    sqlx::query("UPDATE artifacts SET status = 'draft' WHERE id = $1 AND status = 'ready'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

async fn approve(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_admin(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;
    let n = sqlx::query(
        "UPDATE artifacts SET status = 'approved', reviewed_by = $2, reviewed_at = now(), \
         review_note = '' WHERE id = $1 AND deleted_at IS NULL AND status <> 'approved'",
    )
    .bind(id)
    .bind(author.0.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    tracing::info!(actor = %author.0.id, artifact_id = %id, changed = n, "artifact approved");
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

#[derive(Debug, Deserialize)]
pub struct SendBackForm {
    csrf_token: String,
    #[serde(default)]
    note: String,
}

/// Admin returns an artifact to its author as a draft, saying what to fix.
async fn send_back(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<SendBackForm>,
) -> AppResult<Response> {
    require_admin(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let note = form.note.trim();
    if note.is_empty() {
        return Err(AppError::BadRequest("say what needs to change".into()));
    }
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;
    sqlx::query(
        "UPDATE artifacts SET status = 'draft', review_note = $2, reviewed_by = $3, reviewed_at = now() \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(note)
    .bind(author.0.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

// --- review queue ----------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct QueueRow {
    id: Uuid,
    name: String,
    kind: String,
    status: String,
    owner_id: Option<Uuid>,
    submitted_at: Option<DateTime<Utc>>,
    reviewed_at: Option<DateTime<Utc>>,
    review_note: String,
    parent_name: Option<String>,
}

async fn review_queue(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    require_admin(&author.0)?;
    let rows = sqlx::query_as::<_, QueueRow>(
        r#"
        SELECT a.id, a.name, a.kind::text AS kind, a.status::text AS status,
               a.created_by AS owner_id,
               a.submitted_at, a.reviewed_at, a.review_note,
               (SELECT p.name FROM artifacts p WHERE p.id = a.parent_id) AS parent_name
        FROM artifacts a
        WHERE a.deleted_at IS NULL
        ORDER BY a.submitted_at NULLS LAST, a.name
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().filter_map(|r| r.owner_id).collect();
    let people = super::profile::load_people(&state.db, &ids).await?;

    let view = |r: &QueueRow| {
        json!({
            "id": r.id.to_string(), "name": r.name, "kind": r.kind,
            "owner": r.owner_id.and_then(|id| people.get(&id)),
            "submitted_at": r.submitted_at.map(|t| t.to_rfc3339()),
            "reviewed_at": r.reviewed_at.map(|t| t.to_rfc3339()),
            "review_note": r.review_note, "parent_name": r.parent_name,
        })
    };
    let waiting: Vec<Json> = rows.iter().filter(|r| r.status == "ready").map(view).collect();
    let sent_back: Vec<Json> = rows
        .iter()
        .filter(|r| r.status == "draft" && !r.review_note.is_empty())
        .map(view)
        .collect();
    let unowned: Vec<Json> = rows.iter().filter(|r| r.owner_id.is_none()).map(view).collect();
    let count = |s: &str| rows.iter().filter(|r| r.status == s).count();

    let ctx = context! {
        title => "Review",
        waiting => Value::from_serialize(&waiting),
        sent_back => Value::from_serialize(&sent_back),
        unowned => Value::from_serialize(&unowned),
        n_draft => count("draft"), n_ready => count("ready"), n_approved => count("approved"),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_review.html", ctx)
}

/// The number on the sidebar's Review button.
async fn review_count(author: AuthenticatedAuthor, State(state): State<AppState>) -> AppResult<Html<String>> {
    if !audit::is_admin(&author.0) {
        return Ok(Html(String::new()));
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM artifacts WHERE status = 'ready' AND deleted_at IS NULL",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Html(if n > 0 { format!("<span class=\"count\">{n}</span>") } else { String::new() }))
}

// --- recently deleted --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DeletedRow {
    id: Uuid,
    name: String,
    kind: String,
    deleted_at: DateTime<Utc>,
    deleted_by_id: Option<Uuid>,
    deleted_by_email: Option<String>,
    nested: i64,
    parent_name: Option<String>,
}

/// Each delete as one entry: the artifact that was deleted, not every
/// artifact that went with it (those come back when it's restored).
async fn deleted(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    require_admin(&author.0)?;
    let rows = sqlx::query_as::<_, DeletedRow>(
        r#"
        SELECT a.id, a.name, a.kind::text AS kind, a.deleted_at,
               d.actor_id AS deleted_by_id, d.actor_email AS deleted_by_email,
               (WITH RECURSIVE sub AS (
                    SELECT c.id FROM artifacts c WHERE c.parent_id = a.id AND c.deleted_at = a.deleted_at
                    UNION ALL
                    SELECT c.id FROM artifacts c JOIN sub s ON c.parent_id = s.id
                    WHERE c.deleted_at = a.deleted_at)
                SELECT count(*) FROM sub) AS nested,
               p.name AS parent_name
        FROM artifacts a
        LEFT JOIN artifacts p ON p.id = a.parent_id
        LEFT JOIN LATERAL (
            SELECT h.actor_id, h.actor_email FROM artifact_history h
            WHERE h.artifact_id = a.id AND h.action = 'deleted'
            ORDER BY h.id DESC LIMIT 1
        ) d ON true
        WHERE a.deleted_at IS NOT NULL
          AND (p.id IS NULL OR p.deleted_at IS DISTINCT FROM a.deleted_at)
        ORDER BY a.deleted_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().filter_map(|r| r.deleted_by_id).collect();
    let people = super::profile::load_people(&state.db, &ids).await?;
    let items: Vec<Json> = rows
        .iter()
        .map(|r| json!({
            "id": r.id.to_string(), "name": r.name, "kind": r.kind,
            "deleted_at": r.deleted_at.to_rfc3339(),
            "deleted_by": r.deleted_by_id.and_then(|id| people.get(&id)).cloned()
                .or_else(|| r.deleted_by_email.as_ref().map(|e| json!({ "name": e, "initials": "?", "hue": 0 }))),
            "nested": r.nested, "parent_name": r.parent_name,
        }))
        .collect();
    let ctx = context! {
        title => "Recently deleted",
        items => Value::from_serialize(&items),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_deleted.html", ctx)
}

/// Undoes a delete: the artifact and everything deleted along with it.
async fn restore(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_admin(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;

    let parent_deleted: Option<bool> = sqlx::query_scalar(
        "SELECT p.deleted_at IS NOT NULL FROM artifacts a LEFT JOIN artifacts p ON p.id = a.parent_id \
         WHERE a.id = $1 AND a.deleted_at IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    match parent_deleted {
        None => return Err(AppError::NotFound("deleted artifact")),
        Some(true) => {
            return Err(AppError::BadRequest(
                "the artifact this was inside is deleted too; restore that one first".into(),
            ))
        }
        Some(false) => {}
    }

    sqlx::query(
        r#"
        WITH RECURSIVE sub AS (
            SELECT id, deleted_at FROM artifacts WHERE id = $1
            UNION ALL
            SELECT a.id, a.deleted_at FROM artifacts a JOIN sub s ON a.parent_id = s.id
            WHERE a.deleted_at = s.deleted_at
        )
        UPDATE artifacts SET deleted_at = NULL WHERE id IN (SELECT id FROM sub)
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(actor = %author.0.id, artifact_id = %id, "artifact restored");
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

// --- history ---------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct HistoryRow {
    actor_id: Option<Uuid>,
    actor_email: Option<String>,
    action: String,
    changes: Json,
    at: DateTime<Utc>,
}

async fn history(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let rows = sqlx::query_as::<_, HistoryRow>(
        "SELECT actor_id, actor_email, action, changes, at FROM artifact_history \
         WHERE artifact_id = $1 ORDER BY at DESC, id DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Names for parent ids and people for owner ids that appear in the log,
    // including artifacts and authors that have since been deleted.
    let names: HashMap<String, String> = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM artifacts")
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|(id, n)| (id.to_string(), n))
        .collect();
    let emails: HashMap<String, String> = sqlx::query_as::<_, (Uuid, String)>("SELECT id, author_label(id) FROM authors")
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|(id, e)| (id.to_string(), e))
        .collect();
    let actor_ids: Vec<Uuid> = rows.iter().filter_map(|r| r.actor_id).collect();
    let people = super::profile::load_people(&state.db, &actor_ids).await?;
    let lookup = Lookup { names, emails, people };

    let entries: Vec<Json> = rows.iter().map(|r| describe(r, &lookup)).collect();
    render(
        &environment(),
        "studio_history.html",
        context! {
            entries => Value::from_serialize(&entries),
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

struct Lookup {
    names: HashMap<String, String>,
    /// Owner ids to names, for "Owner changed" entries.
    emails: HashMap<String, String>,
    people: HashMap<Uuid, Json>,
}

fn status_label(s: &str) -> &str {
    match s {
        "draft" => "Draft",
        "ready" => "Ready for review",
        "approved" => "Approved",
        other => other,
    }
}

/// Renders one stored value for people: coordinates, ids, and empty values.
fn show(field: &str, v: &Json, l: &Lookup) -> String {
    match (field, v) {
        (_, Json::Null) => match field {
            "parent_id" => "top level".into(),
            "lat" | "lng" => "parent's location".into(),
            "created_by" => "no owner".into(),
            _ => "(empty)".into(),
        },
        ("parent_id", Json::String(s)) => l.names.get(s).cloned().unwrap_or_else(|| "a deleted artifact".into()),
        ("created_by", Json::String(s)) => l.emails.get(s).cloned().unwrap_or_else(|| "a removed account".into()),
        ("status", Json::String(s)) => status_label(s).into(),
        (_, Json::String(s)) if s.is_empty() => "(empty)".into(),
        (_, Json::String(s)) => s.clone(),
        (_, other) => other.to_string(),
    }
}

fn field_label(f: &str) -> &str {
    match f {
        "kind" => "Kind",
        "name" => "Name",
        "description" => "Description",
        "lat" => "Latitude",
        "lng" => "Longitude",
        "parent_id" => "Inside",
        "beacon_id" => "Beacon",
        "status" => "Status",
        "review_note" => "Review note",
        "created_by" => "Owner",
        "url" => "Address",
        "note" => "Note",
        "caption" => "Caption",
        other => other,
    }
}

/// One history row as a title plus "field: from -> to" lines.
fn describe(r: &HistoryRow, l: &Lookup) -> Json {
    let c = &r.changes;
    let s = |k: &str| c.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let diff_lines = |skip: &[&str]| -> Vec<Json> {
        let mut out = Vec::new();
        if let Some(map) = c.as_object() {
            // Coordinates read better as one pair.
            if let (Some(a), Some(b)) = (map.get("lat"), map.get("lng")) {
                let pair = |side: &str| match (a.get(side), b.get(side)) {
                    (Some(Json::Null), _) | (None, _) => "parent's location".to_string(),
                    (Some(x), Some(y)) => format!("{x}, {y}"),
                    (Some(x), None) => x.to_string(),
                };
                out.push(json!({ "label": "Location", "from": pair("from"), "to": pair("to"), "long": false }));
            }
            let order = ["name", "kind", "parent_id", "description", "lat", "lng", "beacon_id",
                         "status", "review_note", "created_by", "url", "note", "caption"];
            for f in order {
                if skip.contains(&f) || ((f == "lat" || f == "lng") && map.contains_key("lat") && map.contains_key("lng")) {
                    continue;
                }
                if let Some(v) = map.get(f).filter(|v| v.get("from").is_some() || v.get("to").is_some()) {
                    let from = show(f, v.get("from").unwrap_or(&Json::Null), l);
                    let to = show(f, v.get("to").unwrap_or(&Json::Null), l);
                    let long = f == "description" || f == "review_note" || from.len() > 80 || to.len() > 80;
                    out.push(json!({ "label": field_label(f), "from": from, "to": to, "long": long }));
                }
            }
        }
        out
    };

    let status_to = c.pointer("/status/to").and_then(Json::as_str).unwrap_or("");
    let status_from = c.pointer("/status/from").and_then(Json::as_str).unwrap_or("");
    let note_to = c.pointer("/review_note/to").and_then(Json::as_str).unwrap_or("");

    let (title, lines): (String, Vec<Json>) = match r.action.as_str() {
        "imported" => ("History starts here".into(), vec![json!({ "text": s("note") })]),
        "created" => {
            let mut lines = vec![];
            for f in ["name", "kind", "parent_id", "status"] {
                if let Some(v) = c.get(f) {
                    lines.push(json!({ "label": field_label(f), "value": show(f, v, l) }));
                }
            }
            match (c.get("lat"), c.get("lng")) {
                (Some(a), Some(b)) => lines.push(json!({ "label": "Location", "value": format!("{a}, {b}") })),
                _ => lines.push(json!({ "label": "Location", "value": "parent's location" })),
            }
            if let Some(d) = c.get("description").and_then(Json::as_str) {
                lines.push(json!({ "label": "Description", "value": d, "long": true }));
            }
            ("Created".into(), lines)
        }
        "edited" => ("Edited".into(), diff_lines(&[])),
        "owner" => ("Owner changed".into(), diff_lines(&[])),
        "status" => {
            let title = match (status_from, status_to) {
                (_, "ready") if status_from == "approved" => "Back in review after changes",
                (_, "ready") => "Submitted for review",
                (_, "approved") => "Approved",
                ("ready", "draft") | ("approved", "draft") if !note_to.is_empty() => "Sent back for changes",
                ("ready", "draft") => "Withdrawn from review",
                _ => "Status changed",
            };
            let mut lines = diff_lines(&["status", "review_note"]);
            if !note_to.is_empty() {
                lines.insert(0, json!({ "label": "Note", "value": note_to, "long": true }));
            }
            (title.into(), lines)
        }
        "deleted" => ("Deleted".into(), vec![]),
        "restored" => ("Restored".into(), vec![]),
        "tag_added" => (format!("Tag added: {}", s("tag")), vec![]),
        "tag_removed" => (format!("Tag removed: {}", s("tag")), vec![]),
        "link_added" => ("Link added".into(), {
            let mut v = vec![json!({ "label": "Address", "value": s("url") })];
            if !s("note").is_empty() { v.push(json!({ "label": "Note", "value": s("note") })); }
            v
        }),
        "link_removed" => ("Link removed".into(), vec![json!({ "label": "Address", "value": s("url") })]),
        "link_edited" => ("Link edited".into(), {
            let mut v = diff_lines(&[]);
            if c.get("url").is_none() { v.insert(0, json!({ "label": "Link", "value": s("link") })); }
            v
        }),
        "links_reordered" => ("Links reordered".into(), vec![]),
        "media_added" => (format!("Photo or file added: {}", s("file")), vec![]),
        "media_removed" => (format!("Photo or file removed: {}", s("file")), vec![]),
        "media_edited" => (format!("Caption changed: {}", s("file")), diff_lines(&[])),
        other => (other.replace('_', " "), diff_lines(&[])),
    };

    json!({
        "at": r.at.to_rfc3339(),
        // The person as they are now; for a removed account, the email
        // recorded at the time.
        "who": r.actor_id.and_then(|id| l.people.get(&id)).cloned()
            .or_else(|| r.actor_email.as_ref().map(|e| json!({ "name": e, "initials": "?", "hue": 0 }))),
        "system": r.actor_id.is_none() && r.actor_email.is_none(),
        "title": title,
        "lines": lines,
    })
}

