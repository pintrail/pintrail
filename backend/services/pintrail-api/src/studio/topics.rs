//! Topics in the Studio (issue #49): a shared page many artifacts link to.
//!
//! A topic is an artifact row with `is_topic = true` (migration
//! `..._artifact_topics`), so creating, editing, reviewing, links, media,
//! and history all go through the ordinary artifact pages. This module adds
//! what is specific to topics:
//!
//! * the Topics list;
//! * the "Topics" card on an artifact's page, which links the artifact to
//!   topics and shows each topic's shared description;
//! * the "Linked artifacts" card on a topic's page, with every linked
//!   artifact on one map and a button to add a new artifact already linked.
//!
//! A link is part of the *artifact's* content (it changes what the artifact's
//! page says), so changing one needs the right to change that artifact, the
//! same rule as its web links and media. The topic's owner isn't asked:
//! linking a building to "LEED certification" no more edits the LEED page
//! than tagging it "solar" edits a tag.

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

use super::routes::{base_ctx, environment, render, require_csrf, require_editor, respond, session_token};
use crate::artifacts::routes::RESOLVED_COORDS_CTE;
use crate::audit;
use crate::authors::extractors::AuthenticatedAuthor;
use crate::authors::model::Author;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Longest note on a link, matching the column.
const NOTE_MAX: usize = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio/topics", get(list))
        .route("/studio/topics/new", get(new_form))
        .route("/studio/topics/{id}/artifacts", get(topic_card).post(topic_link))
        .route("/studio/topics/{id}/artifacts/{artifact_id}/delete", post(topic_unlink))
        .route("/studio/artifacts/{id}/topics", get(artifact_card).post(artifact_link))
        .route("/studio/artifacts/{id}/topics/{topic_id}", post(artifact_note))
        .route("/studio/artifacts/{id}/topics/{topic_id}/delete", post(artifact_unlink))
}

// --- the list ----------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ListRow {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    created_by: Option<Uuid>,
    updated_at: DateTime<Utc>,
    linked: i64,
}

async fn list(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    let rows = sqlx::query_as::<_, ListRow>(
        "SELECT t.id, t.name, t.description, t.status::text AS status, t.created_by, t.updated_at, \
                (SELECT count(*) FROM artifact_topics l JOIN artifacts a ON a.id = l.artifact_id \
                 WHERE l.topic_id = t.id AND a.deleted_at IS NULL) AS linked \
         FROM artifacts t WHERE t.is_topic AND t.deleted_at IS NULL ORDER BY lower(t.name), t.id",
    )
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().filter_map(|r| r.created_by).collect();
    let people = super::profile::load_people(&state.db, &ids).await?;
    let topics: Vec<Json> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(), "name": r.name, "status": r.status, "linked": r.linked,
                "description": excerpt(&r.description, 220),
                "owner": r.created_by.and_then(|id| people.get(&id)),
                "updated_at": r.updated_at.to_rfc3339(),
            })
        })
        .collect();
    let ctx = context! {
        title => "Topics",
        topics => Value::from_serialize(&topics),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_topics.html", ctx)
}

/// The first `max` characters of a description, cut at a word, for a list.
fn excerpt(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map(|(head, _)| head).unwrap_or(&cut);
    format!("{}…", cut.trim_end_matches(|c: char| c.is_ascii_punctuation()))
}

async fn new_form(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    let form = context! {
        name => "", kind => "other", description => "", tags => "",
        parent_id => "", lat => "", lng => "", is_topic => true,
    };
    super::routes::render_form(&state, &author.0, &jar, &headers, None, form, None).await
}

// --- linking -----------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LinkForm {
    csrf_token: String,
    /// The other end: a topic on an artifact's card, an artifact on a topic's.
    #[serde(default)]
    other_id: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
pub struct NoteForm {
    csrf_token: String,
    #[serde(default)]
    note: String,
}

fn clean_note(note: &str) -> Result<String, String> {
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    if note.chars().count() > NOTE_MAX {
        return Err(format!("Keep the note under {NOTE_MAX} characters."));
    }
    Ok(note)
}

