//! The author tier: a small, trusted, admin-provisioned group who create and
//! edit artifacts, authenticating with an httpOnly cookie session.
//!
//! Only this module touches the `authors` and `author_sessions` tables --
//! the convention that replaces the process boundary the earlier Python
//! design used (docs/DESIGN.md 2.3).

pub mod auth;
pub mod extractors;
pub mod model;
pub mod routes;

pub use routes::router;
