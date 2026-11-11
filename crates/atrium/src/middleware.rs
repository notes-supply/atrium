//! What runs around every route: access, host names, headers, `ETag`s and the request log.

use std::sync::Arc;
use std::time::Instant;

use atrium_core::{Access, Viewer};
use atrium_ui::Shell;
use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, HOST, IF_NONE_MATCH,
    REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::uri::Authority;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Router};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::log;

/// The policy for every response. `connect-src` lets the module loader fetch its `.wasm`.
pub const CSP: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; \
    form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

/// Guards every route in `routes` with `access`. A viewer it does not admit gets the same 404
/// as an address with nothing at it, so a listener does not tell which private routes exist.
pub fn guard(routes: Router, access: Access) -> Router {
    routes.layer(from_fn_with_state(access, admit))
}

/// The access check: the one place that decides whether a viewer may see a route.
async fn admit(
    State(access): State<Access>,
    Extension(viewer): Extension<Viewer>,
    shell: Shell,
    request: Request,
    next: Next,
) -> Response {
    if access.admits(viewer) {
        next.run(request).await
    } else {
        shell.not_found()
    }
}

/// Refuses a request whose `Host` is not one of `allowed`, so a page cannot be reached
/// through a name that someone else controls.
pub async fn known_host(
    State(allowed): State<Arc<[String]>>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|host| host.to_str().ok())
        .and_then(|host| host.parse::<Authority>().ok());
    match host {
        Some(host)
            if allowed
                .iter()
                .any(|name| name.eq_ignore_ascii_case(host.host())) =>
        {
            next.run(request).await
        }
        _ => (StatusCode::MISDIRECTED_REQUEST, "unknown host\n").into_response(),
    }
}

/// The security headers, and `Cache-Control: no-store` on every response that has not set
/// its own, which is every page.
pub async fn headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers
        .entry(CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response
}

/// A strong `ETag` over each rendered page, and 304 when the client has it already.
pub async fn etag(request: Request, next: Next) -> Response {
    let known = request.headers().get(IF_NONE_MATCH).cloned();
    let response = next.run(request).await;
    let html = response
        .headers()
        .get(CONTENT_TYPE)
        .is_some_and(|kind| kind.as_bytes().starts_with(b"text/html"));
    if !html || !response.status().is_success() {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let tag = format!("\"{}\"", hex(&Sha256::digest(&bytes)[..16]));
    let Ok(tag) = HeaderValue::from_str(&tag) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if known.is_some_and(|known| matches(&known, &tag)) {
        let mut headers = HeaderMap::new();
        headers.insert(ETAG, tag);
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    parts.headers.insert(ETAG, tag);
    Response::from_parts(parts, Body::from(bytes))
}

/// Whether an `If-None-Match` value names `tag`. The comparison is weak, as RFC 9110 asks.
fn matches(known: &HeaderValue, tag: &HeaderValue) -> bool {
    let Ok(known) = known.to_str() else {
        return false;
    };
    known.split(',').map(str::trim).any(|candidate| {
        candidate == "*" || candidate.trim_start_matches("W/").as_bytes() == tag.as_bytes()
    })
}

/// Lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// One line per request. The query string is left out, since it can hold search words.
pub async fn request_log(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    log::emit(
        "request",
        json!({
            "method": method,
            "path": path,
            "status": response.status().as_u16(),
            "ms": start.elapsed().as_millis(),
        }),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    use atrium_core::{Local, Prefs, Visit};
    use atrium_ui::{Chrome, Footnote, Module};
    use axum::routing::get;
    use jiff::Timestamp;
    use jiff::tz::TimeZone;
    use tower::ServiceExt;

    fn site(access: Access, viewer: Viewer) -> Router {
        let chrome = Arc::new(Chrome {
            nav: Vec::new(),
            search: None,
            stylesheet: String::new(),
            fonts: Vec::new(),
            loader: String::new(),
            palette: Module {
                glue: String::new(),
                wasm: String::new(),
            },
            footnote: Footnote(Arc::new(|_| None)),
        });
        let visit = Visit {
            local: Local::new(Timestamp::UNIX_EPOCH, TimeZone::UTC),
            last: None,
            prefs: Prefs::default(),
        };
        guard(Router::new().route("/", get(|| async { "inside" })), access)
            .layer(Extension(chrome))
            .layer(Extension(visit))
            .layer(Extension(viewer))
    }

    async fn status(access: Access, viewer: Viewer) -> StatusCode {
        let request = Request::get("/").body(Body::empty()).unwrap();
        site(access, viewer)
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn a_private_route_refuses_an_anonymous_viewer() {
        assert_eq!(
            status(Access::Private, Viewer::Anonymous).await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_private_route_answers_the_tailnet() {
        assert_eq!(
            status(Access::Private, Viewer::Tailnet).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn a_public_route_answers_an_anonymous_viewer() {
        assert_eq!(
            status(Access::Public, Viewer::Anonymous).await,
            StatusCode::OK
        );
    }

    #[test]
    fn if_none_match_lists_and_weak_tags_match() {
        let tag = HeaderValue::from_static("\"abc\"");
        for known in ["\"abc\"", "W/\"abc\"", "\"x\", \"abc\"", "*"] {
            assert!(matches(&HeaderValue::from_static(known), &tag), "{known}");
        }
        assert!(!matches(&HeaderValue::from_static("\"abd\""), &tag));
    }
}
