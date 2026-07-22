//! The reader tier: the general public, self-service signup, authenticating
//! with long-lived bearer tokens from the mobile app.
//!
//! Only this module touches the `readers`, `reader_sessions`, and
//! `reader_verification_tokens` tables (docs/DESIGN.md 2.3).

pub mod auth;
pub mod extractors;
pub mod model;
pub mod routes;

pub use routes::router;
