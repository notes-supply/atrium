//! What the browser modules read that the pages do not carry, since a page may hold no
//! inline script: the palette's list of pages, and a page's preview for a link to it.

use atrium_ui::Shell;
use atrium_ui::words::number;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::markdown;
use crate::mneme::{Filters, Listing, Mneme, MnemeError, Sort};
use crate::routes::{bare, page_href};

/// Pages the palette lists, at most.
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
/// `[{ "t": title, "path", "m": when, "href" }]`.
pub async fn index(shell: Shell, State(mneme): State<Mneme>) -> Response {
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
        let found = match mneme.pages(listing).await {
            Ok(found) => found,
            Err(error) => return failed(&error),
        };
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
    answer(StatusCode::OK, &Value::Array(pages))
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
