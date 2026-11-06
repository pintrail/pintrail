//! The admin panel: server-rendered HTML for a handful of people.
//!
//! Comment moderation, trail takedown, author management (docs/DESIGN.md
//! 2.7), and the System tab: build info and the search indexer (#35). Cookie-authenticated and admin-only, with CSRF protection the JSON
//! API does not need -- see `csrf`.

pub mod author_edit;
pub mod csrf;
pub mod routes;
pub mod system;

pub use routes::{redirect_unauthenticated, router};
