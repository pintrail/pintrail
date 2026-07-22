use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use minijinja::{context, Environment};
use serde::Deserialize;
use uuid::Uuid;

use super::csrf;
use crate::authors::auth::{generate_session_token, hash_session_token, SESSION_COOKIE};
use crate::authors::extractors::RequireAdmin;
use crate::authors::model::Author;
use crate::client_ip::ClientIp;
use crate::crypto::{verify_dummy_password, verify_password};
use crate::error::{AppError, AppResult};
use crate::rate_limit::{LOGIN_PER_ACCOUNT, LOGIN_PER_IP};
use crate::state::AppState;

/// Templates are compiled into the binary rather than read from disk, so the
/// panel cannot break because a deployment forgot to copy a directory.
fn environment() -> Environment<'static> {
    let mut env = Environment::new();

    // minijinja autoescapes by file extension, so every one of these names
    // must end in .html. Comment bodies are arbitrary user input rendered to
    // an admin holding an elevated cookie -- exactly where a stored XSS would
    // hurt most.
    env.add_template("base.html", include_str!("templates/base.html"))
        .expect("base template");
    env.add_template("login.html", include_str!("templates/login.html"))
        .expect("login template");
    env.add_template(
        "comment_queue.html",
        include_str!("templates/partials/comment_queue.html"),
    )
    .expect("comment queue template");
    env.add_template(
        "trail_takedown.html",
        include_str!("templates/partials/trail_takedown.html"),
    )
    .expect("trail takedown template");
    env.add_template(
        "author_list.html",
        include_str!("templates/partials/author_list.html"),
    )
    .expect("author list template");

    env
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin", get(|| async { Redirect::to("/admin/comments") }))
        .route("/admin/login", get(login_form).post(login_submit))
        .route("/admin/logout", post(logout))
        .route("/admin/comments", get(comment_queue))
        .route("/admin/comments/{id}/hide", post(hide_comment))
        .route("/admin/comments/{id}/restore", post(restore_comment))
        .route("/admin/trails", get(trail_list))
        .route("/admin/trails/{id}/delete", post(delete_trail))
        .route("/admin/authors", get(author_list))
        .route("/admin/authors/{id}/toggle", post(toggle_author))
}

fn render(name: &str, ctx: minijinja::Value) -> AppResult<Html<String>> {
    let env = environment();
    let template = env
        .get_template(name)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("template {name}: {e}")))?;

    let html = template
        .render(ctx)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("render {name}: {e}")))?;

    Ok(Html(html))
}

/// Pulls the raw session cookie, which the CSRF token is derived from.
fn session_token(jar: &CookieJar) -> String {
    jar.get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default()
}

/// Rejects a form post whose token does not match the caller's session.
fn require_csrf(jar: &CookieJar, submitted: &str) -> AppResult<()> {
    if csrf::verify(&session_token(jar), submitted) {
        Ok(())
    } else {
        tracing::warn!("admin form rejected: CSRF token mismatch");
        Err(AppError::Forbidden)
    }
}

// --- sign in ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

async fn login_form() -> AppResult<Html<String>> {
    render("login.html", context! { title => "Sign in" })
}

/// Signs in and redirects.
///
/// Duplicates the JSON login rather than calling it, because the two differ in
/// how they answer: this one re-renders a form with an error, and issues a
/// redirect on success so a refresh does not repost the password.
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

    // One message for every failure, and a dummy verification when the account
    // does not exist, so neither the response nor its timing distinguishes the
    // cases. Same reasoning as the JSON endpoint.
    let invalid = || -> AppResult<Response> {
        Ok(render(
            "login.html",
            context! {
                title => "Sign in",
                error => "Invalid email or password.",
            },
        )?
        .into_response())
    };

    let Some(author) = author else {
        verify_dummy_password(&form.password);
        return invalid();
    };

    if !verify_password(&form.password, &author.password_hash) || !author.is_active {
        return invalid();
    }

    // The panel is admin-only. A viewer or editor authenticates fine but has
    // no business here, and gets the same opaque message rather than a hint
    // that their credentials were correct.
    if author.role != crate::authors::model::AuthorRole::Admin {
        tracing::warn!(author_id = %author.id, "non-admin attempted to sign in to the panel");
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

    tracing::info!(author_id = %author.id, "admin signed in to the panel");

    let jar = jar.add(crate::authors::routes::session_cookie(
        token.raw,
        &state,
        token.expires_at,
    ));

    Ok((jar, Redirect::to("/admin/comments")).into_response())
}

