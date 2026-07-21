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

/// Vendored third-party assets are versioned in their filename and never
/// change under a given URL, so they can be cached hard.
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
