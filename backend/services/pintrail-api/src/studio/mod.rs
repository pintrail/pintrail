//! The Studio: a browser-based tool for authoring and previewing artifacts,
//! for use while the mobile app is built.
//!
//! Server-rendered HTML with htmx for interactivity and Leaflet for the map,
//! both vendored into the binary (only map tiles are remote). Cookie-
//! authenticated on the author tier: viewers browse, editors and admins write.
//! Reuses admin::csrf and the artifacts coordinate-resolution query.

pub mod assets;
pub mod routes;

pub use routes::{redirect_unauthenticated, router};
