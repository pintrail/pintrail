//! Token lifetimes and helpers for the reader tier.

use chrono::Duration;
use uuid::Uuid;

use super::model::ReaderTokenPurpose;
use crate::crypto::{generate_token, OpaqueToken};
use crate::error::AppResult;

/// Mobile sessions are long-lived: an explorer should not be logged out
/// between campus visits. Far longer than the author tier's 12 hours, which
/// is the right trade for an account that cannot edit content.
pub const SESSION_TTL_DAYS: i64 = 90;

/// Long enough to survive a message sitting in a spam folder overnight.
pub const VERIFY_TTL_HOURS: i64 = 24;

/// Deliberately short. A reset link is a live credential, and unlike a
/// verification link it grants account takeover.
pub const RESET_TTL_HOURS: i64 = 1;

pub fn generate_session_token() -> OpaqueToken {
    generate_token(Duration::days(SESSION_TTL_DAYS))
}

/// Issues a single-use token for `purpose`, invalidating any outstanding
/// tokens of the same purpose first.
///
/// Superseding matters: without it, every "resend verification" click leaves
/// another live token, and each one is an independent chance for an old
/// message in an inbox to still work.
/// Takes a connection rather than a generic executor because it runs two
/// statements, and callers always have a transaction in hand -- superseding
/// and issuing must not be separable.
pub async fn issue_verification_token(
    conn: &mut sqlx::PgConnection,
    reader_id: Uuid,
    purpose: ReaderTokenPurpose,
) -> AppResult<OpaqueToken> {
    let ttl = match purpose {
        ReaderTokenPurpose::VerifyEmail => Duration::hours(VERIFY_TTL_HOURS),
        ReaderTokenPurpose::ResetPassword => Duration::hours(RESET_TTL_HOURS),
    };

    sqlx::query(
        r#"
        UPDATE reader_verification_tokens
        SET consumed_at = now()
        WHERE reader_id = $1 AND purpose = $2 AND consumed_at IS NULL
        "#,
    )
    .bind(reader_id)
    .bind(purpose)
    .execute(&mut *conn)
    .await?;

    let token = generate_token(ttl);

    sqlx::query(
        r#"
        INSERT INTO reader_verification_tokens (reader_id, purpose, token_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(reader_id)
    .bind(purpose)
    .bind(&token.hash)
    .bind(token.expires_at)
    .execute(&mut *conn)
    .await?;

    Ok(token)
}

/// Minimum password policy for the public tier.
///
/// Length only, no composition rules: NIST SP 800-63B advises against
/// character-class requirements, which push users toward predictable
/// substitutions without adding real entropy.
pub fn validate_password(password: &str) -> AppResult<()> {
    let length = password.chars().count();

    if length < 10 {
        return Err(crate::error::AppError::BadRequest(
            "password must be at least 10 characters".into(),
        ));
    }
    if length > 512 {
        // Bounded so a huge input cannot turn argon2 into a CPU exhaustion
        // vector on an unauthenticated endpoint.
        return Err(crate::error::AppError::BadRequest(
            "password must be at most 512 characters".into(),
        ));
    }

    Ok(())
}

/// Loose sanity check; the real validation is whether mail arrives.
pub fn looks_like_email(candidate: &str) -> bool {
    match candidate.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !candidate.contains(char::is_whitespace)
                && candidate.len() <= 320
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_length_bounds_are_enforced() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("just right!").is_ok());
        assert!(validate_password(&"x".repeat(513)).is_err());
        assert!(validate_password(&"x".repeat(512)).is_ok());
    }

    #[test]
    fn email_shape_check() {
        assert!(looks_like_email("explorer@umass.edu"));
        assert!(!looks_like_email("no-at-sign"));
        assert!(!looks_like_email("@umass.edu"));
        assert!(!looks_like_email("a@umass"));
        assert!(!looks_like_email("a b@umass.edu"));
    }

    #[test]
    fn reset_tokens_are_shorter_lived_than_verification_tokens() {
        // A reset link grants account takeover; a verification link does not.
        assert!(RESET_TTL_HOURS < VERIFY_TTL_HOURS);
    }
}
