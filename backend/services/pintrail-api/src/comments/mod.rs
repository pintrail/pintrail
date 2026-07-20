//! Comments: written by readers, moderated by admins.
//!
//! Only this module touches the `comments` table (docs/DESIGN.md 2.3).

pub mod model;
pub mod routes;

pub use routes::router;
