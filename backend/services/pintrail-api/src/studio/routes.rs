use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use minijinja::{context, Environment, Value};
use serde::Deserialize;
use uuid::Uuid;

use crate::admin::csrf;
use crate::artifacts::routes::RESOLVED_COORDS_CTE;
use crate::authors::auth::{generate_session_token, hash_session_token, SESSION_COOKIE};
use crate::authors::extractors::AuthenticatedAuthor;
use crate::authors::model::{Author, AuthorRole};
use crate::client_ip::ClientIp;
use crate::crypto::{verify_dummy_password, verify_password};
use crate::error::{AppError, AppResult};
use crate::rate_limit::{LOGIN_PER_ACCOUNT, LOGIN_PER_IP};
use crate::state::AppState;

const KINDS: [&str; 6] = ["building", "room", "artwork", "installation", "rooftop", "other"];

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    // minijinja autoescapes by .html extension, so every registered name ends
    // in .html -- artifact names and descriptions are author-controlled but
    // still rendered to a browser, and escaping is the default here.
    for (name, src) in [
        ("studio_base.html", include_str!("templates/base.html") as &str),
        ("studio_login.html", include_str!("templates/login.html")),
        ("studio_page.html", include_str!("templates/page.html")),
        ("studio_tree.html", include_str!("templates/tree.html")),
        ("studio_welcome.html", include_str!("templates/welcome.html")),
        ("studio_detail.html", include_str!("templates/detail.html")),
        ("studio_form.html", include_str!("templates/form.html")),
        ("studio_attachments.html", include_str!("templates/attachments.html")),
    ] {
        env.add_template(name, src).expect("studio template compiles");
    }
    env
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio", get(home))
        .route("/studio/login", get(login_form).post(login_submit))
        .route("/studio/logout", post(logout))
        .route("/studio/welcome", get(welcome))
        .route("/studio/tree", get(tree))
        .route("/studio/artifacts/new", get(new_form))
        .route("/studio/artifacts", post(create))
        .route("/studio/artifacts/{id}", get(detail).post(update))
        .route("/studio/artifacts/{id}/edit", get(edit_form))
        .route("/studio/artifacts/{id}/delete", post(remove))
        .route("/studio/artifacts/{id}/attachments", get(attachments))
        .route("/studio/attachments/{id}/delete", post(delete_attachment))
}

// --- rendering helpers -----------------------------------------------------

fn render(env: &Environment, name: &str, ctx: Value) -> AppResult<Html<String>> {
    let tmpl = env
        .get_template(name)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("template {name}: {e}")))?;
    Ok(Html(tmpl.render(ctx).map_err(|e| {
        AppError::Internal(anyhow::anyhow!("render {name}: {e}"))
    })?))
}

/// Returns a bare fragment to htmx (which sends `HX-Request`), or the whole
/// document to a direct navigation or refresh. Both share one fragment
/// template, so a deep-linked URL renders identically to a swapped-in view.
fn respond(
    env: &Environment,
    headers: &HeaderMap,
    fragment: &str,
    ctx: Value,
) -> AppResult<Html<String>> {
    if headers.contains_key("hx-request") {
        render(env, fragment, ctx)
    } else {
        render(env, "studio_page.html", context! { fragment => fragment, ..ctx })
    }
}

fn is_editor(author: &Author) -> bool {
    author.role >= AuthorRole::Editor
}

/// Context every authenticated view needs: who is signed in, whether they may
/// edit, and the CSRF token bound to their session.
fn base_ctx(author: &Author, session_token: &str) -> Value {
    context! {
        author => context! { email => author.email.clone(), role => author.role.as_str() },
        can_edit => is_editor(author),
        csrf_token => csrf::token_for_session(session_token),
    }
}

fn session_token(jar: &CookieJar) -> String {
    jar.get(SESSION_COOKIE).map(|c| c.value().to_string()).unwrap_or_default()
}

