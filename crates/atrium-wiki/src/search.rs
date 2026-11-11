//! `/wiki/search`: the form, its filters and sort, results with snippets, and the empty and
//! no-result states. The prototype's `searchView`.

use atrium_ui::components::pager;
use atrium_ui::words::{count, number};
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::Response;
use maud::{Markup, html};

use crate::mneme::{Filters, Listing, Mneme, Page as WikiPage, Run, Sort};
use crate::routes::{Failure, Params, bare, choice, href, page_href, page_number, params, respond};

/// Results per page.
const PAGE_SIZE: u64 = 50;

const SORTS: [(&str, &str); 7] = [
    ("relevance", "Relevance"),
    ("updated", "Updated"),
    ("created", "Created"),
    ("versions", "Versions"),
    ("words", "Words"),
    ("churn", "Churn"),
    ("title", "Title"),
];

const WITHIN: [(&str, &str); 5] = [
    ("", "Any time"),
    ("1", "A day"),
    ("7", "7 days"),
    ("30", "30 days"),
    ("90", "90 days"),
];

pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = find(&shell, &mneme, query).await;
    respond(&shell, &mneme, shown)
}

/// What the form holds, as the address gave it.
struct Asked {
    words: String,
    /// The sort's name, `relevance` for the best match first.
    sort: String,
    /// The sort Mneme is asked for, `None` for the best match first.
    chosen: Option<Sort>,
    filters: Filters,
    raw: [(&'static str, String); 6],
}

impl Asked {
    fn read(params: &Params) -> Result<Self, Failure> {
        let sort = params
            .sort
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "relevance".to_owned());
        let chosen = if sort == "relevance" {
            None
        } else {
            Some(sort.parse().map_err(|_| Failure::BadQuery)?)
        };
        let raw = |value: &Option<String>| value.clone().unwrap_or_default();
        Ok(Self {
            words: params.q.as_deref().unwrap_or_default().trim().to_owned(),
            sort,
            chosen,
            filters: Filters {
                deleted: false,
                changed_within_days: choice(params.changed.as_deref())?,
                created_within_days: choice(params.created.as_deref())?,
                min_versions: choice(params.vmin.as_deref())?,
                max_versions: choice(params.vmax.as_deref())?,
                min_words: choice(params.wmin.as_deref())?,
                max_words: choice(params.wmax.as_deref())?,
            },
            raw: [
                ("changed", raw(&params.changed)),
                ("created", raw(&params.created)),
                ("vmin", raw(&params.vmin)),
                ("vmax", raw(&params.vmax)),
                ("wmin", raw(&params.wmin)),
                ("wmax", raw(&params.wmax)),
            ],
        })
    }
}

impl Asked {
    /// The sort as the count reads it: "by relevance".
    fn label(&self) -> String {
        SORTS
            .iter()
            .find(|(v, _)| *v == self.sort)
            .map_or("relevance", |(_, l)| l)
            .to_lowercase()
    }

    /// The same search at page `at` of its results.
    fn href(&self, at: u64) -> String {
        let page = at.to_string();
        let sort = if self.chosen.is_some() {
            self.sort.as_str()
        } else {
            ""
        };
        let mut pairs: Vec<(&str, &str)> = vec![("q", &self.words), ("sort", sort)];
        pairs.extend(self.raw.iter().map(|(n, v)| (*n, v.as_str())));
        pairs.push(("page", &page));
        href("/wiki/search", &pairs)
    }
}

fn tips() -> Markup {
    html! {
        aside.search-side.surface data-surface aria-labelledby="tips-h" {
            h2 #tips-h .rail-h { "Searching" }
            p { "Results update as you type and the address keeps the search, so a search can be bookmarked. Without scripts the form submits as usual." }
            p.cmd-hint {
                span { kbd { "/" } " search" }
                span { kbd { "j" } " " kbd { "k" } " results" }
                span { kbd { "Enter" } " open" }
            }
        }
    }
}

fn select(id: &str, label: &str, chosen: &str, options: &[(&str, &str)]) -> Markup {
    html! {
        div.field-row {
            label for=(id) { (label) }
            select.select #(id) name=(id) {
                @for (value, text) in options {
                    option value=(value) selected[*value == chosen] { (text) }
                }
            }
        }
    }
}

