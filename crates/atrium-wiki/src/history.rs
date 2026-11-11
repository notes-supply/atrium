//! `/wiki/page/{id}/history`: every event of one page, newest first, ten at a time. The
//! prototype's `historyView`.

use atrium_ui::components::{delta, pager};
use atrium_ui::words::number;
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use std::collections::HashMap;

use atrium_core::Local;
use maud::{Markup, html};

use crate::mneme::{Event, EventKind, Mneme};
use crate::routes::{
    Failure, Loser, Params, bare, href, losers, page_href, page_id, page_number, params, respond,
};

/// Events per page, as the design shows them.
const PAGE_SIZE: u64 = 10;

pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(id): Path<String>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = history(&shell, &mneme, &id, query).await;
    respond(&shell, &mneme, shown)
}

/// One event: its version, when, what, who, the words, and links to read and compare it.
fn row(
    local: &Local,
    base: &str,
    event: &Event,
    n: u64,
    newest: bool,
    losers: &HashMap<String, Loser>,
) -> Markup {
    let read = if newest {
        base.to_owned()
    } else {
        format!("{base}/at/{}", event.commit)
    };
    let diff = href(&format!("{base}/diff"), &[("to", event.commit.as_str())]);
    html! {
        tr.has-ov[event.overlap.is_some()] {
            td {
                @if event.kind == EventKind::Deleted {
                    span.ttl.ver { "v" (n) }
                } @else {
                    a.ttl.ver href=(read) data-row { "v" (n) }
                }
            }
            td.when data-label="When" {
                (local.time(event.time, &format!("{} {}", local.date_short(event.time), local.hm(event.time))))
            }
            td.kind-cell data-label="Event" {
                (event.kind.word())
                @if let Some(from) = &event.from { small { "from " (bare(from)) } }
                @if let Some(commit) = &event.overlap {
                    small {
                        span.ov-mark aria-hidden="true" {}
                        "overlap with "
                        @match losers.get(commit) {
                            Some(Loser { author, diff: Some(to) }) => a href=(to) { (author) },
                            Some(Loser { author, diff: None }) => (author),
                            None => "another edit",
                        }
                    }
                }
            }
            td.mono data-label="By" { (event.author) }
            td.num data-label="Words" { (number(event.words)) }
            td.num data-label="Changed" {
                @if event.version { (delta(event.added, event.removed)) } @else { span.quiet { "none" } }
            }
            td.acts-cell {
                span.acts {
                    @if event.kind != EventKind::Deleted {
                        a href=(read) { "Read" span.vh { " v" (n) } }
                    }
                    @if event.version {
                        a href=(diff) { "Diff" span.vh { " v" (n.saturating_sub(1)) " to v" (n) } }
                    }
                }
            }
        }
    }
}

async fn history(
    shell: &Shell,
    mneme: &Mneme,
    id: &str,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let id = page_id(id)?;
    let at = page_number(&params(query)?)?;
    let offset = (at - 1) * PAGE_SIZE;
    let (detail, events, _) = tokio::try_join!(
        mneme.page(&id),
        mneme.versions(&id, offset, PAGE_SIZE, None),
        mneme.status(),
    )?;
    let losers = losers(mneme, &events.events).await;
    let local = shell.local();
    let page = &detail.page;
    let base = page_href(&id);
    let pages = events.total.div_ceil(PAGE_SIZE).max(1);
    let first = offset + 1;
    let last = offset + u64::try_from(events.events.len()).unwrap_or(0);
    let link = |at: u64| href(&format!("{base}/history"), &[("page", &at.to_string())]);
    let created = local.date_short(page.created);
    let created_year = if created.ends_with(&local.at(page.created).year().to_string()) {
        created
    } else {
        format!("{created} {}", local.at(page.created).year())
    };
    let body = html! {
        main #(MAIN) .history {
            header.page-head {
                p.crumbs {
                    a href="/wiki" { "Wiki" } " " span aria-hidden="true" { "/" } " "
                    a href=(base) { (page.title) } " " span aria-hidden="true" { "/" } " history"
                }
                h1 { "History of " a.doc-title-link href=(base) { (page.title) } }
                p.sub {
                    (events.total) (if events.total == 1 { " version" } else { " versions" }) " since " (created_year)
                    ". A striped mark means two machines edited the same lines at once."
                }
            }
            section.events.surface data-surface aria-labelledby="ev-h" {
                h2 #ev-h .vh { "Events" }
                table.ledger {
                    caption.vh { "Events " (first) " to " (last) " of " (events.total) ", newest first" }
                    thead {
                        tr {
                            th scope="col" { "Version" }
                            th scope="col" { "When" }
                            th scope="col" { "Event" }
                            th scope="col" { "By" }
                            th.num scope="col" { "Words" }
                            th.num scope="col" { "Changed" }
                            th scope="col" { span.vh { "Actions" } }
                        }
                    }
                    tbody {
                        @for (i, event) in events.events.iter().enumerate() {
                            @let n = events.total - offset - u64::try_from(i).unwrap_or(0);
                            (row(local, &base, event, n, offset == 0 && i == 0, &losers))
                        }
                    }
                }
                (pager(
                    "Pages of events",
                    &format!("Events {first}–{last} of {}, page {at} of {pages}", events.total),
                    (at > 1).then(|| link(at - 1)).as_deref(),
                    (at < pages).then(|| link(at + 1)).as_deref(),
                ))
            }
        }
    };
    Ok(Page::new(
        format!("History of {}, Atrium wiki", page.title),
        "Every event of one page, newest first.",
        body,
    ))
}
