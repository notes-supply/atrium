//! The wiki's routes and what they share. Each page's handler and template live in a module of
//! their own, one per function of the prototype's `tools/build.mjs`; every failure goes through
//! [`respond`], so a Mneme that is away is one 503 page.

use std::collections::HashMap;
use std::str::FromStr;

use atrium_core::Local;
use atrium_ui::components::{delta, overlap};
use atrium_ui::{Page, Shell};
use axum::Router;
use axum::extract::Query;
use axum::extract::rejection::QueryRejection;
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::future::join_all;
use maud::{Markup, html};
use serde::Deserialize;

use crate::mneme::{Event, EventKind, Filters, Listing, Mneme, MnemeError, PageId, Sort};
use crate::{article, changes, data, diff, history, list, lost, machines, search};

/// Every route of the wiki.
pub fn router(mneme: Mneme) -> Router {
    Router::new()
        .route("/wiki", get(list::show))
        .route("/wiki/page/{id}", get(article::show))
        .route("/wiki/page/{id}/history", get(history::show))
        .route("/wiki/page/{id}/at/{commit}", get(article::version))
        .route("/wiki/page/{id}/diff", get(diff::show))
        .route("/wiki/changes", get(changes::show))
        .route("/wiki/search", get(search::show))
        .route("/wiki/machines", get(machines::show))
        .route("/wiki/index.json", get(data::index))
        .route("/wiki/preview.json", get(data::preview))
        .route("/wiki/{*path}", get(lost::by_path))
        .with_state(mneme)
}

/// Why a page cannot be shown.
#[derive(Debug)]
pub enum Failure {
    Mneme(MnemeError),
    /// The address's query cannot be read.
    BadQuery,
}

impl From<MnemeError> for Failure {
    fn from(error: MnemeError) -> Self {
        Self::Mneme(error)
    }
}

/// The one place a wiki page's outcome becomes a response.
pub fn respond(shell: &Shell, mneme: &Mneme, shown: Result<Page, Failure>) -> Response {
    let (status, title, why) = match shown {
        Ok(page) => return shell.page(page),
        Err(Failure::Mneme(MnemeError::NotFound)) => return shell.not_found(),
        Err(Failure::Mneme(MnemeError::Away(_))) => {
            let mut response = shell.respond(
                StatusCode::SERVICE_UNAVAILABLE,
                crate::home::away_page(shell, mneme),
            );
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("30"));
            return response;
        }
        Err(Failure::Mneme(MnemeError::Refused(why))) => {
            (StatusCode::BAD_REQUEST, "Not understood", why)
        }
        Err(Failure::BadQuery) => (
            StatusCode::BAD_REQUEST,
            "Not understood",
            "The address asks for something the wiki does not offer.".to_owned(),
        ),
        Err(Failure::Mneme(MnemeError::Broken(detail))) => (
            StatusCode::BAD_GATEWAY,
            "Unexpected answer",
            format!("Mneme answered in a way this version of Atrium cannot read: {detail}"),
        ),
    };
    let body = html! {
        main #(atrium_ui::MAIN) .not-found {
            section.lost aria-labelledby="lost-h" {
                p.code-404 { (status.as_u16()) ", " (shell.path()) }
                h1 #lost-h { (title) }
                p { (why) }
            }
        }
    };
    shell.respond(
        status,
        Page::new(format!("{title}, Atrium wiki"), "The wiki.", body),
    )
}

/// A page id from an address. One that cannot be an id is a page that is not there.
pub fn page_id(text: &str) -> Result<PageId, Failure> {
    text.parse().map_err(|_| MnemeError::NotFound.into())
}

/// The query parameters any wiki page reads. Each route uses some of them.
#[derive(Debug, Default, Deserialize)]
pub struct Params {
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: Option<String>,
    pub offset: Option<String>,
    pub q: Option<String>,
    pub changed: Option<String>,
    pub created: Option<String>,
    pub vmin: Option<String>,
    pub vmax: Option<String>,
    pub wmin: Option<String>,
    pub wmax: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub only: Option<String>,
}

pub fn params(query: Result<Query<Params>, QueryRejection>) -> Result<Params, Failure> {
    query
        .map(|Query(params)| params)
        .map_err(|_| Failure::BadQuery)
}

/// A parameter's value, where an empty one, as a form sends for "any", is none.
pub fn choice<T: FromStr>(value: Option<&str>) -> Result<Option<T>, Failure> {
    match value {
        None | Some("") => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| Failure::BadQuery),
    }
}

/// The 1-based page of a list the address asks for.
pub fn page_number(params: &Params) -> Result<u64, Failure> {
    Ok(choice::<u64>(params.page.as_deref())?.unwrap_or(1).max(1))
}

/// `path` with `pairs` as its query, leaving out empty values.
pub fn href(path: &str, pairs: &[(&str, &str)]) -> String {
    let pairs: Vec<_> = pairs
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .collect();
    match serde_urlencoded::to_string(pairs) {
        Ok(query) if !query.is_empty() => format!("{path}?{query}"),
        _ => path.to_owned(),
    }
}

pub fn page_href(id: &PageId) -> String {
    format!("/wiki/page/{id}")
}

/// A page's path as the pages show it, without `.md`.
pub fn bare(path: &str) -> &str {
    path.strip_suffix(".md").unwrap_or(path)
}

