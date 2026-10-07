//! One author's page in the admin panel: change their email, name, and
//! role; reset their password; suspend them; or delete the account.
//!
//! Deleting moves everything the author owns (artifacts and trails) to an
//! author the admin chooses first. Without that, the schema would delete
//! their trails with them (`authors_delete_trails`) and leave their artifacts
//! with no owner. Their entries in each artifact's history keep the email
//! recorded at the time.

use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use axum_extra::extract::cookie::CookieJar;
use minijinja::context;
use serde::Deserialize;
use uuid::Uuid;

use super::csrf;
use super::routes::{author_context, render, require_csrf, session_token, url_encode};
use crate::authors::extractors::RequireAdmin;
use crate::authors::model::{looks_like_email, validate_password, Author, AuthorRole};
use crate::crypto::hash_password;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/authors/{id}", get(edit_page).post(update))
        .route("/admin/authors/{id}/password", post(reset_password))
        .route("/admin/authors/{id}/delete", post(delete))
}

#[derive(sqlx::FromRow)]
struct Target {
    id: Uuid,
    email: String,
    full_name: String,
    role: AuthorRole,
    is_active: bool,
    must_change_password: bool,
    avatar_key: Option<String>,
}

async fn load(state: &AppState, id: Uuid) -> AppResult<Target> {
    sqlx::query_as::<_, Target>(
        "SELECT id, email, full_name, role, is_active, must_change_password, avatar_key \
         FROM authors WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound("author"))
}

#[derive(Debug, Deserialize)]
pub struct EditQuery {
    saved: Option<String>,
}

async fn edit_page(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Query(q): Query<EditQuery>,
) -> AppResult<Html<String>> {
    let flash = q.saved.map(|what| match what.as_str() {
        "password" => "Password reset. They have been signed out, and must choose their own password when they next sign in.".to_string(),
        "suspended" => "Suspended. They have been signed out and can't sign in.".to_string(),
        "reactivated" => "Reactivated. They can sign in again.".to_string(),
        _ => "Saved.".to_string(),
    });
    render_page(&admin.0, &state, &jar, id, flash, None).await
}

async fn render_page(
    admin: &Author,
    state: &AppState,
    jar: &CookieJar,
    id: Uuid,
    flash: Option<String>,
    error: Option<String>,
) -> AppResult<Html<String>> {
    let t = load(state, id).await?;
    let owned_artifacts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM artifacts WHERE created_by = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    let owned_trails: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trails WHERE owner_type = 'author' AND owner_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    // Who can take over their work: other active editors and admins.
    let others: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, full_name, email FROM authors \
         WHERE id <> $1 AND is_active AND role IN ('editor', 'admin') \
         ORDER BY lower(NULLIF(full_name, '')) NULLS LAST, lower(email)",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let others: Vec<_> = others
        .into_iter()
        .map(|(oid, name, email)| {
            let label = if name.trim().is_empty() { email } else { format!("{name} ({email})") };
            context! { id => oid.to_string(), label => label }
        })
        .collect();

    render(
        "author_edit.html",
        context! {
            title => t.full_name.clone().chars().take(60).collect::<String>(),
            section => "authors",
            author => author_context(admin),
            csrf_token => csrf::token_for_session(&session_token(jar)),
            target => context! {
                id => t.id.to_string(), email => t.email, full_name => t.full_name,
                role => t.role.as_str(), is_active => t.is_active,
                must_change_password => t.must_change_password,
            },
            is_self => t.id == admin.id,
            roles => ["viewer", "editor", "admin"],
            owned_artifacts => owned_artifacts,
            owned_trails => owned_trails,
            others => others,
            flash => flash,
            error => error,
        },
    )
}

/// Re-renders the page with an error, for any rejected form.
async fn fail(admin: &Author, state: &AppState, jar: &CookieJar, id: Uuid, msg: String) -> AppResult<Response> {
    Ok(render_page(admin, state, jar, id, None, Some(msg)).await?.into_response())
}

// --- account details ----------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpdateForm {
    csrf_token: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    full_name: String,
    /// Absent when the select is disabled (an admin editing themselves).
    role: Option<String>,
}