/// What kind of row `id` is: Some(true) a topic, Some(false) an artifact,
/// None if it doesn't exist (or is deleted).
async fn is_topic(state: &AppState, id: Uuid) -> AppResult<Option<bool>> {
    Ok(sqlx::query_scalar("SELECT is_topic FROM artifacts WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(&state.db)
        .await?)
}

/// Links `artifact` to `topic`, or says why not in words for the card.
/// Linking twice is not an error: the note is updated instead.
async fn link(state: &AppState, author: &Author, artifact: Uuid, topic: Uuid, note: &str) -> AppResult<Option<String>> {
    require_editor(author)?;
    let note = match clean_note(note) {
        Ok(n) => n,
        Err(msg) => return Ok(Some(msg)),
    };
    match (is_topic(state, artifact).await?, is_topic(state, topic).await?) {
        (Some(false), Some(true)) => {}
        (None, _) | (_, None) => return Ok(Some("That no longer exists. Reload the page and try again.".into())),
        _ => return Ok(Some("Choose an artifact and a topic.".into())),
    }
    let own = audit::ownership(&state.db, artifact).await?;
    if !audit::may_modify(author, &own) {
        return Ok(Some("Only the artifact's owner (or an admin) can link it to a topic.".into()));
    }
    let mut tx = audit::begin_as(&state.db, author.id).await?;
    sqlx::query(
        "INSERT INTO artifact_topics (artifact_id, topic_id, note) VALUES ($1, $2, $3) \
         ON CONFLICT (artifact_id, topic_id) DO UPDATE SET note = EXCLUDED.note",
    )
    .bind(artifact)
    .bind(topic)
    .bind(&note)
    .execute(&mut *tx)
    .await?;
    audit::touch(&mut tx, author, artifact).await?;
    tx.commit().await?;
    tracing::info!(actor = %author.id, artifact_id = %artifact, topic_id = %topic, "artifact linked to topic");
    Ok(None)
}

async fn unlink(state: &AppState, author: &Author, artifact: Uuid, topic: Uuid) -> AppResult<()> {
    require_editor(author)?;
    let mut tx = audit::begin_as(&state.db, author.id).await?;
    audit::ensure_can_modify(&mut *tx, author, artifact).await?;
    let removed = sqlx::query("DELETE FROM artifact_topics WHERE artifact_id = $1 AND topic_id = $2")
        .bind(artifact)
        .bind(topic)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed > 0 {
        audit::touch(&mut tx, author, artifact).await?;
    }
    tx.commit().await?;
    Ok(())
}

// --- the Topics card on an artifact's page -------------------------------------

#[derive(sqlx::FromRow)]
struct LinkedTopic {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    note: String,
}

async fn render_artifact_card(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    artifact: Uuid,
    error: Option<&str>,
) -> AppResult<Html<String>> {
    let own = audit::ownership(&state.db, artifact).await?;
    let can_edit = audit::may_modify(author, &own);
    let rows = sqlx::query_as::<_, LinkedTopic>(
        "SELECT t.id, t.name, t.description, t.status::text AS status, l.note \
         FROM artifact_topics l JOIN artifacts t ON t.id = l.topic_id \
         WHERE l.artifact_id = $1 AND t.deleted_at IS NULL ORDER BY lower(t.name), t.id",
    )
    .bind(artifact)
    .fetch_all(&state.db)
    .await?;
    let linked: Vec<Json> = rows
        .iter()
        .map(|r| json!({ "id": r.id.to_string(), "name": r.name, "description": r.description, "status": r.status, "note": r.note }))
        .collect();
    let choices: Vec<(Uuid, String)> = if can_edit {
        sqlx::query_as(
            "SELECT id, name FROM artifacts t WHERE is_topic AND deleted_at IS NULL \
             AND NOT EXISTS (SELECT 1 FROM artifact_topics l WHERE l.topic_id = t.id AND l.artifact_id = $1) \
             ORDER BY lower(name), id",
        )
        .bind(artifact)
        .fetch_all(&state.db)
        .await?
    } else {
        Vec::new()
    };
    let choices: Vec<Json> = choices.iter().map(|(id, name)| json!({ "id": id.to_string(), "name": name })).collect();
    render(
        &environment(),
        "studio_artifact_topics.html",
        context! {
            artifact_id => artifact.to_string(),
            linked => Value::from_serialize(&linked),
            choices => Value::from_serialize(&choices),
            can_edit => can_edit,
            error => error,
            ..base_ctx(author, &session_token(jar))
        },
    )
}

async fn artifact_card(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    render_artifact_card(&state, &author.0, &jar, id, None).await
}

async fn artifact_link(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<LinkForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    let error = match f.other_id.trim().parse::<Uuid>() {
        Ok(topic) => link(&state, &author.0, id, topic, &f.note).await?,
        Err(_) => Some("Choose a topic to link.".to_string()),
    };
    render_artifact_card(&state, &author.0, &jar, id, error.as_deref()).await
}

async fn artifact_note(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path((id, topic)): Path<(Uuid, Uuid)>,
    Form(f): Form<NoteForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    require_editor(&author.0)?;
    let note = match clean_note(&f.note) {
        Ok(n) => n,
        Err(msg) => return render_artifact_card(&state, &author.0, &jar, id, Some(&msg)).await,
    };
    let mut tx = audit::begin_as(&state.db, author.0.id).await?;
    audit::ensure_can_modify(&mut *tx, &author.0, id).await?;
    let changed = sqlx::query(
        "UPDATE artifact_topics SET note = $3 WHERE artifact_id = $1 AND topic_id = $2 AND note <> $3",
    )
    .bind(id)
    .bind(topic)
    .bind(&note)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed > 0 {
        audit::touch(&mut tx, &author.0, id).await?;
    }
    tx.commit().await?;
    render_artifact_card(&state, &author.0, &jar, id, None).await
}

async fn artifact_unlink(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path((id, topic)): Path<(Uuid, Uuid)>,
    Form(f): Form<super::routes::CsrfForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    unlink(&state, &author.0, id, topic).await?;
    render_artifact_card(&state, &author.0, &jar, id, None).await
}

// --- the Linked artifacts card on a topic's page --------------------------------

#[derive(sqlx::FromRow)]
struct LinkedArtifact {
    id: Uuid,
    name: String,
    kind: String,
    status: String,
    note: String,
    created_by: Option<Uuid>,
    parent_name: Option<String>,
    lat: Option<f64>,
    lng: Option<f64>,
}

async fn render_topic_card(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    topic: Uuid,
    error: Option<&str>,
) -> AppResult<Html<String>> {
    if is_topic(state, topic).await? != Some(true) {
        return Err(AppError::NotFound("topic"));
    }
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.name, a.kind::text AS kind, a.status::text AS status, l.note, a.created_by,
               (SELECT p.name FROM artifacts p WHERE p.id = a.parent_id) AS parent_name,
               r.effective_lat AS lat, r.effective_lng AS lng
        FROM artifact_topics l
        JOIN artifacts a ON a.id = l.artifact_id
        JOIN resolved r ON r.id = a.id
        WHERE l.topic_id = $1 AND a.deleted_at IS NULL
        ORDER BY lower(a.name), a.id
        "#
    );
    let rows = sqlx::query_as::<_, LinkedArtifact>(&sql).bind(topic).fetch_all(&state.db).await?;
    let linked: Vec<Json> = rows
        .iter()
        .map(|r| {
            let own = audit::Ownership { created_by: r.created_by };
            json!({
                "id": r.id.to_string(), "name": r.name, "kind": r.kind, "status": r.status,
                "note": r.note, "parent_name": r.parent_name, "placed": r.lat.is_some(),
                "can_edit": audit::may_modify(author, &own),
            })
        })
        .collect();
    let points: Vec<Json> = rows
        .iter()
        .filter_map(|r| Some(json!({ "name": r.name, "lat": r.lat?, "lng": r.lng?, "artifact_id": r.id.to_string() })))
        .collect();

    // The artifacts this author could link from here: ones they may change,
    // not linked already. Labelled with what they sit inside, since two
    // rooms can share a name.
    let choices: Vec<(Uuid, String, Option<String>)> = if require_editor(author).is_ok() {
        sqlx::query_as(
            "SELECT a.id, a.name, p.name FROM artifacts a LEFT JOIN artifacts p ON p.id = a.parent_id \
             WHERE NOT a.is_topic AND a.deleted_at IS NULL AND ($2 OR a.created_by = $3) \
             AND NOT EXISTS (SELECT 1 FROM artifact_topics l WHERE l.artifact_id = a.id AND l.topic_id = $1) \
             ORDER BY lower(a.name), a.id",
        )
        .bind(topic)
        .bind(audit::is_admin(author))
        .bind(author.id)
        .fetch_all(&state.db)
        .await?
    } else {
        Vec::new()
    };
    let choices: Vec<Json> = choices
        .iter()
        .map(|(id, name, parent)| {
            let label = match parent {
                Some(p) => format!("{name} (in {p})"),
                None => name.clone(),
            };
            json!({ "id": id.to_string(), "label": label })
        })
        .collect();

    render(
        &environment(),
        "studio_topic_artifacts.html",
        context! {
            topic_id => topic.to_string(),
            linked => Value::from_serialize(&linked),
            map_data => super::routes::script_json(&json!({ "points": points, "route": false })),
            placed => points.len(),
            choices => Value::from_serialize(&choices),
            error => error,
            ..base_ctx(author, &session_token(jar))
        },
    )
}