#[derive(Debug, Deserialize)]
pub struct CsrfForm {
    pub csrf_token: String,
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

    let mut removal = crate::authors::routes::session_cookie(
        String::new(),
        &state,
        Utc::now() - chrono::Duration::days(1),
    );
    removal.make_removal();

    Ok((jar.add(removal), Redirect::to("/admin/login")).into_response())
}

// --- comment moderation ----------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct QueueParams {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "flagged".to_string()
}

#[derive(Debug, sqlx::FromRow)]
struct QueueRow {
    id: Uuid,
    body: String,
    status: String,
    display_name: Option<String>,
    reader_id: Uuid,
    artifact_name: Option<String>,
    created_at: DateTime<Utc>,
}

async fn comment_queue(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Query(params): Query<QueueParams>,
) -> AppResult<Html<String>> {
    let status = match params.status.as_str() {
        s @ ("flagged" | "visible" | "hidden") => s,
        other => {
            return Err(AppError::BadRequest(format!(
                "unknown status {other:?} (expected flagged, visible, or hidden)"
            )))
        }
    };

    let rows = sqlx::query_as::<_, QueueRow>(
        r#"
        SELECT c.id, c.body, c.status::text AS status, r.display_name, c.reader_id,
               a.name AS artifact_name, c.created_at
        FROM comments c
        JOIN readers r ON r.id = c.reader_id
        JOIN artifacts a ON a.id = c.artifact_id
        WHERE c.status::text = $1
        ORDER BY c.created_at DESC
        LIMIT 200
        "#,
    )
    .bind(status)
    .fetch_all(&state.db)
    .await?;

    let comments: Vec<_> = rows
        .into_iter()
        .map(|r| {
            context! {
                id => r.id.to_string(),
                body => r.body,
                status => r.status,
                // Same pseudonym rule as the public thread: an admin panel is
                // no reason to start rendering email addresses.
                author => crate::comments::model::display_name_for(
                    r.reader_id, r.display_name.as_deref()),
                artifact_name => r.artifact_name.unwrap_or_else(|| "(deleted)".into()),
                created_at => r.created_at.format("%Y-%m-%d %H:%M").to_string(),
            }
        })
        .collect();

    render(
        "comment_queue.html",
        context! {
            title => "Comments",
            section => "comments",
            author => author_context(&admin.0),
            csrf_token => csrf::token_for_session(&session_token(&jar)),
            comments => comments,
        },
    )
}

async fn hide_comment(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    set_comment_status(&state, id, "hidden").await?;
    tracing::info!(actor = %admin.0.id, comment_id = %id, "comment hidden");
    Ok(Redirect::to("/admin/comments"))
}

async fn restore_comment(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;
    set_comment_status(&state, id, "visible").await?;
    tracing::info!(actor = %admin.0.id, comment_id = %id, "comment restored");
    Ok(Redirect::to("/admin/comments?status=hidden"))
}

/// Moderation changes status; it never deletes.
///
/// A hidden comment stays readable to moderators, which matters when a
/// takedown is contested or mistaken. Readers delete their own comments for
/// real — that is their content to remove.
async fn set_comment_status(state: &AppState, id: Uuid, status: &str) -> AppResult<()> {
    let updated = sqlx::query("UPDATE comments SET status = $2::comment_status WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(&state.db)
        .await?
        .rows_affected();

    if updated == 0 {
        return Err(AppError::NotFound("comment"));
    }

    Ok(())
}

// --- trail takedown --------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct TrailRow {
    id: Uuid,
    title: String,
    description: String,
    owner_type: String,
    owner_label: Option<String>,
    owner_id: Uuid,
    stop_count: i64,
    created_at: DateTime<Utc>,
}

