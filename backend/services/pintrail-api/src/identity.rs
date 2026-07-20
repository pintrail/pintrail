//! An authenticated caller from either tier.
//!
//! Reads are served to both authors (cookie) and readers (bearer), and trails
//! are owned by either (`owner_type` in docs/DESIGN.md §2.2). Rather than
//! duplicate every read route per tier, [`Identity`] resolves whichever
//! credential the request actually carries.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use uuid::Uuid;

use crate::authors::auth::SESSION_COOKIE;
use crate::authors::extractors::AuthenticatedAuthor;
use crate::authors::model::Author;
use crate::error::AppError;
use crate::readers::extractors::AuthenticatedReader;
use crate::readers::model::Reader;
use crate::state::AppState;

/// Matches the `owner_type` enum in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, sqlx::Type)]
#[sqlx(type_name = "owner_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OwnerType {
    Author,
    Reader,
}

#[derive(Debug, Clone)]
pub enum Identity {
    Author(Author),
    Reader(Reader),
}

impl Identity {
    pub fn id(&self) -> Uuid {
        match self {
            Identity::Author(a) => a.id,
            Identity::Reader(r) => r.id,
        }
    }

    pub fn owner_type(&self) -> OwnerType {
        match self {
            Identity::Author(_) => OwnerType::Author,
            Identity::Reader(_) => OwnerType::Reader,
        }
    }

    pub fn is_author(&self) -> bool {
        matches!(self, Identity::Author(_))
    }
}

impl FromRequestParts<AppState> for Identity {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Dispatch on which credential is present rather than trying both, so
        // a request never costs two session lookups.
        let has_bearer = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim_start().to_ascii_lowercase().starts_with("bearer "))
            .unwrap_or(false);

        if has_bearer {
            let reader = AuthenticatedReader::from_request_parts(parts, state).await?;
            return Ok(Identity::Reader(reader.into_inner()));
        }

        let has_cookie = CookieJar::from_headers(&parts.headers)
            .get(SESSION_COOKIE)
            .is_some();

        if has_cookie {
            let author = AuthenticatedAuthor::from_request_parts(parts, state).await?;
            return Ok(Identity::Author(author.into_inner()));
        }

        Err(AppError::Unauthorized)
    }
}
