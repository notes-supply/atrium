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

use crate::mneme::{Event, Events, Mneme, MnemeError};
use crate::routes::{
    Failure, Params, RowLook, change_row, choice, href, losers, params, respond, title_of, titles,
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
/// the last day may be missing some of its events, so it is left for the next page.
fn days<'a>(shell: &Shell, events: &'a [Event], full: bool) -> Vec<&'a [Event]> {
    let local = shell.local();
    let mut days: Vec<&[Event]> = events
        .chunk_by(|a, b| local.at(a.time).date() == local.at(b.time).date())
        .take(DAYS + 1)
        .collect();
    if days.len() > DAYS || (full && days.len() > 1) {
        days.pop();
    }
    days
}

/// What the days shown hold: how many changes and pages, the words, and who wrote them.
fn side(days: &[&[Event]], shown: &[&Event]) -> Markup {
    let mut authors: Vec<&str> = Vec::new();
    for event in shown {
        if !authors.contains(&event.author.as_str()) {
            authors.push(&event.author);
        }
    }
    let pages: HashSet<_> = shown.iter().map(|event| &event.page).collect();
    let added: u64 = shown.iter().map(|event| event.added).sum();
    let removed: u64 = shown.iter().map(|event| event.removed).sum();
    html! {
        aside.changes-side.surface data-surface aria-labelledby="side-h" {
            h2 #side-h .rail-h { "These " (count(u64::try_from(days.len()).unwrap_or(0), "day", "days")) }
            dl.side-stats {
                div { dt { "Changes" } dd { (shown.len()) } }
                div { dt { "Pages" } dd { (pages.len()) } }
                div { dt { "Words added" } dd.a { (number(added)) } }
                div { dt { "Words removed" } dd { (number(removed)) } }
            }
            p.diff-sum {
                "By " (authors.join(", ")) ". Machines save their own changes; a name is someone who committed by hand."
            }
        }
    }
}

/// The events from `before` back, when the address says where the older days start and
/// Mneme pages by time, and whether it did; otherwise from `offset`.
async fn older(
    mneme: &Mneme,
    offset: u64,
    before: Option<i64>,
) -> Result<(Events, bool), MnemeError> {
    if let Some(before) = before
        && let Some(found) = mneme.changes_before(before, READ).await?
    {
        return Ok((found, true));
    }
    Ok((mneme.changes(offset, READ, None).await?, false))
}

async fn changes(
    shell: &Shell,
    mneme: &Mneme,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let asked = params(query)?;
    let offset: u64 = choice(asked.offset.as_deref())?.unwrap_or(0);
    let before: Option<i64> = choice(asked.before.as_deref())?;
    let ((found, by_time), titles, _) =
        tokio::try_join!(older(mneme, offset, before), titles(mneme), mneme.status(),)?;
    let full = u64::try_from(found.events.len()).unwrap_or(0) == READ;
    let days = days(shell, &found.events, full);
    let shown: Vec<&Event> = days.iter().flat_map(|day| day.iter()).collect();
    let losers = losers(mneme, &found.events).await;
    let local = shell.local();
    let last = shell.visit().last;
    let is_new = |event: &Event| last.is_some_and(|last| event.time > last);
    let new = shown.iter().filter(|event| is_new(event)).count();
    let shown_count = u64::try_from(shown.len()).unwrap_or(0);
    let skipped = if by_time { 0 } else { offset };
    // The older days start before the oldest day shown, which is whole: `days` leaves out a
    // day the read may have cut.
    let oldest_day = shown.last().and_then(|event| {
        local
            .at(event.time)
            .start_of_day()
            .ok()
            .map(|start| start.timestamp().as_second())
    });
    let older = (skipped + shown_count < found.total).then(|| {
        href(
            "/wiki/changes",
            &[
                (
                    "before",
                    &oldest_day.map_or_else(String::new, |at| at.to_string()),
                ),
                ("offset", &(offset + shown_count).to_string()),
            ],
        )
    });
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
                    @let split = day.iter().position(|event| !is_new(event)).filter(|at| has_new && *at > 0);
                    h2.day id=[(first_new == Some(i)).then_some("new")] {
                        (local.time(day[0].time, &local.day_name(day[0].time)))
                        span { (count(u64::try_from(day.len()).unwrap_or(0), "change", "changes")) }
                    }
                    @match split {
                        Some(at) => {
                            ol.change-list {
                                @for event in &day[..at] { (change_row(local, last, event, title_of(&titles, event), &losers, look)) }
                            }
                            @if let Some(last) = last {
                                p.since-line #seen { "Last visit, " (local.hm(last)) }
                            }
                            ol.change-list {
                                @for event in &day[at..] { (change_row(local, last, event, title_of(&titles, event), &losers, look)) }
                            }
                        },
                        None => ol.change-list {
                            @for event in *day { (change_row(local, last, event, title_of(&titles, event), &losers, look)) }
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
