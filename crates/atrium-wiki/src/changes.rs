//! `/wiki/changes`: recent changes by day, with a rule at the last visit. The prototype's
//! `changesView`.

use std::collections::HashSet;

use atrium_ui::components::since;
use atrium_ui::words::{count, number};
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::Response;
use maud::{Markup, html};

use crate::mneme::{Event, Events, FileEvent, FileEvents, Mneme, MnemeError};
use crate::routes::{
    Change, Failure, Params, RowLook, any_row, choice, href, losers, merged, params, respond,
    titles,
};

/// Days a page of changes shows.
const DAYS: usize = 5;
/// Events one page of changes reads, at most.
const READ: u64 = 500;

pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = changes(&shell, &mneme, query).await;
    respond(&shell, &mneme, shown)
}

/// Events grouped by the reader's day, the first `DAYS` days. When the read was cut short,
/// the last day may be missing some of its events, so it is left for the next page, unless
/// it is the only one: then the page shows what it has of it, and the next goes on from
/// there.
fn days<'a, 'e>(shell: &Shell, events: &'a [Change<'e>], full: bool) -> Vec<&'a [Change<'e>]> {
    let local = shell.local();
    let mut days: Vec<&[Change<'e>]> = events
        .chunk_by(|a, b| local.at(a.time()).date() == local.at(b.time()).date())
        .take(DAYS + 1)
        .collect();
    if days.len() > DAYS || (full && days.len() > 1) {
        days.pop();
    }
    days
}

/// What the days shown hold: how many changes, pages and files, the words, and who wrote
/// them.
fn side(days: &[&[Change<'_>]], shown: &[&Change<'_>]) -> Markup {
    let mut authors: Vec<&str> = Vec::new();
    for change in shown {
        if !authors.contains(&change.author()) {
            authors.push(change.author());
        }
    }
    let page_events: Vec<&Event> = shown
        .iter()
        .filter_map(|change| match change {
            Change::Page(event) => Some(*event),
            Change::File(_) => None,
        })
        .collect();
    let files: HashSet<&str> = shown
        .iter()
        .filter_map(|change| match change {
            Change::File(event) => Some(event.path.as_str()),
            Change::Page(_) => None,
        })
        .collect();
    let pages: HashSet<_> = page_events.iter().map(|event| &event.page).collect();
    let added: u64 = page_events.iter().map(|event| event.added).sum();
    let removed: u64 = page_events.iter().map(|event| event.removed).sum();
    html! {
        aside.changes-side.surface data-surface aria-labelledby="side-h" {
            h2 #side-h .rail-h { "These " (count(u64::try_from(days.len()).unwrap_or(0), "day", "days")) }
            dl.side-stats {
                div { dt { "Changes" } dd { (shown.len()) } }
                div { dt { "Pages" } dd { (pages.len()) } }
                @if !files.is_empty() { div { dt { "Files" } dd { (files.len()) } } }
                div { dt { "Words added" } dd.a { (number(added)) } }
                div { dt { "Words removed" } dd { (number(removed)) } }
            }
            p.diff-sum {
                "By " (authors.join(", ")) ". Machines save their own changes; a name is someone who committed by hand."
            }
        }
    }
}

/// Where a page of changes starts: the newest first, or from a time back, past the changes
/// at the newest second of it that the page before showed. Several changes can share one
/// time, and one read can end among them, so a time alone would lose or repeat some.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Cursor {
    /// Only changes older than this time.
    before: Option<i64>,
    /// Changes to pages, and to files, already shown at the newest second before `before`.
    pages: u64,
    files: u64,
}

/// The events from the cursor back, when the address names one and Mneme pages by time, and
/// whether it did; otherwise from `offset`.
async fn older(mneme: &Mneme, offset: u64, at: Cursor) -> Result<(Events, bool), MnemeError> {
    if let Some(before) = at.before
        && let Some(found) = mneme.changes_before(before, at.pages, READ).await?
    {
        return Ok((found, true));
    }
    Ok((mneme.changes(offset, READ, None).await?, false))
}

/// Changes to pages and files in one list, newest first, and whether a read was cut short.
/// A read cut short may lack the other list's changes older than its oldest, so those are
/// left for the next page.
fn whole<'a>(pages: &'a [Event], files: &'a [FileEvent]) -> (Vec<Change<'a>>, bool) {
    let read = |n: usize| u64::try_from(n).unwrap_or(0) == READ;
    let (pages_full, files_full) = (read(pages.len()), read(files.len()));
    let floor = [
        pages.last().filter(|_| pages_full).map(|event| event.time),
        files.last().filter(|_| files_full).map(|event| event.time),
    ]
    .into_iter()
    .flatten()
    .max();
    let all = merged(pages, files)
        .into_iter()
        .filter(|change| floor.is_none_or(|floor| change.time() >= floor))
        .collect();
    (all, pages_full || files_full)
}

/// How many of `shown` are changes to pages, and to files, by `keep`.
fn tally(shown: &[&Change<'_>], keep: impl Fn(&Change<'_>) -> bool) -> (u64, u64) {
    shown
        .iter()
        .filter(|change| keep(change))
        .fold((0, 0), |(pages, files), change| match change {
            Change::Page(_) => (pages + 1, files),
            Change::File(_) => (pages, files + 1),
        })
}