fn require_editor(author: &Author) -> AppResult<()> {
    if is_editor(author) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn require_csrf(jar: &CookieJar, submitted: &str) -> AppResult<()> {
    if csrf::verify(&session_token(jar), submitted) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

// --- auth ------------------------------------------------------------------

async fn login_form() -> AppResult<Html<String>> {
    render(&environment(), "studio_login.html", context! { title => "Sign in" })
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

/// Signs in any active author. Unlike the admin panel this is not admin-only:
/// editors are the content authors (DESIGN.md §1.2), and viewers get read-only
/// access. Write actions are gated per-handler by `require_editor`.
async fn login_submit(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let email = form.email.trim();
    let account_key = format!("author-login:{}", email.to_ascii_lowercase());
    let ip_key = format!("author-login-ip:{ip}");
    state.limiter.check(&ip_key, LOGIN_PER_IP)?;
    state.limiter.check(&account_key, LOGIN_PER_ACCOUNT)?;

    let author = sqlx::query_as::<_, Author>(
        "SELECT id, email, password_hash, role, is_active, created_at, updated_at \
         FROM authors WHERE lower(email) = lower($1)",
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    let invalid = || -> AppResult<Response> {
        Ok(render(
            &environment(),
            "studio_login.html",
            context! { title => "Sign in", error => "Invalid email or password." },
        )?
        .into_response())
    };

    // One opaque message and a dummy verification on a missing account, so
    // neither the response nor its timing tells an attacker which emails exist.
    let Some(author) = author else {
        verify_dummy_password(&form.password);
        return invalid();
    };
    if !verify_password(&form.password, &author.password_hash) || !author.is_active {
        return invalid();
    }

    let token = generate_session_token();
    sqlx::query("INSERT INTO author_sessions (author_id, token_hash, expires_at) VALUES ($1,$2,$3)")
        .bind(author.id)
        .bind(&token.hash)
        .bind(token.expires_at)
        .execute(&state.db)
        .await?;

    state.limiter.reset(&account_key);
    state.limiter.reset(&ip_key);
    tracing::info!(author_id = %author.id, "author signed in to studio");

    let jar = jar.add(crate::authors::routes::session_cookie(token.raw, &state, token.expires_at));
    Ok((jar, Redirect::to("/studio")).into_response())
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &form.csrf_token)?;
    sqlx::query("DELETE FROM author_sessions WHERE token_hash = $1")
        .bind(hash_session_token(&session_token(&jar)))
        .execute(&state.db)
        .await?;
    let mut removal =
        crate::authors::routes::session_cookie(String::new(), &state, Utc::now() - chrono::Duration::days(1));
    removal.make_removal();
    Ok((jar.add(removal), Redirect::to("/studio/login")).into_response())
}

#[derive(Debug, Deserialize)]
pub struct CsrfForm {
    csrf_token: String,
}

// --- views -----------------------------------------------------------------

async fn home(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    // Always the full document -- this is the entry point, not an htmx swap.
    let ctx = context! {
        title => "Studio",
        fragment => "studio_welcome.html",
        selected_id => Value::from(()),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    let _ = &state;
    render(&environment(), "studio_page.html", ctx)
}

async fn welcome(
    author: AuthenticatedAuthor,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    render(
        &environment(),
        "studio_welcome.html",
        base_ctx(&author.0, &session_token(&jar)),
    )
}

async fn tree(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<TreeQuery>,
) -> AppResult<Html<String>> {
    let roots = load_tree(&state).await?;
    render(
        &environment(),
        "studio_tree.html",
        context! {
            roots => roots,
            selected_id => q.selected.map(|u| u.to_string()),
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    selected: Option<Uuid>,
}

async fn detail(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let d = load_detail(&state, id).await?;
    let ctx = context! {
        title => d.row.name.clone(),
        a => d.value(),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_detail.html", ctx)
}

async fn attachments(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let views = crate::attachments::list_for_artifact(&state, id, true).await?;
    // Still-processing rows drive the fragment to poll again.
    let processing = views.iter().any(|m| {
        matches!(m.status,
            crate::attachments::model::ProcessingStatus::Queued
            | crate::attachments::model::ProcessingStatus::Processing
            | crate::attachments::model::ProcessingStatus::PendingUpload)
    });
    render(
        &environment(),
        "studio_attachments.html",
        context! {
            artifact_id => id.to_string(),
            attachments => serde_json::to_value(&views).map_err(|e| AppError::Internal(e.into()))?,
            processing => processing,
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

// --- forms -----------------------------------------------------------------

async fn new_form(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Query(q): Query<NewQuery>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    let parents = load_parent_options(&state, None).await?;
    let form = context! {
        name => "", kind => "building", description => "",
        parent_id => q.parent.map(|u| u.to_string()), lat => "", lng => "",
    };
    let ctx = context! {
        title => "New artifact",
        heading => "New artifact",
        action => "/studio/artifacts",
        submit_label => "Create",
        cancel_id => Value::from(()),
        kinds => KINDS,
        parents => parents,
        form => form,
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_form.html", ctx)
}

#[derive(Debug, Deserialize)]
pub struct NewQuery {
    parent: Option<Uuid>,
}

async fn edit_form(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    let d = load_detail(&state, id).await?;
    let parents = load_parent_options(&state, Some(id)).await?;

    // Empty strings, not nulls, so the number inputs render blank rather than
    // the literal "none" and so a blank submission means "inherit".
    let form = context! {
        name => d.row.name.clone(),
        kind => d.row.kind.clone(),
        description => d.row.description.clone(),
        parent_id => d.row.parent_id.map(|u| u.to_string()).unwrap_or_default(),
        lat => d.row.lat.map(|v| v.to_string()).unwrap_or_default(),
        lng => d.row.lng.map(|v| v.to_string()).unwrap_or_default(),
    };
    let ctx = context! {
        title => "Edit",
        heading => "Edit artifact",
        action => format!("/studio/artifacts/{id}"),
        submit_label => "Save changes",
        cancel_id => id.to_string(),
        kinds => KINDS,
        parents => parents,
        form => form,
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_form.html", ctx)
}

#[derive(Debug, Deserialize)]
pub struct ArtifactForm {
    csrf_token: String,
    name: String,
    kind: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parent_id: String,
    #[serde(default)]
    lat: String,
    #[serde(default)]
    lng: String,
}

impl ArtifactForm {
    fn coords(&self) -> AppResult<(Option<f64>, Option<f64>)> {
        let parse = |s: &str, what: &str| -> AppResult<Option<f64>> {
            let s = s.trim();
            if s.is_empty() {
                return Ok(None);
            }
            s.parse::<f64>()
                .map(Some)
                .map_err(|_| AppError::BadRequest(format!("{what} must be a number")))
        };
        Ok((parse(&self.lat, "latitude")?, parse(&self.lng, "longitude")?))
    }

    fn parent(&self) -> AppResult<Option<Uuid>> {
        let s = self.parent_id.trim();
        if s.is_empty() {
            return Ok(None);
        }
        Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| AppError::BadRequest("invalid parent".into()))
    }
}

async fn create(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Form(form): Form<ArtifactForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;

    let (lat, lng) = form.coords()?;
    crate::artifacts::routes::validate_coords(lat, lng)?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO artifacts (kind, name, description, lat, lng, parent_id) \
         VALUES ($1::artifact_kind, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(form.kind.trim())
    .bind(form.name.trim())
    .bind(form.description.trim())
    .bind(lat)
    .bind(lng)
    .bind(form.parent()?)
    .fetch_one(&state.db)
    .await
    .map_err(map_db_error)?;

    tracing::info!(actor = %author.0.id, artifact_id = %id, "artifact created in studio");
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

async fn update(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ArtifactForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;

    let (lat, lng) = form.coords()?;
    crate::artifacts::routes::validate_coords(lat, lng)?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let parent = form.parent()?;
    if parent == Some(id) {
        return Err(AppError::BadRequest("an artifact cannot be its own parent".into()));
    }

    let affected = sqlx::query(
        "UPDATE artifacts SET kind = $2::artifact_kind, name = $3, description = $4, \
         lat = $5, lng = $6, parent_id = $7 WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(form.kind.trim())
    .bind(form.name.trim())
    .bind(form.description.trim())
    .bind(lat)
    .bind(lng)
    .bind(parent)
    .execute(&state.db)
    .await
    .map_err(map_db_error)?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("artifact"));
    }

    tracing::info!(actor = %author.0.id, artifact_id = %id, "artifact updated in studio");
    detail_after_change(&state, &author.0, &jar, &headers, id).await
}

async fn remove(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;

    // Soft delete of the whole subtree, matching the JSON API: a surviving
    // child would inherit coordinates from a deleted ancestor.
    sqlx::query(
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
    .execute(&state.db)
    .await?;

    tracing::info!(actor = %author.0.id, artifact_id = %id, "artifact deleted in studio");

    let body = render(
        &environment(),
        "studio_welcome.html",
        base_ctx(&author.0, &session_token(&jar)),
    )?;
    Ok(([("HX-Trigger", "refresh-tree")], body).into_response())
}

async fn delete_attachment(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;

    // Delete the row and both objects, and learn which artifact to re-render.
    let artifact_id: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM attachments WHERE id = $1 \
         RETURNING artifact_id",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    let artifact_id = artifact_id.ok_or(AppError::NotFound("attachment"))?;
    // Object cleanup is best-effort; a leaked object is wasted space, a
    // dangling row would be a broken image. (The JSON delete handler removes
    // objects too; here we keep it simple and let the row drive the UI.)

    let views = crate::attachments::list_for_artifact(&state, artifact_id, true).await?;
    let processing = views.iter().any(|m| {
        matches!(m.status,
            crate::attachments::model::ProcessingStatus::Queued
            | crate::attachments::model::ProcessingStatus::Processing)
    });
    render(
        &environment(),
        "studio_attachments.html",
        context! {
            artifact_id => artifact_id.to_string(),
            attachments => serde_json::to_value(&views).map_err(|e| AppError::Internal(e.into()))?,
            processing => processing,
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

/// After a create or update, render the detail fragment and tell the sidebar
/// tree to refresh via an HX-Trigger response header.
async fn detail_after_change(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    headers: &HeaderMap,
    id: Uuid,
) -> AppResult<Response> {
    let d = load_detail(state, id).await?;
    let ctx = context! {
        title => d.row.name.clone(),
        a => d.value(),
        ..base_ctx(author, &session_token(jar))
    };
    let body = respond(&environment(), headers, "studio_detail.html", ctx)?;
    Ok(([("HX-Trigger", "refresh-tree")], body).into_response())
}

// --- data loading ----------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TreeRow {
    id: Uuid,
    kind: String,
    name: String,
    parent_id: Option<Uuid>,
}

/// Loads every artifact and nests it into a root->children forest for the
/// sidebar. One query, assembled in memory, rather than a query per level.
async fn load_tree(state: &AppState) -> AppResult<Value> {
    let rows = sqlx::query_as::<_, TreeRow>(
        "SELECT id, kind::text AS kind, name, parent_id \
         FROM artifacts WHERE deleted_at IS NULL ORDER BY name, id",
    )
    .fetch_all(&state.db)
    .await?;

    // Build child lists keyed by parent.
    use std::collections::HashMap;
    let mut children: HashMap<Option<Uuid>, Vec<&TreeRow>> = HashMap::new();
    for row in &rows {
        children.entry(row.parent_id).or_default().push(row);
    }

    fn build(id: Option<Uuid>, children: &std::collections::HashMap<Option<Uuid>, Vec<&TreeRow>>) -> Vec<serde_json::Value> {
        children
            .get(&id)
            .map(|kids| {
                kids.iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.id.to_string(),
                            "kind": r.kind,
                            "name": r.name,
                            "children": build(Some(r.id), children),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    let forest = build(None, &children);
    Ok(Value::from_serialize(&forest))
}

#[derive(sqlx::FromRow)]
struct DetailRow {
    id: Uuid,
    kind: String,
    name: String,
    description: String,
    lat: Option<f64>,
    lng: Option<f64>,
    effective_lat: Option<f64>,
    effective_lng: Option<f64>,
    parent_id: Option<Uuid>,
    parent_name: Option<String>,
    source_name: Option<String>,
}

/// A loaded artifact plus its direct children. Keeps the typed row around so
/// handlers can populate the edit form without round-tripping through a
/// `minijinja::Value`.
struct Detail {
    row: DetailRow,
    children: Vec<serde_json::Value>,
}

impl Detail {
    fn value(&self) -> Value {
        Value::from_serialize(serde_json::json!({
            "id": self.row.id.to_string(),
            "kind": self.row.kind,
            "name": self.row.name,
            "description": self.row.description,
            "lat": self.row.lat,
            "lng": self.row.lng,
            "effective_lat": self.row.effective_lat,
            "effective_lng": self.row.effective_lng,
            "parent_id": self.row.parent_id.map(|u| u.to_string()),
            "parent_name": self.row.parent_name,
            "source_name": self.row.source_name,
            "children": self.children,
        }))
    }
}

async fn load_detail(state: &AppState, id: Uuid) -> AppResult<Detail> {
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.kind::text AS kind, a.name, a.description,
               a.lat, a.lng, r.effective_lat, r.effective_lng, a.parent_id,
               (SELECT p.name FROM artifacts p WHERE p.id = a.parent_id) AS parent_name,
               (SELECT s.name FROM artifacts s WHERE s.id = r.location_source_id) AS source_name
        FROM artifacts a JOIN resolved r ON r.id = a.id
        WHERE a.id = $1 AND a.deleted_at IS NULL
        "#
    );
    let row = sqlx::query_as::<_, DetailRow>(&sql)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("artifact"))?;

    let kids = sqlx::query_as::<_, TreeRow>(
        "SELECT id, kind::text AS kind, name, parent_id FROM artifacts \
         WHERE parent_id = $1 AND deleted_at IS NULL ORDER BY name, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let children: Vec<serde_json::Value> = kids
        .iter()
        .map(|c| serde_json::json!({ "id": c.id.to_string(), "kind": c.kind, "name": c.name }))
        .collect();

    Ok(Detail { row, children })
}

#[derive(sqlx::FromRow)]
struct OptionRow {
    id: Uuid,
    name: String,
    kind: String,
}

/// Parent choices for the form. Excludes the artifact itself; the cycle trigger
/// in the database rejects a deeper cycle, which the handler surfaces as a 400.
async fn load_parent_options(state: &AppState, exclude: Option<Uuid>) -> AppResult<Value> {
    let rows = sqlx::query_as::<_, OptionRow>(
        "SELECT id, name, kind::text AS kind FROM artifacts \
         WHERE deleted_at IS NULL AND ($1::uuid IS NULL OR id <> $1) \
         ORDER BY name",
    )
    .bind(exclude)
    .fetch_all(&state.db)
    .await?;

    let opts: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "label": format!("{} ({})", r.name, r.kind),
            })
        })
        .collect();
    Ok(Value::from_serialize(&opts))
}

/// Turns the schema's integrity errors into 400s that explain themselves,
/// rather than an opaque 500 -- the acyclic trigger and parent FK both surface
/// here.
fn map_db_error(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &err {
        match db.code().as_deref() {
            Some("23514") => return AppError::BadRequest(db.message().to_string()),
            Some("23503") => return AppError::BadRequest("the selected parent does not exist".into()),
            Some("23505") => return AppError::Conflict("that beacon id is already assigned".into()),
            _ => {}
        }
    }
    AppError::from(err)
}

/// Sends an expired studio session to the sign-in page instead of a bare JSON
/// 401. Skips the login page and assets so there is no redirect loop.
pub async fn redirect_unauthenticated(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let path = req.uri().path().to_string();
    let exempt = path == "/studio/login" || path.starts_with("/studio/assets/");
    let response = next.run(req).await;
    if !exempt && response.status() == StatusCode::UNAUTHORIZED {
        return Redirect::to("/studio/login").into_response();
    }
    response
}
