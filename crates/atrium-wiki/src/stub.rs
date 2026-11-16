//! A stand-in for Mneme that serves Mneme's own example responses, so tests need no Mneme
//! built, or the demo's invented wiki. In the examples only the page `4226c96dd1386f3a` has a
//! text, history and diff; every other page is unknown.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
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
    /// Answers as a Mneme from before files did: every route of the examples but the
    /// files', which answer 404 as an unknown route.
    Older,
    /// Answers as [`Stub::Fixtures`], but every call for the files' changes fails.
    FileChangesFailing,
    /// Answers with a morning busier than one read of changes, [`BUSY_PAGES`] changes to
    /// pages and [`BUSY_FILES`] to files, several to the second and [`BUSY_BURST`] files in
    /// one second, more than two reads hold, and a few of each the day before.
    Busy,
}

/// Changes to pages in [`Stub::Busy`]'s busy morning, three to the second.
pub const BUSY_PAGES: usize = 600;
/// Changes to files in [`Stub::Busy`]'s busy morning: the newest [`BUSY_BURST`] in its
/// newest second, as one commit of a whole folder makes, then two to the second.
pub const BUSY_FILES: usize = 1200;
/// Changes to files in the busy morning's newest second.
pub const BUSY_BURST: usize = 1100;
/// Changes of each kind the day before.
pub const BUSY_EARLIER: usize = 5;
/// The newest change of the busy morning: 11 November 2026 at 09:30 in Stockholm.
const BUSY_NEWEST: i64 = 1_794_385_800;

/// The time of each busy change, newest first: the morning's, the first `burst` of them in
/// one second and the rest `per_second` to a second, then the day before's.
fn busy_times(
    morning: usize,
    burst: usize,
    per_second: usize,
) -> impl Iterator<Item = (usize, i64)> {
    let per_second = i64::try_from(per_second).unwrap_or(1);
    let at = |i: usize| i64::try_from(i).unwrap_or(0);
    (0..morning)
        .map(move |i| (i, BUSY_NEWEST - at(i.saturating_sub(burst)) / per_second))
        .chain((0..BUSY_EARLIER).map(move |i| (morning + i, BUSY_NEWEST - 86_400 - at(i) * 60)))
}

/// Which part of a busy listing to answer, as Mneme reads it.
#[derive(Deserialize)]
struct Window {
    #[serde(default)]
    offset: usize,
    #[serde(default = "fifty")]
    limit: usize,
    since: Option<i64>,
    before: Option<i64>,
}

/// The events at the times `times` gives, as `event` writes each, in the window asked for.
fn busy(
    window: &Window,
    times: impl Iterator<Item = (usize, i64)>,
    event: impl Fn(usize, i64) -> Value,
) -> Response {
    let kept: Vec<Value> = times
        .filter(|(_, time)| window.since.is_none_or(|since| *time >= since))
        .filter(|(_, time)| window.before.is_none_or(|before| *time < before))
        .map(|(i, time)| event(i, time))
        .collect();
    let total = kept.len();
    let events: Vec<Value> = kept
        .into_iter()
        .skip(window.offset)
        .take(window.limit.min(500))
        .collect();
    json(serde_json::json!({ "total": total, "events": events }).to_string())
}

async fn busy_changes(Query(window): Query<Window>) -> Response {
    busy(&window, busy_times(BUSY_PAGES, 0, 3), |i, time| {
        serde_json::json!({
            "commit": format!("{i:040x}"),
            "page": format!("{i:016x}"),
            "path": format!("busy/page-{i:04}.md"),
            "time": time,
            "author": "Writer",
            "kind": "edited",
            "version": true,
            "words": 10,
            "added": 1,
            "removed": 0,
        })
    })
}

async fn busy_file_changes(Query(window): Query<Window>) -> Response {
    busy(&window, busy_times(BUSY_FILES, BUSY_BURST, 2), |i, time| {
        serde_json::json!({
            "commit": format!("{:040x}", i + 10_000),
            "path": format!("busy/file-{i:04}.bin"),
            "time": time,
            "author": "Writer",
            "kind": "edited",
            "size": 10,
        })
    })
}

