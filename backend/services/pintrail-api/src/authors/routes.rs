use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::auth::{
    generate_session_token, hash_password, hash_session_token, verify_dummy_password,
    verify_password, SESSION_COOKIE,
};
use super::extractors::{AuthenticatedAuthor, RequireAdmin};
use super::model::{Author, AuthorRole, AuthorView};
use crate::client_ip::ClientIp;
use crate::rate_limit::{LOGIN_PER_ACCOUNT, LOGIN_PER_IP};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authors/login", post(login))
        .route("/authors/logout", post(logout))
        .route("/authors/me", get(me))
        // Author account management. Admin-only, enforced by the extractor in
        // each handler's signature rather than by route-layer middleware.
        .route("/admin/authors", get(list_authors).post(create_author))
        .route("/admin/authors/{id}", patch(update_author))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Issues a session cookie in exchange for valid credentials.
///
/// Every failure path returns the same opaque message. Distinguishing "no such
/// account" from "wrong password" from "account suspended" tells an attacker
/// which emails are worth attacking.
async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let email = body.email.trim();

    // Two keys: per-account stops guessing one password list against one
    // author, per-address stops spraying one password across many accounts --
    // which the per-account counter never sees. Checked before any database
    // work so a flood costs as little as possible.
    let account_key = format!("author-login:{}", email.to_ascii_lowercase());
    let ip_key = format!("author-login-ip:{ip}");
    state.limiter.check(&ip_key, LOGIN_PER_IP)?;
    state.limiter.check(&account_key, LOGIN_PER_ACCOUNT)?;

    let author = sqlx::query_as::<_, Author>(
        r#"
        SELECT id, email, password_hash, role, is_active, created_at, updated_at
        FROM authors
        WHERE lower(email) = lower($1)
        "#,
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await?;

    let author = match author {
        Some(a) => a,
        None => {
            // Spend the same CPU a real verification would, so response time
            // does not reveal whether the account exists.
            verify_dummy_password(&body.password);
            return Err(AppError::InvalidCredentials);
        }
    };

    if !verify_password(&body.password, &author.password_hash) {
        tracing::warn!(author_id = %author.id, "failed login: bad password");
        return Err(AppError::InvalidCredentials);
    }

    if !author.is_active {
        tracing::warn!(author_id = %author.id, "failed login: account suspended");
        return Err(AppError::InvalidCredentials);
    }

    let token = generate_session_token();

    sqlx::query(
        r#"
        INSERT INTO author_sessions (author_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(author.id)
    .bind(&token.hash)
    .bind(token.expires_at)
    .execute(&state.db)
    .await?;

    // A success clears the counter; otherwise someone who fumbles a password
    // nine times and then gets it right stays one attempt from lockout for
    // the rest of the window.
    state.limiter.reset(&account_key);
    state.limiter.reset(&ip_key);

    tracing::info!(author_id = %author.id, role = author.role.as_str(), "author logged in");

    let jar = jar.add(session_cookie(token.raw, &state, token.expires_at));

    Ok((
        jar,
        Json(json!({ "author": AuthorView::from(author) })),
    ))
}

/// Clears the session, deleting the server-side row rather than only the
/// cookie -- otherwise a copied cookie stays valid until it expires.
async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<Value>)> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        let token_hash = hash_session_token(cookie.value());

        sqlx::query("DELETE FROM author_sessions WHERE token_hash = $1")
            .bind(&token_hash)
            .execute(&state.db)
            .await?;
    }

    // Overwrite with an already-expired cookie so the browser drops it.
    let mut removal = session_cookie(String::new(), &state, Utc::now() - chrono::Duration::days(1));
    removal.make_removal();

    Ok((jar.add(removal), Json(json!({ "status": "logged out" }))))
}

async fn me(author: AuthenticatedAuthor) -> Json<Value> {
    Json(json!({ "author": AuthorView::from(author.into_inner()) }))
}

// --- Admin-only author management -----------------------------------------

