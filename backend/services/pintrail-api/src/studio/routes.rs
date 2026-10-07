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
use crate::authors::extractors::{AuthenticatedAuthor, SessionAuthor};
use crate::authors::model::{validate_password, Author, AuthorRole};
use crate::client_ip::ClientIp;
use crate::crypto::{hash_password, verify_dummy_password, verify_password};
use crate::error::{AppError, AppResult, PasswordChangeRequiredMarker};
use crate::rate_limit::{LOGIN_PER_ACCOUNT, LOGIN_PER_IP};
use crate::state::AppState;

const KINDS: [&str; 6] = ["building", "room", "artwork", "installation", "rooftop", "other"];

pub(super) fn environment() -> Environment<'static> {
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
        ("studio_password.html", include_str!("templates/password.html")),
        ("studio_links.html", include_str!("templates/links.html")),
        ("studio_map.html", include_str!("templates/map.html")),
        ("studio_history.html", include_str!("templates/history.html")),
        ("studio_review.html", include_str!("templates/review.html")),
        ("studio_deleted.html", include_str!("templates/deleted.html")),
        ("studio_review_card.html", include_str!("templates/review_card.html")),
        ("studio_macros.html", include_str!("templates/macros.html")),
        (
            "studio_avatar.html",
            r#"{% from "studio_macros.html" import avatar %}{{ avatar(me) }}"#,
        ),
        ("studio_profile.html", include_str!("templates/profile.html")),
        ("studio_profile_photo.html", include_str!("templates/profile_photo.html")),
    ] {
        env.add_template(name, src).expect("studio template compiles");
    }
    env.add_global(
        "status_labels",
        Value::from_serialize(serde_json::json!({
            "draft": "Draft", "ready": "Ready for review", "approved": "Approved",
        })),
    );
    env.add_global("studio_js_version", super::assets::studio_js_version());
    env
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio", get(home))
        .route("/studio/login", get(login_form).post(login_submit))
        .route("/studio/logout", post(logout))
        .route("/studio/password", get(password_form).post(password_submit))
        .route("/studio/welcome", get(welcome))
        .route("/studio/tree", get(tree))
        .route("/studio/artifacts/new", get(new_form))
        .route("/studio/artifacts", post(create))
        .route("/studio/artifacts/{id}", get(detail).post(update))
        .route("/studio/artifacts/{id}/edit", get(edit_form))
        .route("/studio/artifacts/{id}/delete", post(remove))
        .route("/studio/artifacts/{id}/attachments", get(attachments))
        .route("/studio/attachments/{id}/delete", post(delete_attachment))
        .route("/studio/map", get(map_view))
        .route("/studio/artifacts/{id}/links", get(links).post(add_link))
        .route("/studio/artifacts/{id}/links/order", post(reorder_links))
        .route("/studio/links/{id}", post(update_link))
        .route("/studio/links/{id}/delete", post(delete_link))
        .merge(super::review::router())
        .merge(super::profile::router())
}

// --- rendering helpers -----------------------------------------------------

