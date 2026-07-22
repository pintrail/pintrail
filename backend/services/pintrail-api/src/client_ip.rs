//! Determining who a request came from, for rate limiting.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use crate::error::AppError;
use crate::state::AppState;

/// The client address a rate limit is keyed on.
///
/// Behind a reverse proxy the socket address is the proxy's, so every request
/// would share one bucket. `X-Forwarded-For` carries the real client — but it
/// is a request header, and anyone talking to the API directly can put
/// anything in it, which would make every per-IP limit trivially bypassable.
///
/// So it is trusted only when `TRUST_PROXY_HEADERS=true`, which a deployment
/// sets precisely because it knows a proxy is stripping and rewriting the
/// header (docs/DESIGN.md §2.1 puts Caddy in front).
///
/// **The default is false**, and the failure mode of getting that wrong is
/// deliberate. Unset behind a proxy, every client shares the proxy's bucket
/// and legitimate users start seeing 429s — loud, immediate, and easy to
/// diagnose. Defaulting to true and being exposed directly would instead make
/// every limit silently spoofable, and nothing would look wrong at all.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if state.settings.trust_proxy_headers {
            if let Some(forwarded) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
            {
                // Leftmost entry is the original client; the rest are proxies.
                if let Some(first) = forwarded.split(',').next() {
                    let candidate = first.trim();
                    if !candidate.is_empty() {
                        return Ok(ClientIp(candidate.to_string()));
                    }
                }
            }
        }

        let addr = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            // Only reachable if the server was built without connect info.
            // Falling back to a shared key keeps the limit conservative
            // rather than disabling it.
            .unwrap_or_else(|| "unknown".to_string());

        Ok(ClientIp(addr))
    }
}