fn busy_routes() -> Router {
    Router::new()
        .route("/v1/changes", get(busy_changes))
        .route("/v1/changes/files", get(busy_file_changes))
        .route(
            "/v1/pages",
            get(|| async { json(r#"{"total":0,"pages":[]}"#.to_owned()) }),
        )
        .route(
            "/v1/status",
            get(|| async { json(include_str!("../fixtures/status.json").to_owned()) }),
        )
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

/// The live page at a path, from the listing's example, and the example's answer for the
/// old path it was made with.
async fn path(Path(asked): Path<String>) -> Response {
    if asked == "alpha.md" {
        return json(include_str!("../fixtures/path.json").to_owned());
    }
    let pages: Value =
        serde_json::from_str(include_str!("../fixtures/pages.json")).unwrap_or_default();
    pages["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|page| page["path"] == asked.as_str() && page["deleted"].is_null())
        .map_or_else(missing, |page| {
            json(
                serde_json::json!({ "id": page["id"], "path": page["path"], "current": true })
                    .to_string(),
            )
        })
}

async fn files(Query(paging): Query<Paging>) -> Response {
    paged(include_str!("../fixtures/files.json"), "files", &paging)
}

async fn file(Path(asked): Path<String>) -> Response {
    if asked == "brand/archive/Kit 2026.zip" {
        return json(include_str!("../fixtures/file.json").to_owned());
    }
    if asked == UNREADABLE {
        let commit = "a75f4a67d9148128bb1b4cc2f494d997cef810e7";
        let file = serde_json::json!({
            "path": UNREADABLE, "size": 26, "type": "text/plain; charset=utf-8",
            "created": 1_788_221_400, "updated": 1_788_221_400, "versions": 1,
            "author": "Writer", "commit": commit, "deleted": null,
        });
        let event = serde_json::json!({
            "commit": commit, "path": UNREADABLE, "time": 1_788_221_400,
            "author": "Writer", "kind": "created", "size": 26,
        });
        return json(
            serde_json::json!({ "file": file, "total": 1, "events": [event] }).to_string(),
        );
    }
    let body = r#"{"error":"no file is at this path, and none was deleted there"}"#.to_owned();
    (StatusCode::NOT_FOUND, json(body)).into_response()
}

async fn file_changes(Query(paging): Query<Paging>) -> Response {
    paged(
        include_str!("../fixtures/file-changes.json"),
        "events",
        &paging,
    )
}

/// A text file whose page Mneme answers but whose bytes it cannot send.
const UNREADABLE: &str = "notes/unreadable.txt";

/// Some bytes for each file of the examples, as long as its listing says, and for the
/// tests: `slow.bin` comes in pieces 400 ms apart, `busy.bin` and [`UNREADABLE`] are files
/// the server cannot send, and `bare.svg` comes without a policy.
async fn raw(
    method: Method,
    headers: HeaderMap,
    Path(path): Path<String>,
    Query(at): Query<At>,
) -> Response {
    match path.as_str() {
        "slow.bin" => {
            let pieces = futures_util::stream::unfold(0_u8, |sent| async move {
                if sent == 5 {
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
                Some((Ok::<_, std::io::Error>(vec![b'a' + sent; 1024]), sent + 1))
            });
            return (
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::CONTENT_LENGTH, "5120"),
                    (
                        header::CONTENT_DISPOSITION,
                        "attachment; filename=\"slow.bin\"",
                    ),
                    (
                        header::CONTENT_SECURITY_POLICY,
                        "default-src 'none'; style-src 'unsafe-inline'; sandbox",
                    ),
                ],
                axum::body::Body::from_stream(pieces),
            )
                .into_response();
        }
        "busy.bin" | UNREADABLE => {
            return crate::raw::error(
                StatusCode::SERVICE_UNAVAILABLE,
                "the file is stored as a delta",
            );
        }
        _ => {}
    }
    let files: Value =
        serde_json::from_str(include_str!("../fixtures/files.json")).unwrap_or_default();
    let size = files["files"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|file| file["path"] == path.as_str())
        .and_then(|file| file["size"].as_u64());
    let Some(size) = size.or_else(|| (path == "bare.svg").then_some(40)) else {
        return crate::raw::error(StatusCode::NOT_FOUND, "nothing is at this path");
    };
    let bytes: Vec<u8> = (0..size)
        .map(|i| b"0123456789abcdef"[(i % 16) as usize])
        .collect();
    let mut answer = crate::raw::answer(
        &method,
        &headers,
        &path,
        axum::body::Bytes::from(bytes),
        at.at.is_some(),
    );
    if path == "bare.svg" {
        answer.headers_mut().remove(header::CONTENT_SECURITY_POLICY);
    }
    answer
}

#[derive(Deserialize)]
struct At {
    at: Option<String>,
}

fn of_alpha(id: &str, answer: impl FnOnce() -> Response) -> Response {
    if id == ALPHA { answer() } else { missing() }
}

fn routes() -> Router {
    older()
        .route("/v1/files", get(files))
        .route("/v1/files/{*path}", get(file))
        .route("/v1/changes/files", get(file_changes))
        .route("/v1/raw/{*path}", get(raw))
}

/// Every route but the files'.
fn older() -> Router {
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
        .route("/v1/paths/{*path}", get(path))
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
        Stub::Busy => busy_routes(),
        Stub::FileChangesFailing => older()
            .route("/v1/files", get(files))
            .route("/v1/files/{*path}", get(file))
            .route("/v1/raw/{*path}", get(raw))
            .route(
                "/v1/changes/files",
                get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
            ),
        Stub::Older => older().fallback(|| async {
            let body = r#"{"error":"no such route"}"#.to_owned();
            (StatusCode::NOT_FOUND, json(body)).into_response()
        }),
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
