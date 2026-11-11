//! A stand-in for Mneme that serves Mneme's own example responses, so tests need no Mneme
//! built. Only the page `4226c96dd1386f3a` has a text, history and diff; every other page is
//! unknown.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::Value;
use tokio::net::TcpListener;

/// How the stand-in behaves.
#[derive(Debug, Clone, Copy)]
pub enum Stub {
    /// Answers every route with its example.
    Fixtures,
    /// Answers only after this long.
    Slow(Duration),
    /// Nothing listens at its address.
    Down,
    /// Answers every route with 500.
    Failing,
}

/// A running stand-in.
pub struct Running {
    /// Its base URL.
    pub base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Running {
    /// A Mneme that runs elsewhere, at `base`. Its requests are not recorded.
    #[must_use]
    pub fn elsewhere(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            requests: Arc::default(),
        }
    }

    /// Every request so far, as path and query.
    #[must_use]
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

const ALPHA: &str = "4226c96dd1386f3a";

fn json(body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

fn missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":"no such page"}"#,
    )
        .into_response()
}

fn of_alpha(id: &str, body: &'static str) -> Response {
    if id == ALPHA { json(body) } else { missing() }
}

#[derive(Deserialize)]
struct Search {
    q: String,
}

async fn search(Query(search): Query<Search>) -> Response {
    if search.q.matches('"').count() % 2 == 1 {
        let error = r#"{"error":"a search with an open quote cannot be read"}"#;
        return (StatusCode::BAD_REQUEST, error).into_response();
    }
    json(include_str!("../fixtures/search.json"))
}

/// The example, with a field Mneme is about to add, which Atrium must ignore.
async fn machines() -> Response {
    let mut body: Value =
        serde_json::from_str(include_str!("../fixtures/machines.json")).unwrap_or_default();
    if let Some(machines) = body["machines"].as_array_mut() {
        for machine in machines {
            machine["skipped"] = Value::Array(Vec::new());
        }
    }
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn routes() -> Router {
    Router::new()
        .route(
            "/v1/pages",
            get(|| async { json(include_str!("../fixtures/pages.json")) }),
        )
        .route(
            "/v1/pages/{id}",
            get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    of_alpha(&id, include_str!("../fixtures/page.json"))
                },
            ),
        )
        .route(
            "/v1/pages/{id}/versions",
            get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    of_alpha(&id, include_str!("../fixtures/versions.json"))
                },
            ),
        )
        .route(
            "/v1/pages/{id}/versions/{commit}",
            get(
                |axum::extract::Path((id, _)): axum::extract::Path<(String, String)>| async move {
                    of_alpha(&id, include_str!("../fixtures/version.json"))
                },
            ),
        )
        .route(
            "/v1/pages/{id}/diff",
            get(
                |axum::extract::Path(id): axum::extract::Path<String>| async move {
                    of_alpha(&id, include_str!("../fixtures/diff.json"))
                },
            ),
        )
        .route(
            "/v1/changes",
            get(|| async { json(include_str!("../fixtures/changes.json")) }),
        )
        .route("/v1/search", get(search))
        .route("/v1/machines", get(machines))
}

async fn record(
    State(requests): State<Arc<Mutex<Vec<String>>>>,
    request: Request,
    next: Next,
) -> Response {
    let uri = request.uri();
    let line = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), ToString::to_string);
    requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(line);
    next.run(request).await
}

/// Starts a stand-in on a free local port.
///
/// # Panics
///
/// When no local port can be bound.
#[expect(
    clippy::expect_used,
    reason = "a test fixture that cannot start fails the test"
)]
pub async fn serve(stub: Stub) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binding a local port");
    let base = format!("http://{}", listener.local_addr().expect("a local address"));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = match stub {
        Stub::Fixtures => routes(),
        Stub::Slow(delay) => {
            routes().layer(middleware::from_fn(move |request, next: Next| async move {
                tokio::time::sleep(delay).await;
                next.run(request).await
            }))
        }
        Stub::Failing => Router::new().fallback(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        Stub::Down => {
            drop(listener);
            return Running { base, requests };
        }
    }
    .layer(middleware::from_fn_with_state(
        Arc::clone(&requests),
        record,
    ));
    tokio::spawn(async move { axum::serve(listener, app).await });
    Running { base, requests }
}
