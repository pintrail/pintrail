//! Trails: ordered sequences of artifacts.
//!
//! Owned by either an author (curated) or a reader (user-built) through
//! `owner_type` + `owner_id` -- the same table for both, since they are the
//! same concept pointed at different artifacts (docs/DESIGN.md 1.4).
//! Only this module touches `trails` and `trail_stops`.

pub mod model;
pub mod routes;

pub use routes::router;
