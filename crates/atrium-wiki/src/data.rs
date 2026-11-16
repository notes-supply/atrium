//! What the browser modules read that the pages do not carry, since a page may hold no
//! inline script: the palette's list of pages, and a page's preview for a link to it.

use atrium_ui::Shell;
use atrium_ui::words::{bytes, number};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mneme::{FileListing, FileSort, Filters, Listing, Mneme, MnemeError, Sort};
use crate::routes::{bare, page_href};
use crate::{kind, markdown};

/// Pages, and files, the palette lists, at most.
const MOST: u64 = 2000;

fn answer(status: StatusCode, body: &Value) -> Response {
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn failed(error: &MnemeError) -> Response {
    let status = match error {
        MnemeError::NotFound => StatusCode::NOT_FOUND,
        MnemeError::Refused(_) => StatusCode::BAD_REQUEST,
        MnemeError::Away(_) => StatusCode::SERVICE_UNAVAILABLE,
        MnemeError::Broken(_) => StatusCode::BAD_GATEWAY,
    };
    answer(status, &json!({ "error": error.to_string() }))
}

/// `GET /wiki/index.json`: every live page, newest first, as the palette lists them:
/// `[{ "t": title, "path", "m": when, "href" }]`; then, from a Mneme that lists files, the
/// files section and every live file, newest first, each with `"g"` its group, `section` or
/// `file`, and the file's kind and size in `m`.
pub async fn index(shell: Shell, State(mneme): State<Mneme>) -> Response {
    let (pages, files) = tokio::join!(pages(&shell, &mneme), files(&mneme));
    match (pages, files) {
        (Ok(mut pages), Ok(files)) => {
            if let Some(files) = files {
                pages.push(json!({
                    "t": "Files",
                    "path": "",
                    "m": "",
                    "href": "/wiki/files",
                    "g": "section",
                }));
                pages.extend(files);
            }
            answer(StatusCode::OK, &Value::Array(pages))
        }
        (Err(error), _) | (_, Err(error)) => failed(&error),
    }
}

/// Every live file, newest first, up to [`MOST`], as the palette lists them, or `None` from a
/// Mneme that lists none.
async fn files(mneme: &Mneme) -> Result<Option<Vec<Value>>, MnemeError> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset < MOST {
        let listing = FileListing {
            under: None,
            words: None,
            sort: Some(FileSort::Updated),
            order: None,
            offset,
            limit: 500,
        };
        let Some(files) = mneme.files(listing).await? else {
            return Ok(None);
        };
        let done = files.files.len() < 500;
        found.extend(files.files.into_iter().map(|file| {
            json!({
                "t": kind::name(&file.path),
                "path": file.path,
                "m": format!("{} · {}", kind::label(&file.path), bytes(file.size)),
                "href": kind::page_href(&file.path),
                "g": "file",
            })
        }));
        offset += 500;
        if done || offset >= files.total {
            break;
        }
    }
    Ok(Some(found))
}

/// Every live page, newest first, up to [`MOST`].
async fn pages(shell: &Shell, mneme: &Mneme) -> Result<Vec<Value>, MnemeError> {
    let local = shell.local();
    let mut pages = Vec::new();
    let mut offset = 0;
    while offset < MOST {
        let listing = Listing {
            sort: Some(Sort::Updated),
            order: None,
            offset,
            limit: 500,
            filters: Filters::default(),
        };
        let found = mneme.pages(listing).await?;
        let done = found.pages.len() < 500;
        pages.extend(found.pages.into_iter().map(|page| {
            json!({
                "t": page.title,
                "path": bare(&page.path),
                "m": local.when(page.updated),
                "href": page_href(&page.id),
            })
        }));
        offset += 500;
        if done || offset >= found.total {
            break;
        }
    }
    Ok(pages)
}

#[derive(Debug, Deserialize)]
pub struct PreviewOf {
    /// A link as a page carries it: `/wiki/page/{id}` or `/wiki/{path}.md`.
    href: String,
}

/// `GET /wiki/preview.json?href=`: what a link to another page previews: `{ "title", "path",
/// "text", "meta" }`, where `text` is the page's first paragraph.
pub async fn preview(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<PreviewOf>, QueryRejection>,
) -> Response {
    let Ok(Query(PreviewOf { href })) = query else {
        return answer(
            StatusCode::BAD_REQUEST,
            &json!({ "error": "name the link in href" }),
        );
    };
    let id = match href.strip_prefix("/wiki/page/").map(str::parse) {
        Some(Ok(id)) => Some(id),
        Some(Err(_)) => None,
        None => {
            let path = href.strip_prefix("/wiki/").unwrap_or(&href);
            let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
            match crate::lost::page_at(&mneme, &path).await {
                Ok(id) => id,
                Err(error) => return failed(&error),
            }
        }
    };
    let Some(id) = id else {
        return failed(&MnemeError::NotFound);
    };
    let detail = match mneme.page(&id).await {
        Ok(detail) => detail,
        Err(error) => return failed(&error),
    };
    let page = detail.page;
    let local = shell.local();
    answer(
        StatusCode::OK,
        &json!({
            "title": page.title,
            "path": bare(&page.path),
            "text": detail.markdown.as_deref().map_or_else(String::new, markdown::first_paragraph),
            "meta": format!("{} words, v{}, updated {}", number(page.words), page.versions, local.when(page.updated)),
        }),
    )
}
