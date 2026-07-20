//! Session handling for the author tier.
//!
//! The password and token primitives live in [`crate::crypto`], shared with
//! the reader tier; this module holds only what is specific to cookie-based
//! author sessions.

use chrono::Duration;

pub use crate::crypto::{
    hash_password, hash_token as hash_session_token, verify_dummy_password, verify_password,
    OpaqueToken as SessionToken,
};

/// Name of the session cookie, prefixed so it is obvious in a browser jar
/// which tier it belongs to.
pub const SESSION_COOKIE: &str = "pintrail_author_session";

/// Long enough not to interrupt an author mid-edit, short enough that a
/// stolen cookie is not indefinite access. Authors have elevated privileges,
/// so this is far shorter than the reader tier's token lifetime.
pub const SESSION_TTL_HOURS: i64 = 12;

pub fn generate_session_token() -> SessionToken {
    crate::crypto::generate_token(Duration::hours(SESSION_TTL_HOURS))
}
