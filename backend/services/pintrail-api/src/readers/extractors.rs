//! Bearer-token extractors for the reader tier.
//!
//! Two levels, mirroring the author tier's role gate:
//!
//! - [`AuthenticatedReader`] — signed in; enough to browse.
//! - [`VerifiedReader`] — signed in *and* email-verified; required to write.
//!
//! DESIGN.md §2.6 requires verification before posting comments or trails.
//! Expressing that as a distinct extractor means a write handler states the
//! requirement in its signature and cannot silently skip it.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use chrono::Utc;

use super::model::Reader;
use crate::crypto::hash_token;
use crate::error::AppError;
use crate::state::AppState;

/// Any authenticated, active reader.
pub struct AuthenticatedReader(pub Reader);

/// An authenticated reader who has confirmed their email address.
pub struct VerifiedReader(pub Reader);

impl AuthenticatedReader {
    pub fn into_inner(self) -> Reader {
        self.0
    }
}

impl VerifiedReader {
    pub fn into_inner(self) -> Reader {
        self.0
    }
}

/// Pulls the bearer token out of the Authorization header.
///
/// The scheme match is case-insensitive because RFC 7235 says the scheme is a
/// case-insensitive token, and mobile HTTP clients do not agree on casing.
fn bearer_token(parts: &Parts) -> Option<String> {
    let raw = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = raw.split_once(' ')?;

    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

async fn load_reader(parts: &mut Parts, state: &AppState) -> Result<Reader, AppError> {
    let raw_token = bearer_token(parts).ok_or(AppError::Unauthorized)?;
    let token_hash = hash_token(&raw_token);

    // Session, expiry, and account status resolve in one query, so there is
    // no window where a suspended reader's existing token still works.
    sqlx::query_as::<_, Reader>(
        r#"
        SELECT r.id, r.email, r.email_verified, r.password_hash, r.is_active,
               r.display_name, r.created_at, r.updated_at
        FROM reader_sessions s
        JOIN readers r ON r.id = s.reader_id
        WHERE s.token_hash = $1
          AND s.expires_at > $2
          AND r.is_active
        "#,
    )
    .bind(&token_hash)
    .bind(Utc::now())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::Unauthorized)
}

impl FromRequestParts<AppState> for AuthenticatedReader {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(AuthenticatedReader(load_reader(parts, state).await?))
    }
}

impl FromRequestParts<AppState> for VerifiedReader {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let reader = load_reader(parts, state).await?;

        if !reader.email_verified {
            // 403 rather than 401: the credentials are fine, the account just
            // is not permitted to do this yet. A distinct message so the app
            // can prompt "resend verification" instead of a login screen.
            return Err(AppError::EmailNotVerified);
        }

        Ok(VerifiedReader(reader))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn parts_with_auth(value: &str) -> Parts {
        let req = Request::builder()
            .header(AUTHORIZATION, value)
            .body(())
            .unwrap();
        req.into_parts().0
    }

    #[test]
    fn extracts_a_well_formed_bearer_token() {
        let parts = parts_with_auth("Bearer abc123");
        assert_eq!(bearer_token(&parts).as_deref(), Some("abc123"));
    }

    #[test]
    fn scheme_match_is_case_insensitive() {
        assert_eq!(
            bearer_token(&parts_with_auth("bearer abc123")).as_deref(),
            Some("abc123")
        );
        assert_eq!(
            bearer_token(&parts_with_auth("BEARER abc123")).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn rejects_other_schemes_and_malformed_headers() {
        assert!(bearer_token(&parts_with_auth("Basic abc123")).is_none());
        assert!(bearer_token(&parts_with_auth("abc123")).is_none());
        assert!(bearer_token(&parts_with_auth("Bearer")).is_none());
        assert!(bearer_token(&parts_with_auth("Bearer   ")).is_none());
    }

    #[test]
    fn missing_header_yields_nothing() {
        let req = Request::builder().body(()).unwrap();
        assert!(bearer_token(&req.into_parts().0).is_none());
    }
}
