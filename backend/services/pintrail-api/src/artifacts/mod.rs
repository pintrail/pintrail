//! Artifacts: the nestable things explorers discover.
//!
//! Author-write (editor and above), read for any authenticated caller. Only
//! this module touches the `artifacts` table (docs/DESIGN.md 2.3).

pub mod model;
pub mod routes;

pub use routes::router;
