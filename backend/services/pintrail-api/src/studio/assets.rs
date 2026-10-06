//! Static front-end assets, compiled into the binary.
//!
//! htmx and Leaflet are vendored rather than pulled from a CDN, matching the
//! rest of the panel: the tool keeps working with no outbound dependency for
//! its own code. (Map *tiles* are the one exception, and are inherent to
//! having a map at all.) `include_str!` bakes them into the binary, so there
//! is nothing to copy at deploy time and no path that can go missing.

use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

const HTMX_JS: &str = include_str!("assets/htmx.min.js");
const LEAFLET_JS: &str = include_str!("assets/leaflet.js");
const LEAFLET_CSS: &str = include_str!("assets/leaflet.css");
const STUDIO_JS: &str = include_str!("assets/studio.js");

/// A fingerprint of studio.js, appended to its URL as `?v=...`.
///
/// Every asset here is cached as immutable for a year. That is right for the
/// vendored htmx and Leaflet, which never change, but studio.js changes with
/// the Studio itself: under a fixed URL, a browser that loaded the old script
/// keeps running it after a deploy, against templates that expect the new
/// one. Putting the content hash in the URL gives each version its own
/// address, so a deploy is picked up on the next page load and the long
/// cache stays safe.
pub fn studio_js_version() -> &'static str {
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| hex::encode(&Sha256::digest(STUDIO_JS.as_bytes())[..6]))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/studio/assets/htmx.min.js", get(htmx))
        .route("/studio/assets/leaflet.js", get(leaflet_js))
        .route("/studio/assets/leaflet.css", get(leaflet_css))
        .route("/studio/assets/studio.js", get(studio_js))
}

async fn studio_js() -> Response {
    immutable("application/javascript; charset=utf-8", STUDIO_JS)
}

/// Cached hard. The vendored third-party files never change; studio.js does,
/// so its URL carries a content hash (see `studio_js_version`).
fn immutable(content_type: &str, body: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        body,
    )
        .into_response()
}

async fn htmx() -> Response {
    immutable("application/javascript; charset=utf-8", HTMX_JS)
}

async fn leaflet_js() -> Response {
    immutable("application/javascript; charset=utf-8", LEAFLET_JS)
}

async fn leaflet_css() -> Response {
    immutable("text/css; charset=utf-8", LEAFLET_CSS)
}
