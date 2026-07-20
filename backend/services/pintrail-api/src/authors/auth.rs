//! Password hashing and session token handling for the author tier.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Name of the session cookie. Prefixed so it is obvious in a browser jar
/// which tier it belongs to once the reader tier exists.
pub const SESSION_COOKIE: &str = "pintrail_author_session";

/// Sessions are long enough not to annoy an author mid-edit, short enough that
/// a stolen cookie is not indefinite access.
pub const SESSION_TTL_HOURS: i64 = 12;

/// A hash of a random throwaway password, used to equalize login timing when
/// the email does not exist. Without it, "unknown email" returns measurably
/// faster than "wrong password", which enumerates valid accounts.
///
/// Computed once at first use rather than hardcoded: a hardcoded PHC literal
/// that failed to parse would make `verify_password` bail early, silently
/// turning this defense into a no-op.
static DUMMY_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        let mut filler = [0u8; 32];
        OsRng.fill_bytes(&mut filler);
        hash_password(&hex::encode(filler)).expect("hashing a generated password cannot fail")
    })
}

/// Hashes a password with argon2id using the crate's default parameters
/// (m=19456 KiB, t=2, p=1 -- the OWASP-recommended baseline).
///
/// Returns a PHC string, which embeds the algorithm, parameters, and salt.
/// That is what makes raising the cost parameters later possible without
/// invalidating existing hashes.
pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hashing failed: {e}")))
}

/// Verifies a password against a stored PHC hash.
///
/// A malformed stored hash returns `false` rather than an error: it means that
/// one row is corrupt, which should fail that login, not 500 the endpoint.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(err) => {
            tracing::error!(error = %err, "stored password hash is malformed");
            false
        }
    }
}

/// Burns roughly the same CPU as a real verification, so a caller cannot tell
/// "no such account" from "wrong password" by timing the response.
pub fn verify_dummy_password(password: &str) {
    let _ = verify_password(password, dummy_hash());
}

/// A freshly minted session token: the raw value goes to the client exactly
/// once, only its hash is persisted.
pub struct SessionToken {
    pub raw: String,
    pub hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Generates a 256-bit session token from the OS CSPRNG.
///
/// Only the SHA-256 of the token is stored, so a database leak does not hand
/// out live sessions. SHA-256 (not argon2) is right here specifically because
/// the input is already high-entropy random -- it is not guessable, so there
/// is nothing for a slow hash to defend against, and sessions are verified on
/// every request.
pub fn generate_session_token() -> SessionToken {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);

    let raw = hex::encode(bytes);
    let hash = hash_session_token(&raw);

    SessionToken {
        raw,
        hash,
        expires_at: Utc::now() + Duration::hours(SESSION_TTL_HOURS),
    }
}

pub fn hash_session_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_round_trips() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn same_password_gets_distinct_hashes() {
        // Distinct salts; identical hashes would mean the salt is not random.
        let a = hash_password("hunter2").unwrap();
        let b = hash_password("hunter2").unwrap();
        assert_ne!(a, b);
        assert!(verify_password("hunter2", &a));
        assert!(verify_password("hunter2", &b));
    }

    #[test]
    fn hash_is_phc_argon2id() {
        let hash = hash_password("x").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$"), "got {hash}");
    }

    #[test]
    fn malformed_stored_hash_fails_closed() {
        assert!(!verify_password("anything", "not-a-phc-string"));
        assert!(!verify_password("anything", ""));
    }

    #[test]
    fn dummy_hash_is_valid_so_timing_defense_actually_runs() {
        // If the dummy hash were malformed, verify_password would bail early
        // on a parse error and the timing equalization would do nothing.
        assert!(
            PasswordHash::new(dummy_hash()).is_ok(),
            "dummy hash must parse or the login timing defense is a no-op"
        );
    }

    #[test]
    fn session_tokens_are_unique_and_hashed() {
        let a = generate_session_token();
        let b = generate_session_token();

        assert_ne!(a.raw, b.raw);
        assert_eq!(a.raw.len(), 64, "32 bytes hex-encoded");
        assert_ne!(a.raw, a.hash, "the raw token must never equal what is stored");
        assert_eq!(a.hash, hash_session_token(&a.raw));
        assert!(a.expires_at > Utc::now());
    }
}
