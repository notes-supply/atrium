//! Links written as a page's file, `/wiki/{path}.md`, as pages link to each other. A path is
//! looked up among the pages now, then among deleted pages and the paths renamed pages had,
//! so an old link still lands. The prototype's `notFound` shows what the record remembers.

use atrium_ui::Shell;
use axum::extract::{Path, State};
use axum::response::Response;
use maud::html;

use crate::markdown;
use crate::mneme::{EventKind, Filters, Listing, Mneme, MnemeError, Page, PageId, Sort};
use crate::routes::{Failure, bare, page_href, redirect, respond};

/// Mneme's largest page of a listing.
const STEP: u64 = 500;
/// Changes read to find a path a page was renamed away from, at most.
const RENAMES: u64 = 4000;

pub async fn by_path(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(path): Path<String>,
) -> Response {
    if !markdown::is_page(&path) {
        return lost(&shell);
    }
    found_by_path(&shell, &mneme, &path).await
}

/// The not-found page, with the wiki's own ways on.
pub fn lost(shell: &Shell) -> Response {
    shell.lost(
        None,
        &[("Recent changes".to_owned(), "/wiki/changes".to_owned())],
    )
}

/// Every page, deleted ones too, until `wanted` finds one.
async fn find_page(
    mneme: &Mneme,
    wanted: impl Fn(&Page) -> bool,
) -> Result<Option<Page>, MnemeError> {
    let mut offset = 0;
    loop {
        let listing = Listing {
            sort: Some(Sort::Title),
            order: None,
            offset,
            limit: STEP,
            filters: Filters {
                deleted: true,
                ..Filters::default()
            },
        };
        let found = mneme.pages(listing).await?;
        if let Some(page) = found.pages.into_iter().find(|page| wanted(page)) {
            return Ok(Some(page));
        }
        offset += STEP;
        if offset >= found.total {
            return Ok(None);
        }
    }
}

/// The page a rename moved away from `path`, newest rename first.
async fn renamed_from(mneme: &Mneme, path: &str) -> Result<Option<PageId>, MnemeError> {
    let mut offset = 0;
    while offset < RENAMES {
        let found = mneme.changes(offset, STEP, None).await?;
        let renamed = found
            .events
            .iter()
            .find(|event| event.kind == EventKind::Renamed && event.from.as_deref() == Some(path));
        if let Some(event) = renamed {
            return Ok(Some(event.page.clone()));
        }
        offset += STEP;
        if found.events.is_empty() || offset >= found.total {
            break;
        }
    }
    Ok(None)
}

enum Found {
    Here(PageId),
    Gone(Page),
    Nothing,
}

async fn look_up(mneme: &Mneme, path: &str) -> Result<Found, MnemeError> {
    let live = find_page(mneme, |page| page.path == path && page.deleted.is_none());
    if let Some(page) = live.await? {
        return Ok(Found::Here(page.id));
    }
    if let Some(page) = find_page(mneme, |page| page.path == path).await? {
        return Ok(Found::Gone(page));
    }
    let Some(id) = renamed_from(mneme, path).await? else {
        return Ok(Found::Nothing);
    };
    match find_page(mneme, |page| page.id == id).await? {
        Some(page) if page.deleted.is_some() => Ok(Found::Gone(page)),
        Some(page) => Ok(Found::Here(page.id)),
        None => Ok(Found::Nothing),
    }
}

/// Sends a link written as a file's path to the page that is there now, or that was there.
pub async fn found_by_path(shell: &Shell, mneme: &Mneme, path: &str) -> Response {
    let found = match look_up(mneme, path).await {
        Ok(found) => found,
        Err(error) => return respond(shell, mneme, Err(Failure::from(error))),
    };
    match found {
        Found::Here(id) => redirect(&page_href(&id)),
        Found::Nothing => lost(shell),
        Found::Gone(page) => {
            let local = shell.local();
            let deletion = mneme
                .versions(&page.id, 0, 1, None)
                .await
                .ok()
                .and_then(|events| {
                    events
                        .events
                        .into_iter()
                        .find(|event| event.kind == EventKind::Deleted)
                });
            let gone = html! {
                p.diff-sum {
                    "deleted "
                    @if let Some(deleted) = page.deleted { (local.when_long(deleted)) }
                    @if let Some(event) = &deletion { " by " (event.author) }
                }
                p { a.ttl-gone href=(page_href(&page.id)) { (page.title) } ", at " code { (bare(&page.path)) } "." }
            };
            shell.lost(
                Some(gone),
                &[("Recent changes".to_owned(), "/wiki/changes".to_owned())],
            )
        }
    }
}
