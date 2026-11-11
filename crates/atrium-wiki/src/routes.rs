//! The wiki's pages. Each handler asks Mneme, then renders plain HTML that works without
//! script. Every failure goes through [`respond`], so a Mneme that is away is one 503 page.

use std::str::FromStr;

use atrium_ui::{Shell, day, time};
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use maud::{Markup, html};
use serde::Deserialize;

use crate::markdown;
use crate::mneme::{
    CommitId, DiffLine, Event, EventKind, Listing, Mneme, MnemeError, Order, PageId, Run, Sort,
    Span,
};

/// Items per page of a listing.
const PAGE_SIZE: u64 = 50;
/// Versions the page itself lists.
const RECENT: u64 = 10;
/// Mneme's largest page of a listing, used to find a page by its path.
const MOST: u64 = 500;

/// Every route of the wiki.
pub fn router(mneme: Mneme) -> Router {
    Router::new()
        .route("/wiki", get(pages))
        .route("/wiki/page/{id}", get(page))
        .route("/wiki/page/{id}/history", get(history))
        .route("/wiki/page/{id}/at/{commit}", get(at))
        .route("/wiki/page/{id}/diff", get(diff))
        .route("/wiki/changes", get(changes))
        .route("/wiki/search", get(search))
        .route("/wiki/machines", get(machines))
        .route("/wiki/{*path}", get(by_path))
        .with_state(mneme)
}

/// Why a page cannot be shown.
#[derive(Debug)]
enum Failure {
    Mneme(MnemeError),
    /// The address's query cannot be read.
    BadQuery,
}

impl From<MnemeError> for Failure {
    fn from(error: MnemeError) -> Self {
        Self::Mneme(error)
    }
}

/// A page's title and content, or why there is none.
type Shown = Result<(String, Markup), Failure>;