async fn list_authors(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> AppResult<Json<Value>> {
    let authors = sqlx::query_as::<_, Author>(
        r#"
        SELECT id, email, password_hash, role, is_active, created_at, updated_at
        FROM authors
        ORDER BY created_at
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let views: Vec<AuthorView> = authors.into_iter().map(AuthorView::from).collect();
    Ok(Json(json!({ "authors": views })))
}

#[derive(Debug, Deserialize)]
pub struct CreateAuthorRequest {
    pub email: String,
    pub password: String,
    pub role: AuthorRole,
}

async fn create_author(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Json(body): Json<CreateAuthorRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let email = body.email.trim();

    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("a valid email is required".into()));
    }
    if body.password.chars().count() < 12 {
        return Err(AppError::BadRequest(
            "password must be at least 12 characters".into(),
        ));
    }

    let hash = hash_password(&body.password)?;

    let author = sqlx::query_as::<_, Author>(
        r#"
        INSERT INTO authors (email, password_hash, role)
        VALUES ($1, $2, $3)
        RETURNING id, email, password_hash, role, is_active, created_at, updated_at
        "#,
    )
    .bind(email)
    .bind(&hash)
    .bind(body.role)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict("an author with that email already exists".into())
        }
        _ => AppError::from(e),
    })?;

    tracing::info!(
        actor = %admin.0.id,
        created = %author.id,
        role = author.role.as_str(),
        "author account created"
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({ "author": AuthorView::from(author) })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct UpdateAuthorRequest {
    pub role: Option<AuthorRole>,
    pub is_active: Option<bool>,
}

async fn update_author(
    admin: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAuthorRequest>,
) -> AppResult<Json<Value>> {
    if body.role.is_none() && body.is_active.is_none() {
        return Err(AppError::BadRequest(
            "provide at least one of role or is_active".into(),
        ));
    }

    // Locking yourself out is the one mistake here with no in-app recovery --
    // it would need someone with database access to undo. Self-demotion and
    // self-suspension are both refused; another admin can still do either.
    if id == admin.0.id {
        if matches!(body.role, Some(r) if r != crate::authors::model::AuthorRole::Admin) {
            return Err(AppError::BadRequest(
                "an admin cannot demote themselves; ask another admin".into(),
            ));
        }
        if body.is_active == Some(false) {
            return Err(AppError::BadRequest(
                "an admin cannot suspend themselves; ask another admin".into(),
            ));
        }
    }

    let mut tx = state.db.begin().await?;

    let author = sqlx::query_as::<_, Author>(
        r#"
        UPDATE authors
        SET role = COALESCE($2, role),
            is_active = COALESCE($3, is_active)
        WHERE id = $1
        RETURNING id, email, password_hash, role, is_active, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(body.role)
    .bind(body.is_active)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound("author"))?;

    // Losing the last admin bricks account management for everyone.
    let active_admins: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM authors WHERE role = 'admin' AND is_active",
    )
    .fetch_one(&mut *tx)
    .await?;

    if active_admins == 0 {
        return Err(AppError::Conflict(
            "that change would leave no active admin".into(),
        ));
    }

    // Suspension must end existing sessions, or the account keeps working
    // until its cookie happens to expire.
    if !author.is_active {
        sqlx::query("DELETE FROM author_sessions WHERE author_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    tracing::info!(actor = %admin.0.id, target = %author.id, "author account updated");

    Ok(Json(json!({ "author": AuthorView::from(author) })))
}

fn session_cookie(
    value: String,
    state: &AppState,
    expires_at: chrono::DateTime<Utc>,
) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, value);

    // Not readable from JavaScript: an XSS bug should not also be a session
    // theft bug.
    cookie.set_http_only(true);
    // Lax rather than Strict: it still blocks the cross-site POST that CSRF
    // needs, while letting an author follow a link into the admin panel and
    // still be logged in.
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    // Configurable only so local development over plain http works; it must
    // be on in any deployment.
    cookie.set_secure(state.settings.cookie_secure);

    let expiry = time::OffsetDateTime::from_unix_timestamp(expires_at.timestamp())
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    cookie.set_expires(expiry);

    cookie
}