async fn update(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<UpdateForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    let t = load(&state, id).await?;

    let email = f.email.trim().to_string();
    if !looks_like_email(&email) {
        return fail(&admin.0, &state, &jar, id, "Enter a valid email address.".into()).await;
    }
    let full_name: String = f.full_name.split_whitespace().collect::<Vec<_>>().join(" ");
    if full_name.chars().count() > 120 {
        return fail(&admin.0, &state, &jar, id, "The full name can be at most 120 characters.".into()).await;
    }
    let role = match f.role.as_deref() {
        // Your own role isn't changeable here: demoting yourself would lock
        // you out of this panel mid-request.
        _ if id == admin.0.id => t.role,
        Some(r) => match r.parse::<AuthorRole>() {
            Ok(role) => role,
            Err(e) => return fail(&admin.0, &state, &jar, id, e).await,
        },
        None => t.role,
    };

    let mut tx = state.db.begin().await?;
    let result = sqlx::query("UPDATE authors SET email = $2, full_name = $3, role = $4 WHERE id = $1")
        .bind(id)
        .bind(&email)
        .bind(&full_name)
        .bind(role)
        .execute(&mut *tx)
        .await;
    match result {
        Ok(_) => {}
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
            return fail(&admin.0, &state, &jar, id, format!("Another author already uses {email}.")).await;
        }
        Err(e) => return Err(e.into()),
    }
    let active_admins: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authors WHERE role = 'admin' AND is_active")
            .fetch_one(&mut *tx)
            .await?;
    if active_admins == 0 {
        return fail(&admin.0, &state, &jar, id, "That change would leave no active admin.".into()).await;
    }
    // A new sign-in address, or less access, should take effect now rather
    // than whenever their session cookie runs out.
    if !email.eq_ignore_ascii_case(&t.email) || role < t.role {
        sqlx::query("DELETE FROM author_sessions WHERE author_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(actor = %admin.0.id, target = %id, role = role.as_str(), "author updated from panel");
    Ok(Redirect::to(&format!("/admin/authors/{id}?saved=details")).into_response())
}

// --- password -------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    csrf_token: String,
    password: String,
    password_confirm: String,
}

async fn reset_password(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<PasswordForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    load(&state, id).await?;
    if id == admin.0.id {
        return fail(&admin.0, &state, &jar, id, "Change your own password from the Studio, under your profile.".into()).await;
    }
    if let Err(e) = validate_password(&f.password) {
        return fail(&admin.0, &state, &jar, id, capitalize(&e)).await;
    }
    if f.password != f.password_confirm {
        return fail(&admin.0, &state, &jar, id, "The two passwords do not match.".into()).await;
    }
    let hash = hash_password(&f.password)?;
    let mut tx = state.db.begin().await?;
    // Flagged, like a new account: a password the admin knows must not stay
    // in use.
    sqlx::query("UPDATE authors SET password_hash = $2, must_change_password = true WHERE id = $1")
        .bind(id)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM author_sessions WHERE author_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(actor = %admin.0.id, target = %id, "author password reset from panel");
    Ok(Redirect::to(&format!("/admin/authors/{id}?saved=password")).into_response())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str() + ".",
        None => String::new(),
    }
}

// --- delete -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeleteForm {
    csrf_token: String,
    #[serde(default)]
    confirm_email: String,
    #[serde(default)]
    transfer_to: String,
}

async fn delete(
    admin: RequireAdmin,
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    Form(f): Form<DeleteForm>,
) -> AppResult<Response> {
    require_csrf(&jar, &f.csrf_token)?;
    let t = load(&state, id).await?;
    if id == admin.0.id {
        return fail(&admin.0, &state, &jar, id, "You can't delete your own account; ask another admin.".into()).await;
    }
    if !f.confirm_email.trim().eq_ignore_ascii_case(&t.email) {
        return fail(&admin.0, &state, &jar, id, "Type their email exactly to confirm the delete.".into()).await;
    }

    let mut tx = crate::audit::begin_as(&state.db, admin.0.id).await?;
    let owns: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM artifacts WHERE created_by = $1) \
              + (SELECT count(*) FROM trails WHERE owner_type = 'author' AND owner_id = $1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let mut moved_to: Option<Uuid> = None;
    if owns > 0 {
        let Ok(to) = f.transfer_to.parse::<Uuid>() else {
            return fail(&admin.0, &state, &jar, id, "Choose who gets their artifacts and trails.".into()).await;
        };
        let ok: Option<bool> = sqlx::query_scalar(
            "SELECT is_active AND role IN ('editor', 'admin') FROM authors WHERE id = $1 AND id <> $2",
        )
        .bind(to)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if ok != Some(true) {
            return fail(&admin.0, &state, &jar, id, "Choose an active editor or admin to take over their work.".into()).await;
        }
        // Deleted artifacts move too, so a later restore has an owner.
        sqlx::query("UPDATE artifacts SET created_by = $2 WHERE created_by = $1")
            .bind(id)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE trails SET owner_id = $2 WHERE owner_type = 'author' AND owner_id = $1")
            .bind(id)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        moved_to = Some(to);
    }

    sqlx::query("DELETE FROM authors WHERE id = $1").bind(id).execute(&mut *tx).await?;
    let active_admins: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authors WHERE role = 'admin' AND is_active")
            .fetch_one(&mut *tx)
            .await?;
    if active_admins == 0 {
        return fail(&admin.0, &state, &jar, id, "That would leave no active admin.".into()).await;
    }
    tx.commit().await?;

    if let Some(key) = t.avatar_key {
        if let Err(e) = state.storage.delete(&key).await {
            tracing::warn!(error = %e, key = %key, "could not delete a removed author's profile photo");
        }
    }
    tracing::info!(actor = %admin.0.id, target = %id, moved_to = ?moved_to, "author deleted from panel");
    Ok(Redirect::to(&format!("/admin/authors?deleted={}", url_encode(&t.email))).into_response())
}