async fn trail_list(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    // Public only. Private and unlisted trails are not browsable here --
    // moderation is about what other people can find, and a panel that lets an
    // admin read every private trail is surveillance, not moderation.
    let rows = sqlx::query_as::<_, TrailRow>(
        r#"
        SELECT t.id, t.title, t.description, t.owner_type::text AS owner_type,
               CASE t.owner_type
                   WHEN 'author' THEN (SELECT a.email FROM authors a WHERE a.id = t.owner_id)
                   ELSE (SELECT r.display_name FROM readers r WHERE r.id = t.owner_id)
               END AS owner_label,
               t.owner_id,
               (SELECT count(*) FROM trail_stops s WHERE s.trail_id = t.id) AS stop_count,
               t.created_at
        FROM trails t
        WHERE t.visibility = 'public'
        ORDER BY t.created_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let trails: Vec<_> = rows
        .into_iter()
        .map(|t| {
            // An author's email is fine to show another admin -- they are
            // colleagues. A reader's is not, so readers get their pseudonym.
            let owner = if t.owner_type == "author" {
                t.owner_label.unwrap_or_else(|| "(unknown)".into())
            } else {
                crate::comments::model::display_name_for(t.owner_id, t.owner_label.as_deref())
            };

            context! {
                id => t.id.to_string(),
                title => t.title,
                description => t.description,
                owner => owner,
                owner_type => t.owner_type,
                stop_count => t.stop_count,
                created_at => t.created_at.format("%Y-%m-%d").to_string(),
            }
        })
        .collect();

    render(
        "trail_takedown.html",
        context! {
            title => "Trails",
            section => "trails",
            author => author_context(&admin.0),
            csrf_token => csrf::token_for_session(&session_token(&jar)),
            trails => trails,
        },
    )
}

async fn delete_trail(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;

    let deleted = sqlx::query("DELETE FROM trails WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if deleted == 0 {
        return Err(AppError::NotFound("trail"));
    }

    tracing::info!(actor = %admin.0.id, trail_id = %id, "trail taken down");
    Ok(Redirect::to("/admin/trails"))
}

// --- author management -----------------------------------------------------

async fn author_list(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<Html<String>> {
    let authors = sqlx::query_as::<_, Author>(
        "SELECT id, email, password_hash, role, is_active, created_at, updated_at \
         FROM authors ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;

    let rows: Vec<_> = authors
        .into_iter()
        .map(|a| {
            context! {
                id => a.id.to_string(),
                email => a.email,
                role => a.role.as_str(),
                is_active => a.is_active,
                is_self => a.id == admin.0.id,
                created_at => a.created_at.format("%Y-%m-%d").to_string(),
            }
        })
        .collect();

    render(
        "author_list.html",
        context! {
            title => "Authors",
            section => "authors",
            author => author_context(&admin.0),
            csrf_token => csrf::token_for_session(&session_token(&jar)),
            authors => rows,
        },
    )
}

/// Suspends or reactivates an author.
async fn toggle_author(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    require_csrf(&jar, &form.csrf_token)?;

    // Same guard as the JSON endpoint: locking yourself out is the one mistake
    // here with no in-app recovery.
    if id == admin.0.id {
        return Err(AppError::BadRequest(
            "an admin cannot suspend themselves; ask another admin".into(),
        ));
    }

    let mut tx = state.db.begin().await?;

    let now_active: bool =
        sqlx::query_scalar("UPDATE authors SET is_active = NOT is_active WHERE id = $1 RETURNING is_active")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound("author"))?;

    let active_admins: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authors WHERE role = 'admin' AND is_active")
            .fetch_one(&mut *tx)
            .await?;

    if active_admins == 0 {
        return Err(AppError::Conflict(
            "that change would leave no active admin".into(),
        ));
    }

    // Suspension must end live sessions, or the account keeps working until
    // its cookie expires.
    if !now_active {
        sqlx::query("DELETE FROM author_sessions WHERE author_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    tracing::info!(actor = %admin.0.id, target = %id, now_active, "author toggled");
    Ok(Redirect::to("/admin/authors"))
}

fn author_context(author: &Author) -> minijinja::Value {
    context! {
        email => author.email.clone(),
        role => author.role.as_str(),
    }
}

/// Sends a browser to the sign-in page instead of a JSON 401.
///
/// Without this, an admin whose session expired gets `{"error":"authentication
/// required"}` as a bare page, with no way forward.
pub async fn redirect_unauthenticated(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let is_login_page = req.uri().path() == "/admin/login";
    let response = next.run(req).await;

    if !is_login_page && response.status() == StatusCode::UNAUTHORIZED {
        return Redirect::to("/admin/login").into_response();
    }

    response
}
