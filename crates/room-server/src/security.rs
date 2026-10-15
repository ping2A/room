//! HTTP security headers (docs/22 §5). TLS terminates at a reverse proxy in M2
//! (see `deploy/Caddyfile` and ADR 0002).

use axum::http::{HeaderName, HeaderValue, Request, header};
use axum::middleware::Next;
use axum::response::Response;

/// CSP for API / lobby / admin: scripts are external files; styles may be
/// set from JS (admin sparkline heights, etc.).
const CSP_STRICT: &str = "default-src 'self'; \
connect-src 'self' ws: wss:; \
img-src 'self' data:; \
media-src 'self'; \
object-src 'none'; \
base-uri 'self'; \
script-src 'self'; \
style-src 'self' 'unsafe-inline'";

/// `room-web` under `/play/` still uses inline `<script>` / `<style>` and Wasm.
/// `blob:` in script-src is required for AudioWorklet modules created via
/// `URL.createObjectURL` (game mixer + Lightcall voice capture/playback).
const CSP_PLAY: &str = "default-src 'self'; \
script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' blob:; \
style-src 'self' 'unsafe-inline'; \
connect-src 'self' ws: wss: blob:; \
img-src 'self' data: blob:; \
media-src 'self' blob:; \
worker-src 'self' blob:; \
child-src 'self' blob:; \
object-src 'none'; \
base-uri 'self'";

/// Attach baseline security headers to every response.
pub async fn security_headers_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    set(headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(headers, header::REFERRER_POLICY, "no-referrer");
    set(
        headers,
        HeaderName::from_static("permissions-policy"),
        "camera=(self), microphone=(self), geolocation=(), clipboard-write=(self)",
    );
    set(headers, HeaderName::from_static("x-frame-options"), "DENY");
    let csp = if is_play_path(&path) {
        CSP_PLAY
    } else {
        CSP_STRICT
    };
    set(headers, header::CONTENT_SECURITY_POLICY, csp);
    res
}

fn is_play_path(path: &str) -> bool {
    path == "/play" || path.starts_with("/play/")
}

fn set(headers: &mut axum::http::HeaderMap, name: HeaderName, value: &'static str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

#[cfg(test)]
mod tests {
    use super::is_play_path;

    #[test]
    fn play_paths() {
        assert!(is_play_path("/play"));
        assert!(is_play_path("/play/"));
        assert!(is_play_path("/play/index.html"));
        assert!(is_play_path("/play/room_web.js"));
        assert!(!is_play_path("/"));
        assert!(!is_play_path("/lobby"));
        assert!(!is_play_path("/admin"));
        assert!(!is_play_path("/api/rooms"));
    }
}
