//! Request extractors that authenticate an author and gate on role.
//!
//! Using an extractor rather than middleware means a handler that needs an
//! admin says so in its own signature:
//!
//! ```ignore
//! async fn takedown(RequireAdmin(author): RequireAdmin, ...) { }
//! ```
//!
//! The check cannot be forgotten, because without the extractor the handler
//! has no author to work with.

use std::marker::PhantomData;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;

use super::auth::{hash_session_token, SESSION_COOKIE};
use super::model::{Author, AuthorRole};
use crate::error::AppError;
use crate::state::AppState;

/// Marker types naming the minimum role a handler requires.
pub trait MinRole {
    const MIN: AuthorRole;
}

pub struct ViewerLevel;
pub struct EditorLevel;
pub struct AdminLevel;

impl MinRole for ViewerLevel {
    const MIN: AuthorRole = AuthorRole::Viewer;
}
impl MinRole for EditorLevel {
    const MIN: AuthorRole = AuthorRole::Editor;
}
impl MinRole for AdminLevel {
    const MIN: AuthorRole = AuthorRole::Admin;
}

/// An authenticated author whose role is at least `R`.
pub struct RequireAuthorRole<R: MinRole>(pub Author, PhantomData<R>);

/// Any authenticated, active author.
pub type AuthenticatedAuthor = RequireAuthorRole<ViewerLevel>;
/// Author who can create and edit content.
pub type RequireEditor = RequireAuthorRole<EditorLevel>;
/// Author who can manage accounts and moderate.
pub type RequireAdmin = RequireAuthorRole<AdminLevel>;

impl<R: MinRole> RequireAuthorRole<R> {
    pub fn into_inner(self) -> Author {
        self.0
    }
}

impl<R> FromRequestParts<AppState> for RequireAuthorRole<R>
where
    R: MinRole,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let raw_token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_owned())
            .ok_or(AppError::Unauthorized)?;

        // The session is looked up by hash; the raw cookie value is never
        // stored, so a database dump yields no usable sessions.
        let token_hash = hash_session_token(&raw_token);

        // One query joins session to author and enforces expiry and activity
        // together, so there is no window where a suspended author's existing
        // session still works.
        let author = sqlx::query_as::<_, Author>(
            r#"
            SELECT a.id, a.email, a.password_hash, a.role, a.is_active,
                   a.created_at, a.updated_at
            FROM author_sessions s
            JOIN authors a ON a.id = s.author_id
            WHERE s.token_hash = $1
              AND s.expires_at > $2
              AND a.is_active
            "#,
        )
        .bind(&token_hash)
        .bind(Utc::now())
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;

        // Authenticated but under-privileged is 403, not 401: retrying with
        // the same credentials will not help.
        if author.role < R::MIN {
            tracing::warn!(
                author_id = %author.id,
                role = author.role.as_str(),
                required = R::MIN.as_str(),
                "author denied: insufficient role"
            );
            return Err(AppError::Forbidden);
        }

        Ok(RequireAuthorRole(author, PhantomData))
    }
}