/// Every page's title by id, deleted pages too, so lists of events can name their pages. A
/// page beyond the first 2,000 is named by its file.
pub async fn titles(mneme: &Mneme) -> Result<HashMap<PageId, String>, MnemeError> {
    const STEP: u64 = 500;
    let mut titles = HashMap::new();
    let mut offset = 0;
    loop {
        let found = mneme
            .pages(Listing {
                sort: Some(Sort::Title),
                order: None,
                offset,
                limit: STEP,
                filters: Filters {
                    deleted: true,
                    ..Filters::default()
                },
            })
            .await?;
        let done = found.pages.len() < usize::try_from(STEP).unwrap_or(usize::MAX);
        titles.extend(found.pages.into_iter().map(|page| (page.id, page.title)));
        offset += STEP;
        if done || offset >= found.total || offset >= 2000 {
            return Ok(titles);
        }
    }
}

/// The title of an event's page, or its file's name when the listing did not have it.
pub fn title_of<'a>(titles: &'a HashMap<PageId, String>, event: &'a Event) -> &'a str {
    titles.get(&event.page).map_or_else(
        || {
            let file = event.path.rsplit('/').next().unwrap_or(&event.path);
            bare(file)
        },
        String::as_str,
    )
}

/// The edit an overlap mark names, as far as Atrium can find it: the author, and where its
/// diff is. Mneme writes the commit of the edit that lost; older answers wrote a machine.
#[derive(Debug, Clone)]
pub struct Loser {
    pub author: String,
    pub diff: Option<String>,
}

/// Finds the edit each overlap among `events` names: first among `events`, then in the history
/// of each page an unresolved overlap is on.
pub async fn losers(mneme: &Mneme, events: &[Event]) -> HashMap<String, Loser> {
    let mut found: HashMap<String, Loser> = HashMap::new();
    let known: HashMap<&str, &Event> = events.iter().map(|e| (e.commit.as_str(), e)).collect();
    let mut pages: Vec<&PageId> = Vec::new();
    for event in events {
        let Some(commit) = event.overlap.as_deref() else {
            continue;
        };
        if let Some(lost) = known.get(commit) {
            found.insert(commit.to_owned(), loser(lost));
        } else if commit.parse::<crate::mneme::CommitId>().is_ok() {
            if !pages.contains(&&event.page) {
                pages.push(&event.page);
            }
        } else {
            found.insert(
                commit.to_owned(),
                Loser {
                    author: commit.to_owned(),
                    diff: None,
                },
            );
        }
    }
    let histories = join_all(
        pages
            .into_iter()
            .map(|page| mneme.versions(page, 0, 500, None)),
    )
    .await;
    for history in histories.into_iter().flatten() {
        for event in &history.events {
            let wanted = events
                .iter()
                .any(|e| e.overlap.as_deref() == Some(event.commit.as_str()));
            if wanted {
                found.insert(event.commit.to_string(), loser(event));
            }
        }
    }
    found
}

fn loser(event: &Event) -> Loser {
    Loser {
        author: event.author.clone(),
        diff: Some(href(
            &format!("{}/diff", page_href(&event.page)),
            &[("to", event.commit.as_str())],
        )),
    }
}

/// The overlap mark for an event, saying whose edit lost the same lines.
pub fn overlap_mark(event: &Event, losers: &HashMap<String, Loser>, tail: &str) -> Option<Markup> {
    let commit = event.overlap.as_deref()?;
    let text = match losers.get(commit) {
        Some(Loser {
            author,
            diff: Some(diff),
        }) => html! { a href=(diff) { (author) } (tail) },
        Some(Loser { author, diff: None }) => html! { (author) (tail) },
        None => html! { "another edit" (tail) },
    };
    Some(overlap(&text))
}

/// How a change row shows its time and whether its change links to the diff.
#[derive(Debug, Clone, Copy)]
pub struct RowLook {
    /// The day as well as the time, for lists that span days.
    pub day: bool,
    pub diff_link: bool,
}

/// One change, as recent changes list it: time, page, what happened, who, and the words.
pub fn change_row(
    local: &Local,
    last: Option<i64>,
    event: &Event,
    title: &str,
    losers: &HashMap<String, Loser>,
    look: RowLook,
) -> Markup {
    let new = last.is_some_and(|last| event.time > last);
    let page = page_href(&event.page);
    let diff = href(&format!("{page}/diff"), &[("to", event.commit.as_str())]);
    let kind = match event.kind {
        EventKind::Created => Some("created".to_owned()),
        EventKind::Renamed => Some(format!(
            "renamed from {}",
            bare(event.from.as_deref().unwrap_or_default())
        )),
        EventKind::Deleted => Some(format!("deleted, was {}", bare(&event.path))),
        EventKind::Edited | EventKind::Other => None,
    };
    let when = if look.day {
        local.when(event.time)
    } else {
        local.hm(event.time)
    };
    html! {
        li data-new[new] {
            (local.time(event.time, &when))
            span {
                @if event.kind == EventKind::Deleted {
                    span.ttl.ttl-gone { (title) }
                } @else {
                    a.ttl href=(page) data-row data-carry { (title) }
                }
                @if let Some(kind) = kind { span.kind { (kind) } }
                @if let Some(mark) = overlap_mark(event, losers, " edited the same lines") { (mark) }
                @if new { span.vh { ", new since your last visit" } }
            }
            span.who { (event.author) }
            @if event.kind == EventKind::Renamed && !event.version {
                span {}
            } @else if look.diff_link && event.kind != EventKind::Deleted {
                a.delta-link href=(diff) { (delta(event.added, event.removed)) }
            } @else {
                (delta(event.added, event.removed))
            }
        }
    }
}

/// A response for an answer that is only a status, such as a redirect.
pub fn redirect(to: &str) -> Response {
    axum::response::Redirect::to(to).into_response()
}