pub(super) fn render(env: &Environment, name: &str, ctx: Value) -> AppResult<Html<String>> {
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
pub(super) fn respond(
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
pub(super) fn base_ctx(author: &Author, session_token: &str) -> Value {
    context! {
        author => context! { email => author.email.clone(), role => author.role.as_str() },
        me => Value::from_serialize(super::profile::person(
            author.id, author.label(), author.avatar_key.as_deref(),
        )),
        can_edit => is_editor(author),
        can_create => is_editor(author),
        is_admin => crate::audit::is_admin(author),
        csrf_token => csrf::token_for_session(session_token),
    }
}

pub(super) fn session_token(jar: &CookieJar) -> String {
    jar.get(SESSION_COOKIE).map(|c| c.value().to_string()).unwrap_or_default()
}

pub(super) fn require_editor(author: &Author) -> AppResult<()> {
    if is_editor(author) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub(super) fn require_csrf(jar: &CookieJar, submitted: &str) -> AppResult<()> {
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
        "SELECT id, email, password_hash, role, is_active, must_change_password, created_at, updated_at \
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
    let landing = if author.must_change_password { "/studio/password" } else { "/studio" };
    Ok((jar, Redirect::to(landing)).into_response())
}

// --- password change -------------------------------------------------------
//
// Lives in the Studio because every author role can sign in here; the admin
// panel is admin-only. It takes `SessionAuthor` rather than a role extractor
// because it must serve exactly the authors those extractors refuse.

fn password_ctx(author: &Author, jar: &CookieJar, error: Option<&str>) -> Value {
    context! {
        title => "Change password",
        email => author.email.clone(),
        required => author.must_change_password,
        csrf_token => csrf::token_for_session(&session_token(jar)),
        error => error,
    }
}

async fn password_form(
    SessionAuthor(author): SessionAuthor,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    render(&environment(), "studio_password.html", password_ctx(&author, &jar, None))
}

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    csrf_token: String,
    current_password: String,
    new_password: String,
    new_password_confirm: String,
}

async fn password_submit(
    SessionAuthor(author): SessionAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<PasswordForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &form.csrf_token)?;

    let reject = |message: &str| -> AppResult<Response> {
        let ctx = password_ctx(&author, &jar, Some(message));
        Ok(render(&environment(), "studio_password.html", ctx)?.into_response())
    };

    // Shares the sign-in counter: a stolen session cookie must not become an
    // unthrottled oracle for guessing the account's password.
    let account_key = format!("author-login:{}", author.email.to_ascii_lowercase());
    state.limiter.check(&account_key, LOGIN_PER_ACCOUNT)?;

    if !verify_password(&form.current_password, &author.password_hash) {
        return reject("Current password is incorrect.");
    }
    state.limiter.reset(&account_key);

    if let Err(e) = validate_password(&form.new_password) {
        return reject(&e);
    }
    if form.new_password != form.new_password_confirm {
        return reject("The new passwords do not match.");
    }
    if form.new_password == form.current_password {
        return reject("Choose a password different from the current one.");
    }

    let hash = hash_password(&form.new_password)?;
    let current_session = hash_session_token(&session_token(&jar));

    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE authors SET password_hash = $1, must_change_password = false WHERE id = $2")
        .bind(&hash)
        .bind(author.id)
        .execute(&mut *tx)
        .await?;
    // Same reasoning as the CLI reset: sessions opened under the old password
    // should not outlive it. This one stays, so the author is not bounced out.
    sqlx::query("DELETE FROM author_sessions WHERE author_id = $1 AND token_hash <> $2")
        .bind(author.id)
        .bind(&current_session)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    tracing::info!(author_id = %author.id, "author changed password");

    let landing = if author.role == AuthorRole::Admin { "/admin" } else { "/studio" };
    Ok(Redirect::to(landing).into_response())
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
    pub(super) csrf_token: String,
}

// --- views -----------------------------------------------------------------

async fn home(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Response> {
    // Everyone gets a name before they start, so the Studio can say who
    // added what rather than showing email addresses.
    if author.0.full_name.trim().is_empty() {
        return Ok(Redirect::to("/studio/profile?welcome=1").into_response());
    }
    // Always the full document -- this is the entry point, not an htmx swap.
    let ctx = context! {
        title => "Studio",
        fragment => "studio_welcome.html",
        selected_id => Value::from(()),
        ..base_ctx(&author.0, &session_token(&jar))
    };
    let _ = &state;
    Ok(render(&environment(), "studio_page.html", ctx)?.into_response())
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
    let roots = load_tree(&state, q.mine.is_some().then_some(author.0.id)).await?;
    render(
        &environment(),
        "studio_tree.html",
        context! {
            roots => roots,
            mine => q.mine.is_some(),
            selected_id => q.selected.map(|u| u.to_string()),
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    selected: Option<Uuid>,
    /// Present (any value) to show only the signed-in author's artifacts.
    mine: Option<String>,
}

async fn detail(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let ctx = detail_ctx(&state, &author.0, &jar, id).await?;
    respond(&environment(), &headers, "studio_detail.html", ctx)
}

/// Everything the artifact page needs, including what this author may do
/// with it. `can_edit` here means "may change this artifact" (it drives the
/// links and media controls); `can_create` is the general editor right to
/// add artifacts, including inside someone else's.
async fn detail_ctx(state: &AppState, author: &Author, jar: &CookieJar, id: Uuid) -> AppResult<Value> {
    let d = load_detail(state, id).await?;
    let own = crate::audit::Ownership { created_by: d.row.created_by };
    let can_modify = crate::audit::may_modify(author, &own);
    let can_delete = crate::audit::can_delete(&state.db, author, id).await?;
    Ok(context! {
        title => d.row.name.clone(),
        a => d.value(),
        can_edit => can_modify,
        can_delete => can_delete,
        can_create => is_editor(author),
        is_owner => d.row.created_by == Some(author.id),
        ..base_ctx(author, &session_token(jar))
    })
}

async fn attachments(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    let own = crate::audit::ownership(&state.db, id).await?;
    let can_modify = crate::audit::may_modify(&author.0, &own);
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
            can_edit => can_modify,
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
    let form = context! {
        name => "", kind => "building", description => "", tags => "",
        parent_id => q.parent.map(|u| u.to_string()).unwrap_or_default(), lat => "", lng => "",
    };
    render_form(&state, &author.0, &jar, &headers, None, form, None).await
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
    crate::audit::ensure_can_modify(&state.db, &author.0, id).await?;
    let d = load_detail(&state, id).await?;

    // Empty strings, not nulls, so the inputs render blank rather than the
    // literal "none" and so a blank submission means "inherit".
    let form = context! {
        name => d.row.name.clone(),
        kind => d.row.kind.clone(),
        description => d.row.description.clone(),
        tags => d.tags.join(", "),
        parent_id => d.row.parent_id.map(|u| u.to_string()).unwrap_or_default(),
        lat => d.row.lat.map(|v| v.to_string()).unwrap_or_default(),
        lng => d.row.lng.map(|v| v.to_string()).unwrap_or_default(),
        owner_id => d.row.created_by.map(|u| u.to_string()).unwrap_or_default(),
    };
    render_form(&state, &author.0, &jar, &headers, Some(id), form, None).await
}

/// Renders the create (`edit_id` None) or edit form. Shared by the GET
/// handlers and by a failed submission, which re-renders the author's own
/// input with the problem stated above it rather than losing their work.
async fn render_form(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    headers: &HeaderMap,
    edit_id: Option<Uuid>,
    form: Value,
    error: Option<&str>,
) -> AppResult<Html<String>> {
    let parents = load_parent_options(state, edit_id).await?;
    let tag_suggestions = load_tag_suggestions(state).await?;
    // Admins can hand an artifact to another author (or claim one that
    // predates authorship). Everyone else owns what they create.
    let owners = if edit_id.is_some() && crate::audit::is_admin(author) {
        load_owner_options(state).await?
    } else {
        Value::from(())
    };
    let (title, heading, action, submit_label) = match edit_id {
        Some(id) => ("Edit", "Edit artifact", format!("/studio/artifacts/{id}"), "Save changes"),
        None => ("New artifact", "New artifact", "/studio/artifacts".to_string(), "Create"),
    };
    let ctx = context! {
        title => title,
        heading => heading,
        action => action,
        submit_label => submit_label,
        cancel_id => edit_id.map(|u| u.to_string()),
        kinds => KINDS,
        parents => parents,
        tag_suggestions => tag_suggestions,
        owners => owners,
        form => form,
        error => error,
        ..base_ctx(author, &session_token(jar))
    };
    respond(&environment(), headers, "studio_form.html", ctx)
}

#[derive(Debug, Deserialize)]
pub struct ArtifactForm {
    csrf_token: String,
    name: String,
    kind: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    parent_id: String,
    #[serde(default)]
    lat: String,
    #[serde(default)]
    lng: String,
    /// Admin-only, on the edit form: the artifact's owner.
    #[serde(default)]
    owner_id: Option<String>,
}

/// A form submission that passed validation.
struct ValidArtifact {
    kind: String,
    name: String,
    description: String,
    tags: Vec<String>,
    parent: Option<Uuid>,
    lat: Option<f64>,
    lng: Option<f64>,
}

impl ArtifactForm {
    /// The submitted values, echoed back into the form on a failed save.
    fn echo(&self) -> Value {
        context! {
            name => self.name.clone(), kind => self.kind.clone(),
            description => self.description.clone(), tags => self.tags.clone(),
            parent_id => self.parent_id.clone(), lat => self.lat.clone(), lng => self.lng.clone(),
            owner_id => self.owner_id.clone().unwrap_or_default(),
        }
    }

    /// Checks everything the database would otherwise reject with an opaque
    /// error, and returns a message an author can act on.
    fn validate(&self, self_id: Option<Uuid>) -> Result<ValidArtifact, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Give the artifact a name.".into());
        }
        if name.chars().count() > 200 {
            return Err("The name is too long (200 characters at most).".into());
        }
        let kind = self.kind.trim();
        if !KINDS.contains(&kind) {
            return Err("Choose a kind from the list.".into());
        }

        let parent = match self.parent_id.trim() {
            "" => None,
            s => Some(Uuid::parse_str(s).map_err(|_| "Choose a parent from the list.".to_string())?),
        };
        if parent.is_some() && parent == self_id {
            return Err("An artifact cannot be inside itself.".into());
        }

        let (lat, lng) = parse_coords(&self.lat, &self.lng)?;
        if parent.is_none() && lat.is_none() {
            return Err("A top-level artifact needs a location. Click the map to place it, \
                        or choose the artifact it sits inside under Inside (parent)."
                .into());
        }

        Ok(ValidArtifact {
            kind: kind.to_string(),
            name: name.to_string(),
            description: self.description.trim().to_string(),
            tags: parse_tags(&self.tags)?,
            parent,
            lat,
            lng,
        })
    }
}

/// Parses the latitude and longitude boxes. Also accepts a "lat, lng" pair
/// pasted whole into the latitude box, which is how Google Maps copies one.
fn parse_coords(lat: &str, lng: &str) -> Result<(Option<f64>, Option<f64>), String> {
    let (lat, lng) = match (lat.trim(), lng.trim()) {
        (pair, "") if pair.contains(',') => {
            let mut it = pair.splitn(2, ',');
            (it.next().unwrap_or("").trim().to_string(), it.next().unwrap_or("").trim().to_string())
        }
        (a, b) => (a.to_string(), b.to_string()),
    };
    let num = |s: &str, what: &str| -> Result<Option<f64>, String> {
        if s.is_empty() {
            return Ok(None);
        }
        s.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(Some)
            .ok_or_else(|| format!("The {what} must be a number, like 42.3910."))
    };
    let (lat, lng) = (num(&lat, "latitude")?, num(&lng, "longitude")?);
    match (lat, lng) {
        (Some(_), None) | (None, Some(_)) => {
            Err("Enter both latitude and longitude, or clear both to inherit the parent's location.".into())
        }
        (Some(a), _) if !(-90.0..=90.0).contains(&a) => Err("Latitude must be between -90 and 90.".into()),
        (_, Some(b)) if !(-180.0..=180.0).contains(&b) => Err("Longitude must be between -180 and 180.".into()),
        pair => Ok(pair),
    }
}

/// Splits the tag field into clean, distinct tags. Tags are free text; only
/// whitespace is normalised, and duplicates are dropped ignoring case.
pub(crate) fn parse_tags(raw: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for piece in raw.split([',', '\n']) {
        let tag = piece.split_whitespace().collect::<Vec<_>>().join(" ");
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 60 {
            return Err(format!("The tag \u{201c}{tag}\u{201d} is too long (60 characters at most)."));
        }
        if !out.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            out.push(tag);
        }
    }
    if out.len() > 30 {
        return Err("That's a lot of tags. Keep it to 30 or fewer.".into());
    }
    Ok(out)
}

/// A database rejection worth showing in the form (the acyclic trigger, a
/// parent deleted meanwhile), or a real failure.
fn form_db_error(err: sqlx::Error) -> Result<String, AppError> {
    match map_db_error(err) {
        AppError::BadRequest(m) | AppError::Conflict(m) => {
            if m.contains("cycle") {
                Ok("That parent is inside this artifact already, so it can't also contain it.".into())
            } else {
                Ok(m)
            }
        }
        other => Err(other),
    }
}

async fn replace_tags(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    // Only the difference is written, so the history records tags actually
    // added and removed rather than every tag on every save.
    let lowered: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
    sqlx::query("DELETE FROM artifact_tags WHERE artifact_id = $1 AND NOT (lower(tag) = ANY($2))")
        .bind(id)
        .bind(&lowered)
        .execute(&mut **tx)
        .await?;
    if !tags.is_empty() {
        sqlx::query(
            "INSERT INTO artifact_tags (artifact_id, tag) SELECT $1, unnest($2::text[]) \
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(tags)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
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

    let v = match form.validate(None) {
        Ok(v) => v,
        Err(msg) => {
            return Ok(render_form(&state, &author.0, &jar, &headers, None, form.echo(), Some(&msg))
                .await?
                .into_response())
        }
    };

    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    let inserted: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
        "INSERT INTO artifacts (kind, name, description, lat, lng, parent_id) \
         VALUES ($1::artifact_kind, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(&v.kind)
    .bind(&v.name)
    .bind(&v.description)
    .bind(v.lat)
    .bind(v.lng)
    .bind(v.parent)
    .fetch_one(&mut *tx)
    .await;
    let id = match inserted {
        Ok(id) => id,
        Err(e) => {
            let msg = form_db_error(e)?;
            return Ok(render_form(&state, &author.0, &jar, &headers, None, form.echo(), Some(&msg))
                .await?
                .into_response());
        }
    };
    replace_tags(&mut tx, id, &v.tags).await?;
    tx.commit().await?;

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
    crate::audit::ensure_can_modify(&state.db, &author.0, id).await?;

    let v = match form.validate(Some(id)) {
        Ok(v) => v,
        Err(msg) => {
            return Ok(render_form(&state, &author.0, &jar, &headers, Some(id), form.echo(), Some(&msg))
                .await?
                .into_response())
        }
    };

    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    let updated = sqlx::query(
        "UPDATE artifacts SET kind = $2::artifact_kind, name = $3, description = $4, \
         lat = $5, lng = $6, parent_id = $7 WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(&v.kind)
    .bind(&v.name)
    .bind(&v.description)
    .bind(v.lat)
    .bind(v.lng)
    .bind(v.parent)
    .execute(&mut *tx)
    .await;
    let affected = match updated {
        Ok(r) => r.rows_affected(),
        Err(e) => {
            let msg = form_db_error(e)?;
            return Ok(render_form(&state, &author.0, &jar, &headers, Some(id), form.echo(), Some(&msg))
                .await?
                .into_response());
        }
    };
    if affected == 0 {
        return Err(AppError::NotFound("artifact"));
    }
    replace_tags(&mut tx, id, &v.tags).await?;
    if crate::audit::is_admin(&author.0) {
        if let Some(owner) = form.owner_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let owner = Uuid::parse_str(owner).map_err(|_| AppError::BadRequest("invalid owner".into()))?;
            sqlx::query("UPDATE artifacts SET created_by = $2 WHERE id = $1 AND created_by IS DISTINCT FROM $2")
                .bind(id)
                .bind(owner)
                .execute(&mut *tx)
                .await?;
        }
    }
    crate::audit::reopen_if_approved(&mut tx, &author.0, id).await?;
    tx.commit().await?;

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

    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    if !crate::audit::can_delete(&mut *tx, &author.0, id).await? {
        return Err(AppError::Forbidden);
    }
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
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

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

    let artifact_id: Uuid = sqlx::query_scalar("SELECT artifact_id FROM attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("attachment"))?;
    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    crate::audit::ensure_can_modify(&mut *tx, &author.0, artifact_id).await?;
    sqlx::query("DELETE FROM attachments WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    crate::audit::touch(&mut tx, &author.0, artifact_id).await?;
    tx.commit().await?;
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
            can_edit => true,
            ..base_ctx(&author.0, &session_token(&jar))
        },
    )
}

/// After a create or update, render the detail fragment and tell the sidebar
/// tree to refresh via an HX-Trigger response header.
pub(super) async fn detail_after_change(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    headers: &HeaderMap,
    id: Uuid,
) -> AppResult<Response> {
    let ctx = detail_ctx(state, author, jar, id).await?;
    let body = respond(&environment(), headers, "studio_detail.html", ctx)?;
    // Put the saved artifact's address in the location bar, so a reload or a
    // shared link lands back on it rather than on the welcome page.
    let url = format!("/studio/artifacts/{id}");
    Ok(([("HX-Trigger", "refresh-tree".to_string()), ("HX-Push-Url", url)], body).into_response())
}

// --- map overview --------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MapRow {
    id: Uuid,
    kind: String,
    name: String,
    parent_id: Option<Uuid>,
    effective_lat: Option<f64>,
    effective_lng: Option<f64>,
    location_source_id: Option<Uuid>,
    tags: Vec<String>,
    status: String,
}

/// Every artifact on one map. One marker per artifact that has its own
/// coordinates; artifacts that inherit theirs are listed in the popup of the
/// marker they inherit from, since they would otherwise sit stacked on it.
async fn map_view(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> AppResult<Html<String>> {
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.kind::text AS kind, a.name, a.parent_id,
               r.effective_lat, r.effective_lng, r.location_source_id,
               COALESCE((SELECT array_agg(t.tag ORDER BY lower(t.tag))
                         FROM artifact_tags t WHERE t.artifact_id = a.id), '{{}}') AS tags,
               a.status::text AS status
        FROM artifacts a JOIN resolved r ON r.id = a.id
        WHERE a.deleted_at IS NULL
        ORDER BY a.name, a.id
        "#
    );
    let rows = sqlx::query_as::<_, MapRow>(&sql).fetch_all(&state.db).await?;

    let item = |r: &MapRow| {
        serde_json::json!({
            "id": r.id.to_string(), "kind": r.kind, "name": r.name, "tags": r.tags, "status": r.status,
            "parent_id": r.parent_id.map(|u| u.to_string()),
        })
    };

    // Owners first, so each inheritor can be attached to its owner's marker.
    let mut markers: Vec<serde_json::Value> = Vec::new();
    let mut index = std::collections::HashMap::new();
    for r in rows.iter().filter(|r| r.location_source_id == Some(r.id)) {
        index.insert(r.id, markers.len());
        let mut m = item(r);
        m["lat"] = serde_json::json!(r.effective_lat);
        m["lng"] = serde_json::json!(r.effective_lng);
        m["inside"] = serde_json::json!([]);
        markers.push(m);
    }
    let mut unplaced = Vec::new();
    for r in rows.iter().filter(|r| r.location_source_id != Some(r.id)) {
        match r.location_source_id.and_then(|src| index.get(&src)) {
            Some(&i) => markers[i]["inside"].as_array_mut().expect("array").push(item(r)),
            None => unplaced.push(item(r)),
        }
    }

    let mut all_tags: Vec<String> = Vec::new();
    for r in &rows {
        for t in &r.tags {
            if !all_tags.iter().any(|x| x.to_lowercase() == t.to_lowercase()) {
                all_tags.push(t.clone());
            }
        }
    }
    all_tags.sort_by_key(|t| t.to_lowercase());

    let data = serde_json::json!({ "markers": markers });
    let ctx = context! {
        title => "Map",
        map_data => script_json(&data),
        total => rows.len(),
        placed => markers.len(),
        unplaced => Value::from_serialize(&unplaced),
        kinds => KINDS,
        all_tags => all_tags,
        ..base_ctx(&author.0, &session_token(&jar))
    };
    respond(&environment(), &headers, "studio_map.html", ctx)
}

/// JSON for a `<script type="application/json">` block. `<`, `>`, and `&` are
/// escaped as \u sequences (still valid JSON) so an artifact named
/// "</script>" cannot end the block early; marked safe so autoescaping leaves
/// the quotes alone.
fn script_json(data: &serde_json::Value) -> Value {
    let json = data
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    Value::from_safe_string(json)
}

// --- links ------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct LinkRow {
    id: Uuid,
    artifact_id: Uuid,
    url: String,
    note: String,
    preview_title: Option<String>,
    preview_description: Option<String>,
    preview_image_url: Option<String>,
    preview_site_name: Option<String>,
}

impl LinkRow {
    fn view(&self) -> serde_json::Value {
        let parsed = reqwest::Url::parse(&self.url).ok();
        let host = parsed
            .as_ref()
            .and_then(|u| u.host_str())
            .map(|h| h.trim_start_matches("www.").to_string())
            .unwrap_or_default();
        let favicon = parsed
            .as_ref()
            .and_then(|u| u.host_str().map(|h| format!("{}://{}/favicon.ico", u.scheme(), h)));
        serde_json::json!({
            "id": self.id.to_string(),
            "url": self.url,
            "note": self.note,
            "host": host,
            "favicon": favicon,
            "title": self.preview_title,
            "description": self.preview_description,
            "image": self.preview_image_url,
            "site_name": self.preview_site_name,
        })
    }
}

const LINK_COLUMNS: &str = "id, artifact_id, url, note, preview_title, preview_description, \
                            preview_image_url, preview_site_name";

#[derive(Debug, Deserialize)]
pub struct LinkForm {
    csrf_token: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
pub struct OrderForm {
    csrf_token: String,
    #[serde(default)]
    ids: String,
}

/// Renders the links card. `error` and `draft` re-show a rejected add (or,
/// with `edit_id`, a rejected edit) with the author's input intact.
async fn render_links(
    state: &AppState,
    author: &Author,
    jar: &CookieJar,
    artifact_id: Uuid,
    error: Option<&str>,
    draft: Value,
    edit_id: Option<Uuid>,
) -> AppResult<Html<String>> {
    let sql = format!(
        "SELECT {LINK_COLUMNS} FROM artifact_links WHERE artifact_id = $1 ORDER BY position, created_at"
    );
    let rows = sqlx::query_as::<_, LinkRow>(&sql)
        .bind(artifact_id)
        .fetch_all(&state.db)
        .await?;
    let links: Vec<serde_json::Value> = rows.iter().map(LinkRow::view).collect();
    render(
        &environment(),
        "studio_links.html",
        context! {
            artifact_id => artifact_id.to_string(),
            links => Value::from_serialize(&links),
            error => error,
            draft => draft,
            edit_id => edit_id.map(|u| u.to_string()),
            can_edit => links_editable(state, author, artifact_id).await?,
            ..base_ctx(author, &session_token(jar))
        },
    )
}


async fn ensure_artifact(state: &AppState, id: Uuid) -> AppResult<()> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM artifacts WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    if exists { Ok(()) } else { Err(AppError::NotFound("artifact")) }
}

fn clean_note(note: &str) -> Result<String, String> {
    let note = note.trim().to_string();
    if note.chars().count() > 500 {
        return Err("Keep the description of the link under 500 characters.".into());
    }
    Ok(note)
}

async fn links(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> AppResult<Html<String>> {
    ensure_artifact(&state, id).await?;
    render_links(&state, &author.0, &jar, id, None, Value::from(()), None).await
}

/// The links card's controls follow the artifact's owner, not just the role.
async fn links_editable(state: &AppState, author: &Author, artifact_id: Uuid) -> AppResult<bool> {
    let own = crate::audit::ownership(&state.db, artifact_id).await?;
    Ok(crate::audit::may_modify(author, &own))
}

async fn add_link(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<LinkForm>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    crate::audit::ensure_can_modify(&state.db, &author.0, id).await?;

    let draft = context! { url => form.url.clone(), note => form.note.clone() };
    let checked = super::preview::normalize_url(&form.url).and_then(|u| Ok((u, clean_note(&form.note)?)));
    let (url, note) = match checked {
        Ok(v) => v,
        Err(msg) => return render_links(&state, &author.0, &jar, id, Some(&msg), draft, None).await,
    };

    let preview = super::preview::fetch(&url).await.unwrap_or_default();
    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    sqlx::query(
        "INSERT INTO artifact_links (artifact_id, url, note, position, preview_title, \
         preview_description, preview_image_url, preview_site_name, preview_fetched_at) \
         VALUES ($1, $2, $3, \
                 (SELECT COALESCE(max(position) + 1, 0) FROM artifact_links WHERE artifact_id = $1), \
                 $4, $5, $6, $7, now())",
    )
    .bind(id)
    .bind(url.as_str())
    .bind(&note)
    .bind(&preview.title)
    .bind(&preview.description)
    .bind(&preview.image_url)
    .bind(&preview.site_name)
    .execute(&mut *tx)
    .await?;
    crate::audit::touch(&mut tx, &author.0, id).await?;
    tx.commit().await?;

    tracing::info!(actor = %author.0.id, artifact_id = %id, "link added in studio");
    render_links(&state, &author.0, &jar, id, None, Value::from(()), None).await
}

async fn load_link(state: &AppState, id: Uuid) -> AppResult<LinkRow> {
    let sql = format!("SELECT {LINK_COLUMNS} FROM artifact_links WHERE id = $1");
    sqlx::query_as::<_, LinkRow>(&sql)
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("link"))
}

async fn update_link(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(link_id): Path<Uuid>,
    Form(form): Form<LinkForm>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let link = load_link(&state, link_id).await?;
    crate::audit::ensure_can_modify(&state.db, &author.0, link.artifact_id).await?;

    let draft = context! { url => form.url.clone(), note => form.note.clone() };
    let checked = super::preview::normalize_url(&form.url).and_then(|u| Ok((u, clean_note(&form.note)?)));
    let (url, note) = match checked {
        Ok(v) => v,
        Err(msg) => {
            return render_links(&state, &author.0, &jar, link.artifact_id, Some(&msg), draft, Some(link_id)).await
        }
    };

    // Re-read the page only when the address changed, or the first fetch
    // came back empty and the author is effectively asking to try again.
    let refetch = url.as_str() != link.url || link.preview_title.is_none();
    let preview = if refetch { super::preview::fetch(&url).await.unwrap_or_default() } else { Default::default() };
    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    if refetch {
        sqlx::query(
            "UPDATE artifact_links SET url = $2, note = $3, preview_title = $4, \
             preview_description = $5, preview_image_url = $6, preview_site_name = $7, \
             preview_fetched_at = now() WHERE id = $1",
        )
        .bind(link_id)
        .bind(url.as_str())
        .bind(&note)
        .bind(&preview.title)
        .bind(&preview.description)
        .bind(&preview.image_url)
        .bind(&preview.site_name)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("UPDATE artifact_links SET note = $2 WHERE id = $1")
            .bind(link_id)
            .bind(&note)
            .execute(&mut *tx)
            .await?;
    }
    crate::audit::touch(&mut tx, &author.0, link.artifact_id).await?;
    tx.commit().await?;
    render_links(&state, &author.0, &jar, link.artifact_id, None, Value::from(()), None).await
}

async fn delete_link(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(link_id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let artifact_id = load_link(&state, link_id).await?.artifact_id;
    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    crate::audit::ensure_can_modify(&mut *tx, &author.0, artifact_id).await?;
    sqlx::query("DELETE FROM artifact_links WHERE id = $1")
        .bind(link_id)
        .execute(&mut *tx)
        .await?;
    crate::audit::touch(&mut tx, &author.0, artifact_id).await?;
    tx.commit().await?;
    render_links(&state, &author.0, &jar, artifact_id, None, Value::from(()), None).await
}

/// Saves a drag-and-drop reorder. `ids` is the full list in its new order;
/// ids that don't belong to this artifact are ignored.
async fn reorder_links(
    author: AuthenticatedAuthor,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<OrderForm>,
) -> AppResult<Html<String>> {
    require_editor(&author.0)?;
    require_csrf(&jar, &form.csrf_token)?;
    let ids: Vec<Uuid> = form
        .ids
        .split(',')
        .filter_map(|s| Uuid::parse_str(s.trim()).ok())
        .collect();
    let mut tx = crate::audit::begin_as(&state.db, author.0.id).await?;
    crate::audit::ensure_can_modify(&mut *tx, &author.0, id).await?;
    let moved = sqlx::query(
        "UPDATE artifact_links l SET position = o.ord - 1 \
         FROM unnest($2::uuid[]) WITH ORDINALITY AS o(id, ord) \
         WHERE l.id = o.id AND l.artifact_id = $1 AND l.position IS DISTINCT FROM o.ord - 1",
    )
    .bind(id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if moved > 0 {
        // One history entry for the whole reorder, not one per moved link.
        sqlx::query("SELECT artifact_history_add($1, 'links_reordered', '{}')")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        crate::audit::touch(&mut tx, &author.0, id).await?;
    }
    tx.commit().await?;
    render_links(&state, &author.0, &jar, id, None, Value::from(()), None).await
}

// --- data loading ----------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TreeRow {
    id: Uuid,
    kind: String,
    name: String,
    parent_id: Option<Uuid>,
    status: String,
}

/// Loads every artifact and nests it into a root->children forest for the
/// sidebar. One query, assembled in memory, rather than a query per level.
async fn load_tree(state: &AppState, mine: Option<Uuid>) -> AppResult<Value> {
    let rows = sqlx::query_as::<_, TreeRow>(
        "SELECT id, kind::text AS kind, name, parent_id, status::text AS status \
         FROM artifacts WHERE deleted_at IS NULL ORDER BY name, id",
    )
    .fetch_all(&state.db)
    .await?;

    // "Only mine" is a flat list: the author's artifacts, wherever they sit.
    if let Some(me) = mine {
        let owned: std::collections::HashSet<Uuid> = sqlx::query_scalar(
            "SELECT id FROM artifacts WHERE deleted_at IS NULL AND created_by = $1",
        )
        .bind(me)
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .collect();
        let flat: Vec<serde_json::Value> = rows
            .iter()
            .filter(|r| owned.contains(&r.id))
            .map(|r| serde_json::json!({
                "id": r.id.to_string(), "kind": r.kind, "name": r.name,
                "status": r.status, "children": [],
            }))
            .collect();
        return Ok(Value::from_serialize(&flat));
    }

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
                            "status": r.status,
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
    created_by: Option<Uuid>,
    updated_by: Option<Uuid>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    status: String,
    submitted_at: Option<chrono::DateTime<Utc>>,
    reviewed_at: Option<chrono::DateTime<Utc>>,
    reviewed_by: Option<Uuid>,
    review_note: String,
}

/// A loaded artifact plus its direct children. Keeps the typed row around so
/// handlers can populate the edit form without round-tripping through a
/// `minijinja::Value`.
struct Detail {
    /// The people the page names, keyed by author id.
    people: std::collections::HashMap<Uuid, serde_json::Value>,
    row: DetailRow,
    children: Vec<serde_json::Value>,
    tags: Vec<String>,
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
            "tags": self.tags,
            "status": self.row.status,
            "created_by": self.row.created_by.and_then(|id| self.people.get(&id)),
            "updated_by": self.row.updated_by.and_then(|id| self.people.get(&id)),
            "created_at": self.row.created_at.to_rfc3339(),
            "updated_at": self.row.updated_at.to_rfc3339(),
            "submitted_at": self.row.submitted_at.map(|t| t.to_rfc3339()),
            "reviewed_at": self.row.reviewed_at.map(|t| t.to_rfc3339()),
            "reviewed_by": self.row.reviewed_by.and_then(|id| self.people.get(&id)),
            "review_note": self.row.review_note,
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
               (SELECT s.name FROM artifacts s WHERE s.id = r.location_source_id) AS source_name,
               a.created_by,
               a.updated_by,
               a.created_at, a.updated_at, a.status::text AS status, a.submitted_at,
               a.reviewed_at,
               a.reviewed_by,
               a.review_note
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
        "SELECT id, kind::text AS kind, name, parent_id, status::text AS status FROM artifacts \
         WHERE parent_id = $1 AND deleted_at IS NULL ORDER BY name, id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let children: Vec<serde_json::Value> = kids
        .iter()
        .map(|c| serde_json::json!({ "id": c.id.to_string(), "kind": c.kind, "name": c.name, "status": c.status }))
        .collect();

    let tags = load_tags(state, id).await?;
    let ids: Vec<Uuid> = [row.created_by, row.updated_by, row.reviewed_by].into_iter().flatten().collect();
    let people = super::profile::load_people(&state.db, &ids).await?;
    Ok(Detail { people, row, children, tags })
}

async fn load_tags(state: &AppState, id: Uuid) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT tag FROM artifact_tags WHERE artifact_id = $1 ORDER BY lower(tag)")
        .bind(id)
        .fetch_all(&state.db)
        .await?)
}