fn range(
    legend: &str,
    low: (&str, &str, &str, &str),
    high: (&str, &str),
    step: Option<&str>,
    raw: &[(&'static str, String); 6],
) -> Markup {
    let value = |name: &str| {
        raw.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    html! {
        fieldset.fieldset {
            legend { (legend) }
            div.range {
                input.num-in type="number" name=(low.0) min=(low.1) step=[step] placeholder=(low.2) aria-label=(low.3) value=[value(low.0)];
                span { "to" }
                input.num-in type="number" name=(high.0) min=(low.1) step=[step] placeholder="any" aria-label=(high.1) value=[value(high.0)];
            }
        }
    }
}

/// The search form, with its filters open when `open`. `prefix` keeps ids unique where a page
/// shows the form twice.
fn form(asked: &Asked, open: bool) -> Markup {
    let set = asked.filters.count();
    let raw = &asked.raw;
    let chosen = |name: &str| {
        raw.iter()
            .find(|(n, _)| *n == name)
            .map_or("", |(_, v)| v.as_str())
            .to_owned()
    };
    html! {
        form.find-form action="/wiki/search" method="get" role="search" aria-label="Search the wiki" data-module="swap" {
            div.find-row {
                label.vh for="q" { "Search words" }
                input #q name="q" type="search" value=(asked.words) autocomplete="off" spellcheck="false" aria-describedby="find-count";
                button.btn type="submit" { "Search" }
            }
            details.filters open[open || set > 0] {
                summary { "Filters " b { @if set == 0 { "none set" } @else { (count(u64::try_from(set).unwrap_or(0), "set", "set")) } } }
                div.filter-grid {
                    (select("changed", "Changed within", &chosen("changed"), &WITHIN))
                    (select("created", "Created within", &chosen("created"), &WITHIN))
                    (range("Versions", ("vmin", "1", "1", "At least this many versions"), ("vmax", "At most this many versions"), None, raw))
                    (range("Words", ("wmin", "0", "0", "At least this many words"), ("wmax", "At most this many words"), Some("50"), raw))
                    (select("sort", "Sort by", &asked.sort, &SORTS))
                }
            }
        }
    }
}

fn empty_state(pages: u64) -> Markup {
    html! {
        div.search-state data-state="empty" {
            h2 { "Search " (number(pages)) " pages" }
            p { "Type a word or two. Titles count more than text. Filters narrow by age, versions and length; the sort can put the most changed pages first." }
            ul {
                li { a href="/wiki/search?q=&sort=updated" { "Every page, newest first" } }
                li { a href="/wiki/search?q=&sort=churn" { "The pages that change most" } }
                li { a href="/wiki/search?q=&created=7" { "Pages created this week" } }
            }
        }
    }
}

fn none_state(words: &str) -> Markup {
    html! {
        div.search-state data-state="none" {
            h2 { "Nothing matches " q { (words) } }
            p { "No page title or text has that. Try fewer words, a shorter form of the word, or clear the filters." }
            ul {
                li { a href="/wiki/search" { "Clear the search" } }
                li { a href="/wiki/changes" { "Browse recent changes" } }
            }
        }
    }
}

fn result(shell: &Shell, page: &WikiPage, snippet: Option<&[Run]>) -> Markup {
    let local = shell.local();
    html! {
        li {
            h3 { a href=(page_href(&page.id)) data-row data-carry { (page.title) } }
            p.path { (bare(&page.path)) }
            @if let Some(runs) = snippet {
                p.snip { @for run in runs { @if run.hit { mark { (run.text) } } @else { (run.text) } } }
            }
            p.rmeta {
                span { "Updated " (local.time(page.updated, &local.when(page.updated))) }
                span { (number(page.words)) " words" }
                span { "v" (page.versions) }
            }
        }
    }
}

async fn find(
    shell: &Shell,
    mneme: &Mneme,
    query_params: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let params = params(query_params)?;
    let asked = Asked::read(&params)?;
    let (words, sort, filters) = (asked.words.clone(), asked.chosen, asked.filters);
    let at = page_number(&params)?;
    let listing = Listing {
        sort,
        order: None,
        offset: (at - 1) * PAGE_SIZE,
        limit: PAGE_SIZE,
        filters,
    };
    let browsing = words.is_empty() && (sort.is_some() || filters.count() > 0);
    let status = mneme.status();
    let (status, found) = if !words.is_empty() {
        let (status, hits) = tokio::try_join!(status, mneme.search(&words, listing))?;
        let results: Vec<_> = hits
            .hits
            .into_iter()
            .map(|hit| (hit.page, Some(hit.snippet)))
            .collect();
        (status, Some((hits.total, results)))
    } else if browsing {
        let (status, pages) = tokio::try_join!(status, mneme.pages(listing))?;
        (
            status,
            Some((
                pages.total,
                pages.pages.into_iter().map(|page| (page, None)).collect(),
            )),
        )
    } else {
        (status.await?, None)
    };
    let label = asked.label();
    let pages = found
        .as_ref()
        .map_or(1, |(total, _)| total.div_ceil(PAGE_SIZE).max(1));
    let link = |at: u64| asked.href(at);
    let body = html! {
        main #(MAIN) .search {
            section.find.surface data-surface aria-labelledby="find-h" {
                h1 #find-h .label { "Search the wiki" }
                (form(&asked, false))
                div.find-bar {
                    p #find-count .find-count aria-live="polite" data-count {
                        @match &found {
                            Some((total, _)) if !words.is_empty() => {
                                b { (number(*total)) } (if *total == 1 { " page matches " } else { " pages match " }) q { (words) } ", by " (label)
                            },
                            Some((total, _)) => { b { (number(*total)) } (if *total == 1 { " page" } else { " pages" }) ", by " (label) },
                            None => "Type to search every page.",
                        }
                    }
                }
            }
            section.results.surface data-surface aria-labelledby="results-h" data-swap-target {
                h2 #results-h .vh { "Results" }
                @match &found {
                    None => (empty_state(status.pages)),
                    Some((_, results)) if results.is_empty() => (none_state(&words)),
                    Some((_, results)) => {
                        ol.result-list { @for (page, snippet) in results { (result(shell, page, snippet.as_deref())) } }
                        @if pages > 1 {
                            (pager("Pages of results", &format!("Page {at} of {pages}"),
                                (at > 1).then(|| link(at - 1)).as_deref(),
                                (at < pages).then(|| link(at + 1)).as_deref()))
                        }
                    },
                }
            }
            (tips())
        }
    };
    let title = if words.is_empty() {
        "Search the wiki, Atrium".to_owned()
    } else {
        format!("{words}, search, Atrium")
    };
    Ok(Page::new(
        title,
        "Search with snippets, filters and sorting.",
        body,
    ))
}
