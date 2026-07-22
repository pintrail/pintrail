//! CSRF protection for the admin panel's HTML forms.
//!
//! The JSON API never needed this. A cross-site `fetch` cannot read a JSON
//! response without CORS, and the reader tier authenticates with a bearer
//! token a browser will not attach automatically. An HTML form is different:
//! any page anywhere can POST to this one, and the browser attaches the
//! author's session cookie.
//!
//! `SameSite=Lax` on that cookie already blocks the cross-site POST, and is
//! the primary defense. This is the second layer, for the case where that
//! attribute is lost — a proxy rewriting cookies, an older browser, or someone
//! relaxing it to `None` to fix an unrelated integration.
//!
//! The token is derived from the session token rather than stored, so it needs
//! no schema change and no server-side state: an attacker cannot compute it
//! without the session cookie, which is `HttpOnly` and unreadable from script.

use sha2::{Digest, Sha256};

/// Domain separator, so this hash can never collide with the session-token
/// hash stored in `author_sessions`.
const DOMAIN: &str = "pintrail-admin-csrf-v1";

pub fn token_for_session(session_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN.as_bytes());
    hasher.update(session_token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time comparison.
///
/// A short-circuiting `==` leaks how many leading bytes matched, which is
/// enough to reconstruct the token one byte at a time given enough attempts.
pub fn verify(session_token: &str, submitted: &str) -> bool {
    let expected = token_for_session(session_token);

    if expected.len() != submitted.len() {
        return false;
    }

    expected
        .bytes()
        .zip(submitted.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_its_own_session() {
        let token = token_for_session("session-abc");
        assert!(verify("session-abc", &token));
    }

    #[test]
    fn token_from_another_session_is_rejected() {
        let token = token_for_session("session-abc");
        assert!(!verify("session-xyz", &token));
    }

    #[test]
    fn rejects_empty_and_malformed_submissions() {
        let session = "session-abc";
        assert!(!verify(session, ""));
        assert!(!verify(session, "nope"));
        assert!(!verify(session, &token_for_session(session)[..10]));
    }

    #[test]
    fn is_not_the_session_token_itself() {
        // If these were equal, embedding the CSRF token in a page would be
        // handing out the session.
        let session = "session-abc";
        assert_ne!(token_for_session(session), session);
    }

    #[test]
    fn differs_from_the_stored_session_hash() {
        // The domain separator is what guarantees this; without it the value
        // in a form field would equal the value in the sessions table.
        let session = "session-abc";
        assert_ne!(token_for_session(session), crate::crypto::hash_token(session));
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(token_for_session("s"), token_for_session("s"));
    }
}
