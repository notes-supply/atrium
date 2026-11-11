//! A stand-in for Mneme that serves Mneme's own example responses, so tests need no Mneme
//! built, or the demo's invented wiki. In the examples only the page `4226c96dd1386f3a` has a
//! text, history and diff; every other page is unknown.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, Request, State};
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
    /// Answers with the design's invented wiki.
    Demo,
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

fn json(body: String) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

fn missing() -> Response {
    let body = r#"{"error":"no such page"}"#.to_owned();
    (StatusCode::NOT_FOUND, json(body)).into_response()
}

/// Which part of a listing to answer, as Mneme reads it.
#[derive(Deserialize)]
struct Paging {
    #[serde(default)]
    offset: usize,
    #[serde(default = "fifty")]
    limit: usize,
}

const fn fifty() -> usize {
    50
}

/// An example listing cut to the part `paging` asks for. `total` stays the whole count.
fn paged(example: &str, field: &str, paging: &Paging) -> Response {
    let mut body: Value = serde_json::from_str(example).unwrap_or_default();
    if let Some(items) = body[field].as_array_mut() {
        let kept: Vec<Value> = items
            .drain(..)
            .skip(paging.offset)
            .take(paging.limit)
            .collect();
        *items = kept;
    }
    json(body.to_string())
}

#[derive(Deserialize)]
struct Search {
    q: String,
}

async fn search(Query(search): Query<Search>, Query(paging): Query<Paging>) -> Response {
    if search.q.matches('"').count() % 2 == 1 {
        let error = r#"{"error":"a search with an open quote cannot be read"}"#.to_owned();
        return (StatusCode::BAD_REQUEST, json(error)).into_response();
    }
    paged(include_str!("../fixtures/search.json"), "hits", &paging)
}

async fn machines() -> Response {
    json(include_str!("../fixtures/machines.json").to_owned())
}

async fn pages(Query(paging): Query<Paging>) -> Response {
    paged(include_str!("../fixtures/pages.json"), "pages", &paging)
}

async fn changes(Query(paging): Query<Paging>) -> Response {
    paged(include_str!("../fixtures/changes.json"), "events", &paging)
}

async fn page(Path(id): Path<String>) -> Response {
    of_alpha(&id, || {
        json(include_str!("../fixtures/page.json").to_owned())
    })
}

async fn versions(Path(id): Path<String>, Query(paging): Query<Paging>) -> Response {
    of_alpha(&id, || {
        paged(include_str!("../fixtures/versions.json"), "events", &paging)
    })
}

async fn version(Path((id, _commit)): Path<(String, String)>) -> Response {
    of_alpha(&id, || {
        json(include_str!("../fixtures/version.json").to_owned())
    })
}

async fn diff(Path(id): Path<String>) -> Response {
    of_alpha(&id, || {
        json(include_str!("../fixtures/diff.json").to_owned())
    })
}

fn of_alpha(id: &str, answer: impl FnOnce() -> Response) -> Response {
    if id == ALPHA { answer() } else { missing() }
}

fn routes() -> Router {
    Router::new()
        .route("/v1/pages", get(pages))
        .route("/v1/pages/{id}", get(page))
        .route("/v1/pages/{id}/versions", get(versions))
        .route("/v1/pages/{id}/versions/{commit}", get(version))
        .route("/v1/pages/{id}/diff", get(diff))
        .route("/v1/changes", get(changes))
        .route("/v1/search", get(search))
        .route("/v1/machines", get(machines))
        .route(
            "/v1/status",
            get(|| async { json(include_str!("../fixtures/status.json").to_owned()) }),
        )
        .route(
            "/v1/questions",
            get(|| async { json(include_str!("../fixtures/questions.json").to_owned()) }),
        )
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
        Stub::Demo => crate::demo::routes(),
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