/// The link to the changes before those shown, when there are any. It goes on from the
/// oldest change shown, whatever its day: the second it is in, past the changes of that
/// second already shown, so that a day busier than one read goes on on the next page and
/// nothing is lost or shown twice. `offset` counts the pages' changes shown, for a Mneme
/// that pages by offset alone.
fn older_link(
    shown: &[&Change<'_>],
    pages: &Events,
    files: Option<&FileEvents>,
    offset: u64,
    at: Cursor,
    by_time: bool,
) -> Option<String> {
    let (shown_pages, shown_files) = tally(shown, |_| true);
    let (skipped_pages, skipped_files) = if by_time {
        (at.pages, at.files)
    } else {
        (offset, at.files)
    };
    let more_pages = skipped_pages + shown_pages < pages.total;
    let more_files = files.is_some_and(|files| skipped_files + shown_files < files.total);
    if !(more_pages || more_files) {
        return None;
    }
    let oldest = shown.last().map(|change| change.time())?;
    let (same_pages, same_files) = tally(shown, |change| change.time() == oldest);
    // The changes at the oldest second that a page before this one showed are still to be
    // skipped when this page showed nothing older.
    let (same_pages, same_files) = if at.before == Some(oldest + 1) {
        (same_pages + at.pages, same_files + at.files)
    } else {
        (same_pages, same_files)
    };
    let count = |n: u64| if n == 0 { String::new() } else { n.to_string() };
    Some(href(
        "/wiki/changes",
        &[
            ("before", &(oldest + 1).to_string()),
            ("pages", &count(same_pages)),
            ("files", &count(same_files)),
            ("offset", &(offset + shown_pages).to_string()),
        ],
    ))
}

async fn changes(
    shell: &Shell,
    mneme: &Mneme,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let asked = params(query)?;
    let offset: u64 = choice(asked.offset.as_deref())?.unwrap_or(0);
    let at = Cursor {
        before: choice(asked.before.as_deref())?,
        pages: choice(asked.pages.as_deref())?.unwrap_or(0),
        files: choice(asked.files.as_deref())?.unwrap_or(0),
    };
    let ((found, by_time), titles, _, files) = tokio::try_join!(
        older(mneme, offset, at),
        titles(mneme),
        mneme.status(),
        mneme.file_changes(at.files, READ, None, at.before),
    )?;
    let file_events = files.as_ref().map_or(&[][..], |files| &files.events[..]);
    let (all, full) = whole(&found.events, file_events);
    let days = days(shell, &all, full);
    let shown: Vec<&Change<'_>> = days.iter().flat_map(|day| day.iter()).collect();
    let losers = losers(mneme, &found.events).await;
    let local = shell.local();
    let last = shell.visit().last;
    let is_new = |change: &Change<'_>| last.is_some_and(|last| change.time() > last);
    let new = shown.iter().filter(|change| is_new(change)).count();
    let older = older_link(&shown, &found, files.as_ref(), offset, at, by_time);
    let newer = (offset > 0).then(|| "/wiki/changes".to_owned());
    let look = RowLook {
        day: false,
        diff_link: true,
    };
    let first_new = days.iter().position(|day| day.iter().any(&is_new));
    let body = html! {
        main #(MAIN) .changes-page {
            header.page-head {
                h1 { "Recent changes" }
                p.sub {
                    @if let Some(last) = last {
                        (since(&count(u64::try_from(new).unwrap_or(0), "change", "changes"), local, last))
                    } @else {
                        "Every change to the wiki, newest first."
                    }
                }
            }
            section.days.surface data-surface aria-label="Changes by day" {
                @for (i, day) in days.iter().enumerate() {
                    @let has_new = day.iter().any(&is_new);
                    @let split = day.iter().position(|change| !is_new(change)).filter(|at| has_new && *at > 0);
                    h2.day id=[(first_new == Some(i)).then_some("new")] {
                        (local.time(day[0].time(), &local.day_name(day[0].time())))
                        span { (count(u64::try_from(day.len()).unwrap_or(0), "change", "changes")) }
                    }
                    @match split {
                        Some(at) => {
                            ol.change-list {
                                @for change in &day[..at] { (any_row(local, last, *change, &titles, &losers, look)) }
                            }
                            @if let Some(last) = last {
                                p.since-line #seen { "Last visit, " (local.hm(last)) }
                            }
                            ol.change-list {
                                @for change in &day[at..] { (any_row(local, last, *change, &titles, &losers, look)) }
                            }
                        },
                        None => ol.change-list {
                            @for change in *day { (any_row(local, last, *change, &titles, &losers, look)) }
                        },
                    }
                }
                nav.pager aria-label="Older changes" {
                    span { "Showing " (days.len()) (if days.len() == 1 { " day" } else { " days" }) }
                    span.btns {
                        @if let Some(newer) = &newer { a.btn href=(newer) rel="prev" { "Newest" } }
                        @if let Some(older) = &older { a.btn href=(older) rel="next" { "Older" } }
                    }
                }
            }
            (side(&days, &shown))
        }
    };
    Ok(Page::new(
        "Recent changes, Atrium wiki",
        "Every change to the wiki, by day.",
        body,
    ))
}