/// The one place a wiki page's outcome becomes a response.
fn respond(shell: &Shell, shown: Shown) -> Response {
    let (status, title, why) = match shown {
        Ok((title, main)) => return shell.page(&title, main),
        Err(Failure::Mneme(MnemeError::NotFound)) => return shell.not_found(),
        Err(Failure::Mneme(MnemeError::Away(_))) => {
            let mut response = shell.respond(
                StatusCode::SERVICE_UNAVAILABLE,
                "The wiki is away",
                html! {
                    h1 { "The wiki is briefly away" }
                    p { "Mneme, which holds the wiki, did not answer in time. Try again in a minute; the rest of Atrium works." }
                },
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
        Err(Failure::Mneme(MnemeError::Broken(_))) => (
            StatusCode::BAD_GATEWAY,
            "Unexpected answer",
            "Mneme answered in a way this version of Atrium cannot read.".to_owned(),
        ),
    };
    shell.respond(status, title, html! { h1 { (title) } p { (why) } })
}

/// A page id from an address. One that cannot be an id is a page that is not there.
fn page_id(text: &str) -> Result<PageId, Failure> {
    text.parse().map_err(|_| MnemeError::NotFound.into())
}

/// The query parameters any wiki page reads. Each route uses some of them.
#[derive(Debug, Default, Deserialize)]
struct Params {
    sort: Option<String>,
    order: Option<String>,
    offset: Option<String>,
    q: Option<String>,
    days: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

fn params(query: Result<Query<Params>, QueryRejection>) -> Result<Params, Failure> {
    query
        .map(|Query(params)| params)
        .map_err(|_| Failure::BadQuery)
}

/// A parameter's value, where an empty one, as a form sends for "any", is none.
fn choice<T: FromStr>(value: Option<&str>) -> Result<Option<T>, Failure> {
    match value {
        None | Some("") => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| Failure::BadQuery),
    }
}

fn offset(params: &Params) -> Result<u64, Failure> {
    choice(params.offset.as_deref()).map(Option::unwrap_or_default)
}

/// `path` with `pairs` as its query.
fn href(path: &str, pairs: &[(&str, &str)]) -> String {
    let pairs: Vec<_> = pairs
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .collect();
    match serde_urlencoded::to_string(pairs) {
        Ok(query) if !query.is_empty() => format!("{path}?{query}"),
        _ => path.to_owned(),
    }
}

fn page_href(id: &PageId) -> String {
    format!("/wiki/page/{id}")
}

/// Links to the pages before and after `offset` of a listing of `total`.
fn paging(total: u64, offset: u64, size: u64, link: impl Fn(u64) -> String) -> Markup {
    html! {
        @if offset > 0 || offset + size < total {
            nav aria-label="Pages of this list" {
                ul {
                    @if offset > 0 {
                        li { a href=(link(offset.saturating_sub(size))) rel="prev" { "Newer or earlier" } }
                    }
                    @if offset + size < total {
                        li { a href=(link(offset + size)) rel="next" { "More" } }
                    }
                }
            }
        }
    }
}

/// One event, as the history, the recent changes and a page's recent versions list it.
fn event(event: &Event, with_page: bool) -> Markup {
    let page = page_href(&event.page);
    html! {
        li {
            (time(event.time)) " "
            @if with_page {
                a href=(page) { (event.path) } " "
            }
            (event.kind.word())
            @if let Some(from) = &event.from { " from " (from) }
            " by " (event.author)
            @if event.version { ", +" (event.added) " −" (event.removed) " words" }
            @if let Some(overlap) = &event.overlap { ", overlapping " (overlap) }
            "."
            @if event.kind != EventKind::Deleted {
                " " a href={ (page) "/at/" (event.commit) } { "Text at " (event.commit.short()) }
            }
            @if event.version {
                " " a href=(href(&format!("{page}/diff"), &[("to", event.commit.as_str())])) { "Changes" }
            }
        }
    }
}

/// A page's text with a heading, using the text's own level-1 heading when it has one.
fn text(markdown: &str, path: &str, title: &str) -> Markup {
    let rendered = markdown::render(markdown, path);
    html! {
        article {
            @if !rendered.titled { h1 { (title) } }
            (rendered.html)
        }
    }
}

async fn pages(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    respond(&shell, list(&mneme, query).await)
}

async fn list(mneme: &Mneme, query: Result<Query<Params>, QueryRejection>) -> Shown {
    let params = params(query)?;
    let sort = choice(params.sort.as_deref())?.unwrap_or(Sort::Title);
    let order = choice(params.order.as_deref())?.unwrap_or(sort.default_order());
    let offset = offset(&params)?;
    let listing = Listing {
        sort: Some(sort),
        order: Some(order),
        offset,
        limit: PAGE_SIZE,
    };
    let found = mneme.pages(listing).await?;
    let sorted = |by: Sort, order: Order, offset: u64| {
        href(
            "/wiki",
            &[
                ("sort", by.name()),
                ("order", order.name()),
                ("offset", &offset.to_string()),
            ],
        )
    };
    let main = html! {
        h1 { "Wiki" }
        p {
            (found.total) " pages, by " (sort.name()) ", "
            (if order == Order::Asc { "ascending" } else { "descending" }) ". "
            a href=(sorted(sort, order.flip(), 0)) { "Reverse the order" }
        }
        table {
            thead {
                tr {
                    @for by in Sort::ALL {
                        @let current = by == sort;
                        th scope="col" aria-sort=[current.then_some(if order == Order::Asc { "ascending" } else { "descending" })] {
                            a href=(sorted(by, if current { order.flip() } else { by.default_order() }, 0)) {
                                (by.name())
                            }
                        }
                    }
                }
            }
            tbody {
                @for page in &found.pages {
                    tr {
                        th scope="row" { a href=(page_href(&page.id)) { (page.title) } }
                        td { (time(page.updated)) }
                        td { (time(page.created)) }
                        td { (page.versions) }
                        td { (page.words) }
                        td { (page.churn) }
                    }
                }
            }
        }
        (paging(found.total, offset, PAGE_SIZE, |at| sorted(sort, order, at)))
    };
    Ok(("Wiki".to_owned(), main))
}

async fn page(shell: Shell, State(mneme): State<Mneme>, Path(id): Path<String>) -> Response {
    if id.parse::<PageId>().is_err() && markdown::is_page(&id) {
        return found_by_path(&shell, &mneme, &format!("page/{id}")).await;
    }
    respond(&shell, show(&mneme, &id).await)
}

async fn show(mneme: &Mneme, id: &str) -> Shown {
    let id = page_id(id)?;
    let (detail, recent) = tokio::try_join!(mneme.page(&id), mneme.versions(&id, 0, RECENT))?;
    let page = &detail.page;
    let history = format!("{}/history", page_href(&id));
    let main = html! {
        @if let Some(markdown) = &detail.markdown {
            (text(markdown, &page.path, &page.title))
        } @else {
            h1 { (page.title) }
            p {
                "This page was deleted "
                @if let Some(deleted) = page.deleted { (time(deleted)) }
                ". " a href=(history) { "Its history" } " keeps every version."
            }
        }
        section aria-labelledby="about" {
            h2 #about { "About this page" }
            dl {
                dt { "File" } dd { code { (page.path) } }
                dt { "Words" } dd { (page.words) }
                dt { "Created" } dd { (time(page.created)) }
                dt { "Last changed" } dd { (time(page.updated)) }
            }
        }
        section aria-labelledby="activity" {
            h2 #activity { "Activity" }
            table {
                thead {
                    tr {
                        th scope="col" { "Over" }
                        th scope="col" { "Versions" }
                        th scope="col" { "Sessions" }
                        th scope="col" { "Words added" }
                        th scope="col" { "Words removed" }
                    }
                }
                tbody {
                    @for window in &detail.windows {
                        tr {
                            th scope="row" {
                                @match window.days {
                                    Some(days) => { "the last " (days) " days" },
                                    None => "all time",
                                }
                            }
                            td { (window.versions) }
                            td { (window.sessions) }
                            td { (window.added) }
                            td { (window.removed) }
                        }
                    }
                }
            }
        }
        section aria-labelledby="recent" {
            h2 #recent { "Recent versions" }
            ol { @for change in &recent.events { (event(change, false)) } }
            p { a href=(history) { "All " (recent.total) " events" } }
        }
    };
    Ok((page.title.clone(), main))
}

async fn history(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(id): Path<String>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    respond(&shell, events_of(&mneme, &id, query).await)
}

async fn events_of(mneme: &Mneme, id: &str, query: Result<Query<Params>, QueryRejection>) -> Shown {
    let id = page_id(id)?;
    let offset = offset(&params(query)?)?;
    let (detail, events) =
        tokio::try_join!(mneme.page(&id), mneme.versions(&id, offset, PAGE_SIZE))?;
    let title = format!("History of {}", detail.page.title);
    let base = format!("{}/history", page_href(&id));
    let main = html! {
        h1 { (title) }
        p { a href=(page_href(&id)) { "The page as it is now" } }
        ol { @for change in &events.events { (event(change, false)) } }
        (paging(events.total, offset, PAGE_SIZE, |at| href(&base, &[("offset", &at.to_string())])))
    };
    Ok((title, main))
}

async fn at(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path((id, commit)): Path<(String, String)>,
) -> Response {
    respond(&shell, version(&mneme, &id, &commit).await)
}

async fn version(mneme: &Mneme, id: &str, commit: &str) -> Shown {
    let id = page_id(id)?;
    let commit: CommitId = commit.parse().map_err(|_| MnemeError::NotFound)?;
    let (detail, version) = tokio::try_join!(mneme.page(&id), mneme.version(&id, &commit))?;
    let title = format!("{} at {}", detail.page.title, version.commit.short());
    let page = page_href(&id);
    let main = html! {
        p {
            "This is the text as commit " code { (version.commit.short()) } " left it. "
            a href=(page) { "The page as it is now" } ", "
            a href=(href(&format!("{page}/diff"), &[("to", version.commit.as_str())])) { "what this commit changed" } "."
        }
        (text(&version.markdown, &detail.page.path, &title))
    };
    Ok((title, main))
}

async fn diff(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(id): Path<String>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    respond(&shell, compare(&mneme, &id, query).await)
}

fn line(line: &DiffLine) -> Markup {
    let spans = |spans: &[Span]| {
        html! {
            @for span in spans {
                @if span.changed { mark { (span.text) } } @else { (span.text) }
            }
        }
    };
    html! {
        @match line {
            DiffLine::Same(text) => li { "  " (text) },
            DiffLine::Removed(parts) => li { del { "- " (spans(parts)) } },
            DiffLine::Added(parts) => li { ins { "+ " (spans(parts)) } },
            DiffLine::Skipped(lines) => li { i { (lines) " unchanged lines" } },
        }
    }
}

async fn compare(mneme: &Mneme, id: &str, query: Result<Query<Params>, QueryRejection>) -> Shown {
    let id = page_id(id)?;
    let params = params(query)?;
    let from: Option<CommitId> = choice(params.from.as_deref())?;
    let to: Option<CommitId> = choice(params.to.as_deref())?;
    let (detail, diff) =
        tokio::try_join!(mneme.page(&id), mneme.diff(&id, from.as_ref(), to.as_ref()))?;
    let title = format!("Changes to {}", detail.page.title);
    let page = page_href(&id);
    let main = html! {
        h1 { (title) }
        p {
            "From "
            @match &diff.from {
                Some(from) => a href={ (page) "/at/" (from) } { code { (from.short()) } },
                None => "nothing",
            }
            " to "
            a href={ (page) "/at/" (diff.to) } { code { (diff.to.short()) } }
            ". Removed lines start with -, added lines with +, and changed words are marked."
        }
        @match &diff.lines {
            Some(lines) => ol.diff { @for each in lines { (line(each)) } },
            None => p { "These texts are too long to compare." },
        }
    };
    Ok((title, main))
}

async fn changes(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    respond(&shell, recent(&mneme, query).await)
}

async fn recent(mneme: &Mneme, query: Result<Query<Params>, QueryRejection>) -> Shown {
    let offset = offset(&params(query)?)?;
    let found = mneme.changes(offset, PAGE_SIZE).await?;
    let days = found.events.chunk_by(|a, b| day(a.time) == day(b.time));
    let main = html! {
        h1 { "Recent changes" }
        @for events in days {
            section {
                h2 { (day(events[0].time)) }
                ol { @for change in events { (event(change, true)) } }
            }
        }
        (paging(found.total, offset, PAGE_SIZE, |at| href("/wiki/changes", &[("offset", &at.to_string())])))
    };
    Ok(("Recent changes".to_owned(), main))
}

/// The `count` latest changes for the home page, or a line saying the wiki is away.
pub async fn latest(mneme: &Mneme, count: u64) -> Markup {
    if let Ok(found) = mneme.changes(0, count).await {
        html! { ol { @for change in &found.events { (event(change, true)) } } }
    } else {
        html! { p { "The wiki is briefly away." } }
    }
}

async fn search(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    respond(&shell, find(&mneme, query).await)
}

fn snippet(runs: &[Run]) -> Markup {
    html! { @for run in runs { @if run.hit { mark { (run.text) } } @else { (run.text) } } }
}

fn select(name: &str, label: &str, chosen: &str, options: &[(&str, &str)]) -> Markup {
    html! {
        label for=(name) { (label) }
        " "
        select #(name) name=(name) {
            @for (value, text) in options {
                option value=(value) selected[*value == chosen] { (text) }
            }
        }
    }
}

async fn find(mneme: &Mneme, query: Result<Query<Params>, QueryRejection>) -> Shown {
    let params = params(query)?;
    let words = params.q.as_deref().unwrap_or_default().trim();
    let sort: Option<Sort> = choice(params.sort.as_deref())?;
    let order: Option<Order> = choice(params.order.as_deref())?;
    let days: Option<u32> = choice(params.days.as_deref())?;
    let offset = offset(&params)?;
    let listing = Listing {
        sort,
        order,
        offset,
        limit: PAGE_SIZE,
    };
    let found = if words.is_empty() {
        None
    } else {
        Some(mneme.search(words, days, listing).await?)
    };
    let sorts: Vec<(&str, &str)> = [("", "best match")]
        .into_iter()
        .chain(Sort::ALL.map(|sort| (sort.name(), sort.name())))
        .collect();
    let chosen = |value: &Option<String>| value.clone().unwrap_or_default();
    let main = html! {
        h1 { "Search" }
        form action="/wiki/search" method="get" role="search" {
            p {
                label for="q" { "Words" }
                " "
                input #q type="search" name="q" value=(words);
                " "
                button { "Search" }
            }
            p {
                (select("sort", "Sort by", &chosen(&params.sort), &sorts))
                " "
                (select("order", "Order", &chosen(&params.order), &[("", "usual"), ("asc", "ascending"), ("desc", "descending")]))
                " "
                (select("days", "Changed within", &chosen(&params.days), &[("", "any time"), ("7", "7 days"), ("30", "30 days"), ("90", "90 days"), ("365", "a year")]))
            }
        }
        @if let Some(found) = &found {
            h2 { (found.total) (if found.total == 1 { " page matches" } else { " pages match" }) }
            ol {
                @for hit in &found.hits {
                    li {
                        a href=(page_href(&hit.page.id)) { (hit.page.title) }
                        " " code { (hit.page.path) } ", changed " (time(hit.page.updated))
                        p { (snippet(&hit.snippet)) }
                    }
                }
            }
            (paging(found.total, offset, PAGE_SIZE, |at| href("/wiki/search", &[
                ("q", words),
                ("sort", &chosen(&params.sort)),
                ("order", &chosen(&params.order)),
                ("days", &chosen(&params.days)),
                ("offset", &at.to_string()),
            ])))
        } @else {
            p {
                "Words must all match. Put a phrase in quotes, end a word with * to match its start, "
                "and put - before a word to leave out pages with it."
            }
        }
    };
    Ok(("Search".to_owned(), main))
}

async fn machines(shell: Shell, State(mneme): State<Mneme>) -> Response {
    respond(&shell, autosave(&mneme).await)
}

async fn autosave(mneme: &Mneme) -> Shown {
    let found = mneme.machines().await?;
    let when = |at: Option<i64>| html! { @match at { Some(at) => (time(at)), None => "never" } };
    let main = html! {
        h1 { "Machines" }
        p { "Each machine's autosave service, as Mneme last heard from it." }
        table {
            thead {
                tr {
                    th scope="col" { "Machine" }
                    th scope="col" { "State" }
                    th scope="col" { "Up to date" }
                    th scope="col" { "Last sync" }
                    th scope="col" { "Last report" }
                }
            }
            tbody {
                @for machine in &found.machines {
                    tr {
                        th scope="row" { (machine.name) }
                        td {
                            (machine.state.as_deref().unwrap_or("not reported"))
                            @if let Some(detail) = machine.detail.as_deref().filter(|detail| !detail.is_empty()) {
                                ": " (detail)
                            }
                        }
                        td {
                            @if machine.current { "yes" } @else { "no" }
                            @if let Some(head) = &machine.head { " (" code { (head.get(..7).unwrap_or(head)) } ")" }
                        }
                        td { (when(machine.seen)) }
                        td { (when(machine.reported)) }
                    }
                }
            }
        }
    };
    Ok(("Machines".to_owned(), main))
}

async fn by_path(shell: Shell, State(mneme): State<Mneme>, Path(path): Path<String>) -> Response {
    if !markdown::is_page(&path) {
        return shell.not_found();
    }
    found_by_path(&shell, &mneme, &path).await
}

/// Sends a link written as the file's path, as pages link to each other, to the page that
/// is at that path now.
async fn found_by_path(shell: &Shell, mneme: &Mneme, path: &str) -> Response {
    let mut offset = 0;
    loop {
        let listing = Listing {
            sort: Some(Sort::Title),
            order: None,
            offset,
            limit: MOST,
        };
        let found = match mneme.pages(listing).await {
            Ok(found) => found,
            Err(error) => return respond(shell, Err(error.into())),
        };
        if let Some(page) = found.pages.iter().find(|page| page.path == path) {
            return Redirect::to(&page_href(&page.id)).into_response();
        }
        offset += MOST;
        if found.pages.is_empty() || offset >= found.total {
            return shell.not_found();
        }
    }
}
