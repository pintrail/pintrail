//! The admin panel: server-rendered HTML for a handful of people.
//!
//! Comment moderation, trail takedown, and author management (docs/DESIGN.md
//! 2.7). Cookie-authenticated and admin-only, with CSRF protection the JSON
//! API does not need -- see `csrf`.

pub mod csrf;
pub mod routes;

pub use routes::{redirect_unauthenticated, router};
