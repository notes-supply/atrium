//! Links written as a page's file, `/wiki/{path}.md`, as pages link to each other. Mneme
//! finds the page at a path now or before a rename, so an old link still lands. It leaves
//! deleted pages out, so a path it does not know is looked for among them, and the
//! prototype's `notFound` shows what the record remembers.

use atrium_ui::Shell;
use axum::extract::{Path, State};
use axum::response::Response;
use maud::html;

use crate::markdown;
use crate::mneme::{EventKind, Filters, Listing, Mneme, MnemeError, Page, PageId, Sort};
use crate::routes::{Failure, bare, page_href, redirect, respond};

/// Mneme's largest page of a listing.
const STEP: u64 = 500;

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

/// The deleted page that was at `path`, read from every page, deleted ones too.
async fn deleted_at(mneme: &Mneme, path: &str) -> Result<Option<Page>, MnemeError> {
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
        if let Some(page) = found
            .pages
            .into_iter()
            .find(|page| page.path == path && page.deleted.is_some())
        {
            return Ok(Some(page));
        }
        offset += STEP;
        if offset >= found.total {
            return Ok(None);
        }
    }
}

/// The page at `path` now, or the one a rename moved away from it.
pub async fn page_at(mneme: &Mneme, path: &str) -> Result<Option<PageId>, MnemeError> {
    Ok(mneme.path(path).await?.map(|found| found.id))
}

enum Found {
    Here(PageId),
    Gone(Page),
    Nothing,
}

async fn look_up(mneme: &Mneme, path: &str) -> Result<Found, MnemeError> {
    if let Some(id) = page_at(mneme, path).await? {
        return Ok(Found::Here(id));
    }
    Ok(deleted_at(mneme, path)
        .await?
        .map_or(Found::Nothing, Found::Gone))
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
