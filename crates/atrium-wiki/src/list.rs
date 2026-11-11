//! `/wiki`: every page, with sort and order. The prototype's `wikiList`.

use atrium_ui::components::pager;
use atrium_ui::words::number;
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::Response;
use maud::{Markup, html};

use crate::mneme::{Filters, Listing, Mneme, Order, Sort};
use crate::routes::{Failure, Params, bare, choice, href, page_href, page_number, params, respond};

/// Pages per page of the list, as the design shows them.
const PAGE_SIZE: u64 = 16;

/// The columns, in order: the sort each heads, its heading and its class.
const COLUMNS: [(Sort, &str, &str); 6] = [
    (Sort::Title, "Page", ""),
    (Sort::Words, "Words", "num"),
    (Sort::Versions, "Versions", "num"),
    (Sort::Churn, "Changed", "num hide-s"),
    (Sort::Updated, "Updated", ""),
    (Sort::Created, "Created", "hide-s"),
];

/// How the list reads its sort: "by last update".
const fn by(sort: Sort) -> &'static str {
    match sort {
        Sort::Title => "by title",
        Sort::Updated => "by last update",
        Sort::Created => "by creation",
        Sort::Versions => "by versions",
        Sort::Words => "by length",
        Sort::Churn => "by words changed",
    }
}

/// The two orders as a reader names them for a sort: newest first, A to Z.
const fn orders(sort: Sort) -> [(Order, &'static str); 2] {
    match sort {
        Sort::Title => [(Order::Asc, "A to Z"), (Order::Desc, "Z to A")],
        Sort::Updated | Sort::Created => {
            [(Order::Desc, "Newest first"), (Order::Asc, "Oldest first")]
        }
        Sort::Versions | Sort::Words | Sort::Churn => {
            [(Order::Desc, "Most first"), (Order::Asc, "Fewest first")]
        }
    }
}

pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = list(&shell, &mneme, query).await;
    respond(&shell, &mneme, shown)
}

/// The list's address sorted by `by` in `order`, at page `at`.
fn sorted(by: Sort, order: Order, at: u64) -> String {
    let page = if at > 1 {
        at.to_string()
    } else {
        String::new()
    };
    href(
        "/wiki",
        &[
            ("sort", by.name()),
            ("order", order.name()),
            ("page", &page),
        ],
    )
}

/// The column heads, each a link that sorts by its column, the current one reversing it.
fn head(sort: Sort, order: Order) -> Markup {
    html! {
        thead {
            tr {
                @for (column, label, class) in COLUMNS {
                    @let current = column == sort;
                    th scope="col" class=[(!class.is_empty()).then_some(class)]
                        aria-sort=[current.then_some(if order == Order::Asc { "ascending" } else { "descending" })]
                    {
                        a.sorter href=(sorted(column, if current { order.flip() } else { column.default_order() }, 1)) { (label) }
                    }
                }
            }
        }
    }
}

async fn list(
    shell: &Shell,
    mneme: &Mneme,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let params = params(query)?;
    let sort = choice(params.sort.as_deref())?.unwrap_or(Sort::Updated);
    let order = choice(params.order.as_deref())?.unwrap_or(sort.default_order());
    let at = page_number(&params)?;
    let listing = Listing {
        sort: Some(sort),
        order: Some(order),
        offset: (at - 1) * PAGE_SIZE,
        limit: PAGE_SIZE,
        filters: Filters::default(),
    };
    let (found, status) = tokio::try_join!(mneme.pages(listing), mneme.status())?;
    let local = shell.local();
    let pages = found.total.div_ceil(PAGE_SIZE).max(1);
    let first = listing.offset + 1;
    let last = listing.offset + u64::try_from(found.pages.len()).unwrap_or(0);
    let order_words = orders(sort)
        .iter()
        .find(|(o, _)| *o == order)
        .map_or("", |(_, words)| *words)
        .to_lowercase();
    let body = html! {
        main #(MAIN) .wiki-index {
            header.page-head {
                h1 { "The wiki" }
                p.sub {
                    (number(status.pages)) " pages and " (number(status.commits)) " commits on the record."
                    @if let Some(newest) = status.head_time { " The newest change was " (local.when_long(newest)) "." }
                }
            }
            section.wiki-list.surface data-surface aria-labelledby="list-h" {
                h2 #list-h .vh { "All pages" }
                div.list-bar {
                    p.find-count {
                        "Pages " b { (first) "–" (last) } " of " (number(found.total)) ", " (by(sort))
                    }
                    nav.order aria-label="Order" {
                        span.order-label { "Order" }
                        @for (each, label) in orders(sort) {
                            a.seg href=(sorted(sort, each, 1)) aria-current=[(each == order).then_some("true")] { (label) }
                        }
                    }
                }
                table.ledger {
                    caption.vh { "All pages, sorted " (by(sort)) ", " (order_words) ". Column heads change the sort." }
                    (head(sort, order))
                    tbody {
                        @for page in &found.pages {
                            tr {
                                td {
                                    a.ttl href=(page_href(&page.id)) data-row data-carry { (page.title) }
                                    span.pth { (bare(&page.path)) }
                                }
                                td.num data-label="Words" { (number(page.words)) }
                                td.num data-label="Versions" { (page.versions) }
                                td.num.hide-s data-label="Changed" { (number(page.churn)) }
                                td.when data-label="Updated" { (local.time(page.updated, &local.when(page.updated))) }
                                td.when.hide-s data-label="Created" { (local.time(page.created, &local.date_short(page.created))) }
                            }
                        }
                    }
                }
                (pager(
                    "Pages of the list",
                    &format!("Page {at} of {pages}"),
                    (at > 1).then(|| sorted(sort, order, at - 1)).as_deref(),
                    (at < pages).then(|| sorted(sort, order, at + 1)).as_deref(),
                ))
            }
        }
    };
    Ok(Page::new(
        "Wiki, Atrium",
        "Every page of the wiki, with sort and order.",
        body,
    ))
}
