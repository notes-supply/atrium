//! A page as it is now, `/wiki/page/{id}`, and as it was, `/wiki/page/{id}/at/{commit}`: the
//! prototype's `pageView` and `versionView`.

use atrium_core::Local;
use atrium_ui::components::{Mark, Want, delta, mark};
use atrium_ui::draw::{self, Activity};
use atrium_ui::words::number;
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::{Path, State};
use axum::response::Response;
use maud::{Markup, html};

use crate::markdown::{self, Heading};
use crate::mneme::{CommitId, Event, Events, Mneme, MnemeError, PageDetail, PageId, Window};
use crate::routes::{Failure, bare, href, losers, overlap_mark, page_href, page_id, respond};

/// Versions the page's rail lists.
const RECENT: u64 = 4;
/// Events a version's page reads to find its number, at most.
const MOST_EVENTS: u64 = 2000;
const DAY: i64 = 24 * 60 * 60;

pub async fn show(shell: Shell, State(mneme): State<Mneme>, Path(id): Path<String>) -> Response {
    if id.parse::<PageId>().is_err() && markdown::is_page(&id) {
        return crate::lost::found_by_path(&shell, &mneme, &format!("page/{id}")).await;
    }
    let shown = page(&shell, &mneme, &id).await;
    respond(&shell, &mneme, shown)
}

/// The path as breadcrumbs: `Wiki / homelab / backup-rotation`.
fn crumbs(path: &str) -> Markup {
    html! {
        p.crumbs {
            a href="/wiki" { "Wiki" }
            @for part in bare(path).split('/') {
                " " span aria-hidden="true" { "/" } " " (part)
            }
        }
    }
}

/// `14 Jun 2026`.
fn date(local: &Local, seconds: i64) -> String {
    let at = local.at(seconds);
    let short = local.date_short(seconds);
    if short.ends_with(&at.year().to_string()) {
        short
    } else {
        format!("{short} {}", at.year())
    }
}

/// The page's activity over the last 7, 30 and 90 days and all time.
pub fn windows(windows: &[Window]) -> Markup {
    html! {
        table.windows {
            caption.vh { "Versions, edit sessions and words over four windows" }
            thead {
                tr {
                    th scope="col" { "Window" }
                    th scope="col" { "Vers." }
                    th scope="col" { "Sess." }
                    th scope="col" { span aria-hidden="true" { "+" } span.vh { "Words added" } }
                    th scope="col" { span aria-hidden="true" { "−" } span.vh { "Words removed" } }
                }
            }
            tbody {
                @for window in windows {
                    tr class={ "r" (window.days.unwrap_or(0)) } {
                        th scope="row" {
                            @match window.days {
                                Some(days) => { (days) " days" },
                                None => "All time",
                            }
                        }
                        td { (window.versions) }
                        td { (window.sessions) }
                        td.a { (number(window.added)) }
                        td.r { (number(window.removed)) }
                    }
                }
            }
        }
    }
}

/// Words changed per day over the last 90 days, from the events in them.
pub fn ninety_days(local: &Local, events: &[Event], id: &str) -> Markup {
    let mut days = [Activity::default(); 90];
    for event in events {
        let ago = (local.seconds() - event.time).div_euclid(DAY);
        if let Some(day) = usize::try_from(ago).ok().and_then(|ago| days.get_mut(ago)) {
            day.versions += 1;
            day.words += event.added + event.removed;
        }
    }
    draw::history(&days, id)
}

fn toc(headings: &[Heading]) -> Markup {
    html! {
        details.toc open data-module="toc" {
            summary { "On this page" }
            ol {
                @for heading in headings.iter().filter(|heading| heading.level == 2) {
                    li { a href={ "#" (heading.id) } { (heading.text) } }
                }
            }
        }
    }
}