async fn topic_card(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    render_topic_card(&state, &author.0, &jar, id, None).await
}

async fn topic_link(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<LinkForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    let error = match f.other_id.trim().parse::<Uuid>() {
        Ok(artifact) => link(&state, &author.0, artifact, id, &f.note).await?,
        Err(_) => Some("Choose an artifact to link.".to_string()),
    };
    Ok(render_topic_card(&state, &author.0, &jar, id, error.as_deref()).await?.into_response())
}

async fn topic_unlink(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path((id, artifact)): Path<(Uuid, Uuid)>,
    Form(f): Form<super::routes::CsrfForm>,
) -> AppResult<Html<String>> {
    require_csrf(&jar, &f.csrf_token)?;
    unlink(&state, &author.0, artifact, id).await?;
    render_topic_card(&state, &author.0, &jar, id, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpt_cuts_at_a_word() {
        assert_eq!(excerpt("short text", 50), "short text");
        assert_eq!(excerpt("one two three four", 9), "one two…");
        assert_eq!(excerpt("a,\n\nb", 50), "a, b");
    }

    #[test]
    fn notes_are_trimmed_and_bounded() {
        assert_eq!(clean_note("  Gold,   2019 ").unwrap(), "Gold, 2019");
        assert!(clean_note(&"x".repeat(NOTE_MAX + 1)).is_err());
    }
}