/// Every tag in use, most common first, for the form's autocomplete. One
/// spelling per tag ignoring case, so suggestions steer authors toward the
/// existing "solar" rather than a new "Solar".
async fn load_tag_suggestions(state: &AppState) -> AppResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT min(tag) FROM artifact_tags t JOIN artifacts a ON a.id = t.artifact_id \
         WHERE a.deleted_at IS NULL GROUP BY lower(tag) ORDER BY count(*) DESC, lower(tag) LIMIT 300",
    )
    .fetch_all(&state.db)
    .await?)
}

#[derive(sqlx::FromRow)]
struct OptionRow {
    id: Uuid,
    name: String,
    kind: String,
    effective_lat: Option<f64>,
    effective_lng: Option<f64>,
    source_name: Option<String>,
}

/// Parent choices for the form, each with the location a child placed inside
/// it would inherit (its own, or the nearest ancestor's), so the map can show
/// that spot before the author refines it. Excludes the artifact itself; the
/// cycle trigger in the database rejects a deeper cycle, which the handler
/// surfaces in the form.
async fn load_parent_options(state: &AppState, exclude: Option<Uuid>) -> AppResult<Value> {
    let sql = format!(
        r#"
        {RESOLVED_COORDS_CTE}
        SELECT a.id, a.name, a.kind::text AS kind, r.effective_lat, r.effective_lng,
               (SELECT s.name FROM artifacts s WHERE s.id = r.location_source_id) AS source_name
        FROM artifacts a JOIN resolved r ON r.id = a.id
        WHERE a.deleted_at IS NULL AND ($1::uuid IS NULL OR a.id <> $1)
        ORDER BY a.name
        "#
    );
    let rows = sqlx::query_as::<_, OptionRow>(&sql)
        .bind(exclude)
        .fetch_all(&state.db)
        .await?;

    let opts: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.to_string(),
                "label": format!("{} ({})", r.name, r.kind),
                "lat": r.effective_lat.map(|v| v.to_string()).unwrap_or_default(),
                "lng": r.effective_lng.map(|v| v.to_string()).unwrap_or_default(),
                "source": r.source_name.clone().unwrap_or_default(),
            })
        })
        .collect();
    Ok(Value::from_serialize(&opts))
}

/// Authors who can own artifacts, for the admin's owner picker.
async fn load_owner_options(state: &AppState) -> AppResult<Value> {
    // Named by full name where there is one, with the email to tell apart
    // two people with the same name.
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, full_name, email FROM authors WHERE role IN ('editor', 'admin') AND is_active \
         ORDER BY lower(NULLIF(full_name, '')) NULLS LAST, lower(email)",
    )
    .fetch_all(&state.db)
    .await?;
    let opts: Vec<serde_json::Value> = rows
        .iter()
        .map(|(id, name, email)| {
            let label = if name.trim().is_empty() { email.clone() } else { format!("{name} ({email})") };
            serde_json::json!({ "id": id.to_string(), "label": label })
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
    if response.extensions().get::<PasswordChangeRequiredMarker>().is_some() {
        return Redirect::to("/studio/password").into_response();
    }
    response
}