async fn page(shell: &Shell, mneme: &Mneme, id: &str) -> Result<Page, Failure> {
    let id = page_id(id)?;
    let local = shell.local();
    let since = local.seconds() - 90 * DAY;
    let (detail, recent, ninety, _) = tokio::try_join!(
        mneme.page(&id),
        mneme.versions(&id, 0, RECENT, None),
        mneme.versions(&id, 0, 500, Some(since)),
        mneme.status(),
    )?;
    let losers = losers(mneme, &recent.events).await;
    let PageDetail {
        page,
        markdown,
        windows: counts,
    } = detail;
    let rendered = markdown
        .as_deref()
        .map(|text| markdown::render(text, &page.path));
    let href = page_href(&id);
    let history = format!("{href}/history");
    let newest = recent.events.first();
    let body = html! {
        div.read-progress aria-hidden="true" {}
        main #(MAIN) .page {
            aside.rail.rail-l aria-label="This page" {
                (crumbs(&page.path))
                @if let Some(rendered) = &rendered { (toc(&rendered.headings)) }
                dl.facts {
                    div { dt { "Path" } dd { (page.path) } }
                    div { dt { "Words" } dd { (number(page.words)) } }
                    div { dt { "Created" } dd { (local.time(page.created, &date(local, page.created))) } }
                    div { dt { "Versions" } dd { a href=(history) { (recent.total) } } }
                }
            }
            article.paper.surface data-surface data-reading data-module="continue preview" data-page=(id) aria-labelledby="doc-title" {
                header.doc-head {
                    h1 #doc-title .doc-title { (page.title) }
                    p.doc-meta {
                        @if let Some(deleted) = page.deleted {
                            "Deleted " (local.when_long(deleted)) ". "
                            a href=(history) { "Its history" } " keeps every version."
                        } @else if let Some(newest) = newest {
                            "Version " (recent.total) ", " (local.when_long(newest.time)) " by " (newest.author) ". "
                            a href=(format!("{href}/diff")) { "What changed" } "."
                        }
                    }
                }
                @if let Some(rendered) = &rendered { (rendered.html) }
            }
            aside #history .rail.rail-r.surface data-surface aria-labelledby="hist-h" {
                h2 #hist-h .rail-h { "History" }
                figure.hist {
                    (ninety_days(local, &ninety.events, "hist-title"))
                    figcaption.hist-scale { span { "90 days ago" } span { "today" } }
                }
                (windows(&counts))
                h3.rail-h3 { "Recent versions" }
                ol.versions {
                    @for (i, event) in recent.events.iter().enumerate() {
                        @let n = recent.total - u64::try_from(i).unwrap_or(0);
                        li.cur[i == 0] {
                            a.v href=(if i == 0 { href.clone() } else { format!("{href}/at/{}", event.commit) }) { "v" (n) }
                            span { (local.time(event.time, &local.when(event.time))) " " span.who { (event.author) } }
                            (delta(event.added, event.removed))
                            @if let Some(mark) = overlap_mark(event, &losers, " edited the same lines") { (mark) }
                        }
                    }
                }
                a.more href=(history) { "All " (recent.total) " versions" }
            }
        }
    };
    Ok(Page::new(
        format!("{}, Atrium wiki", page.title),
        "A page of the wiki, its history and what is on it.",
        body,
    ))
}

/// Every event of a page, newest first, up to [`MOST_EVENTS`].
pub async fn all_events(mneme: &Mneme, id: &PageId) -> Result<Events, MnemeError> {
    let mut all = mneme.versions(id, 0, 500, None).await?;
    while u64::try_from(all.events.len()).unwrap_or(u64::MAX) < all.total.min(MOST_EVENTS) {
        let offset = u64::try_from(all.events.len()).unwrap_or(0);
        let more = mneme.versions(id, offset, 500, None).await?;
        if more.events.is_empty() {
            break;
        }
        all.events.extend(more.events);
    }
    Ok(all)
}

/// An event's number: 1 for the first, the total for the newest.
pub fn number_of(all: &Events, commit: &CommitId) -> Option<u64> {
    let index = all
        .events
        .iter()
        .position(|event| &event.commit == commit)?;
    Some(all.total - u64::try_from(index).ok()?)
}

pub async fn version(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path((id, commit)): Path<(String, String)>,
) -> Response {
    let shown = old(&shell, &mneme, &id, &commit).await;
    respond(&shell, &mneme, shown)
}

