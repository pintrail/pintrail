//! Password hashing and opaque-token handling, shared by both identity tiers.
//!
//! Authors and readers authenticate differently -- cookie versus bearer -- but
//! the underlying primitives are identical, so they live here rather than
//! being implemented twice with a chance of drifting apart.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Hashes a password with argon2id using the crate defaults (m=19456 KiB,
/// t=2, p=1 -- the OWASP baseline).
///
/// Returns a PHC string embedding algorithm, parameters, and salt, which is
/// what makes raising the cost later possible without invalidating existing
/// hashes.
pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hashing failed: {e}")))
}

/// Verifies a password against a stored PHC hash.
///
/// A malformed stored hash returns `false` rather than erroring: that means
/// one row is corrupt, which should fail that login rather than 500 the
/// endpoint.
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

/// A hash of a random throwaway password, used to equalize login timing when
/// the account does not exist. Without it, "unknown email" returns measurably
/// faster than "wrong password", which enumerates valid accounts.
///
/// Computed at first use rather than hardcoded: a hardcoded PHC literal that
/// failed to parse would make `verify_password` bail early, silently turning
/// this defense into a no-op.
static DUMMY_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn dummy_hash() -> &'static str {
    DUMMY_HASH.get_or_init(|| {
        let mut filler = [0u8; 32];
        OsRng.fill_bytes(&mut filler);
        hash_password(&hex::encode(filler)).expect("hashing a generated password cannot fail")
    })
}

/// Burns roughly the same CPU a real verification would.
pub fn verify_dummy_password(password: &str) {
    let _ = verify_password(password, dummy_hash());
}

/// A freshly minted opaque token. The raw value goes to its recipient exactly
/// once; only the hash is persisted.
pub struct OpaqueToken {
    pub raw: String,
    pub hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Generates a 256-bit token from the OS CSPRNG, valid for `ttl`.
///
/// Only the SHA-256 is stored, so a database leak hands out nothing usable.
/// SHA-256 rather than argon2 is deliberate: the input is already
/// high-entropy random, so there is nothing for a slow hash to defend
/// against, and these are verified on every request.
pub fn generate_token(ttl: Duration) -> OpaqueToken {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);

    let raw = hex::encode(bytes);
    let hash = hash_token(&raw);

    OpaqueToken {
        raw,
        hash,
        expires_at: Utc::now() + ttl,
    }
}

pub fn hash_token(raw: &str) -> String {
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
        let a = hash_password("hunter2").unwrap();
        let b = hash_password("hunter2").unwrap();
        assert_ne!(a, b, "identical hashes would mean the salt is not random");
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
        assert!(
            PasswordHash::new(dummy_hash()).is_ok(),
            "dummy hash must parse or the login timing defense is a no-op"
        );
    }

    #[test]
    fn tokens_are_unique_and_stored_hashed() {
        let a = generate_token(Duration::hours(1));
        let b = generate_token(Duration::hours(1));

        assert_ne!(a.raw, b.raw);
        assert_eq!(a.raw.len(), 64, "32 bytes hex-encoded");
        assert_ne!(a.raw, a.hash, "the raw token must never equal what is stored");
        assert_eq!(a.hash, hash_token(&a.raw));
        assert!(a.expires_at > Utc::now());
    }
}
