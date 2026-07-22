//! Attachments: media hung off an artifact.
//!
//! Bytes never pass through this process. Clients PUT originals directly to
//! object storage with a presigned URL and read back the same way
//! (docs/DESIGN.md 2.5). Only this module touches the `attachments` table.

pub mod mime;
pub mod model;
pub mod routes;

pub use routes::{list_for_artifact, router};