async fn old(shell: &Shell, mneme: &Mneme, id: &str, commit: &str) -> Result<Page, Failure> {
    let id = page_id(id)?;
    let commit: CommitId = commit.parse().map_err(|_| MnemeError::NotFound)?;
    let (detail, version, all, _) = tokio::try_join!(
        mneme.page(&id),
        mneme.version(&id, &commit),
        all_events(mneme, &id),
        mneme.status(),
    )?;
    let local = shell.local();
    let page = &detail.page;
    let href = page_href(&id);
    let index = all
        .events
        .iter()
        .position(|event| event.commit == version.commit);
    let event = index.map(|i| &all.events[i]);
    let n = number_of(&all, &version.commit);
    let label = n.map_or_else(|| version.commit.short().to_owned(), |n| format!("v{n}"));
    let since = index.map_or(0, |i| u64::try_from(i).unwrap_or(0));
    let step = |i: usize| -> Option<(String, String)> {
        let event = all.events.get(i)?;
        let n = number_of(&all, &event.commit)?;
        let to = if i == 0 {
            href.clone()
        } else {
            format!("{href}/at/{}", event.commit)
        };
        Some((to, format!("v{n}, {}", local.date_short(event.time))))
    };
    let older = index.and_then(|i| step(i + 1));
    let newer = index.and_then(|i| i.checked_sub(1)).and_then(step);
    let rendered = markdown::render(&version.markdown, &page.path);
    let changed_since = href_diff(&href, &version.commit);
    let body = html! {
        main #(MAIN) .version {
            div.old-band role="note" aria-label="Old version" {
                (mark(Mark::Want(Want::Look)))
                p {
                    "You are reading " b { "version " (n.map_or_else(|| version.commit.short().to_owned(), |n| n.to_string())) " of " (all.total) }
                    @if let Some(event) = event { ", saved " (local.when_long(event.time)) " by " (event.author) }
                    ". The page has changed " (since) (if since == 1 { " time" } else { " times" }) " since."
                }
                span.btns {
                    a.btn href=(href) { "Read the current version" }
                    a.btn.quiet-btn href=(changed_since) { "What changed since" }
                }
            }
            article.paper.surface data-surface data-reading aria-labelledby="doc-title" {
                header.doc-head {
                    p.crumbs { (page.path) " at " (label) }
                    h1 #doc-title .doc-title { (page.title) }
                    p.doc-meta {
                        "Version " (n.map_or_else(|| version.commit.short().to_owned(), |n| n.to_string())) " of " (all.total) "."
                        @if let Some(event) = event { " " (number(event.words)) " words." }
                    }
                }
                (rendered.html)
            }
            aside.rail.rail-r.surface data-surface aria-labelledby="vs-h" {
                h2 #vs-h .rail-h { "Version " (n.map_or_else(|| version.commit.short().to_owned(), |n| n.to_string())) }
                @if let Some(event) = event {
                    dl.facts {
                        div { dt { "Saved" } dd { (local.time(event.time, &format!("{} {}", local.date_short(event.time), local.hm(event.time)))) } }
                        div { dt { "By" } dd { (event.author) } }
                        div { dt { "Words" } dd { (number(event.words)) } }
                        div { dt { "Changed" } dd { (delta(event.added, event.removed)) } }
                    }
                }
                nav.vstep aria-label="Neighbouring versions" {
                    @if let Some((to, text)) = &older { a href=(to) { "← " (text) } }
                    @if let Some((to, text)) = &newer { a href=(to) { (text) " →" } }
                    a href=(format!("{href}/history")) { "Every version" }
                }
            }
        }
    };
    Ok(Page::new(
        format!(
            "{}, version {}, Atrium wiki",
            page.title,
            n.map_or_else(|| version.commit.short().to_owned(), |n| n.to_string())
        ),
        "An old version of a wiki page.",
        body,
    ))
}

/// What changed between `from` and the current text.
fn href_diff(page: &str, from: &CommitId) -> String {
    href(&format!("{page}/diff"), &[("from", from.as_str())])
}
