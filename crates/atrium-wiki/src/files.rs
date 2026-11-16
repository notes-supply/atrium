//! Files that are not pages, such as images, PDFs and zips: `/wiki/files`, the list by
//! folder; `/wiki/files/{path}`, one file with its preview and every version; and
//! `/wiki/raw/{path}`, its bytes, streamed from Mneme's `/v1/raw` under the wiki's access
//! rules with Mneme's own headers, its policy for the file among them.

use std::cmp::Reverse;
use std::io;
use std::time::Duration;

use atrium_ui::components::pager;
use atrium_ui::words::{bytes, count, number};
use atrium_ui::{MAIN, Page, Shell};
use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::Response;
use maud::{Markup, html};
use serde::Deserialize;

use crate::kind::{self, Shown};
use crate::markdown::{FileLinks, Known, KnownFile};
use crate::mneme::{
    CommitId, EventKind, File, FileDetail, FileEvent, FileListing, FileSort, Mneme, MnemeError,
    Order,
};
use crate::routes::{Failure, Params, choice, href, page_number, params, respond};
use crate::{lost, markdown};

/// Rows per page of the list, as the pages list shows them.
const PAGE_SIZE: u64 = 16;
/// Events per page of a file's history.
const EVENTS: u64 = 20;
/// Mneme's largest page of a listing.
const STEP: u64 = 500;
/// Files a folder's list reads to find its folders, at most.
const MOST: u64 = 2000;
/// The bytes of a text file its page shows.
const TEXT_START: usize = 4096;
/// How long a download waits for Mneme's next bytes once the browser is ready for them.
const STALL: Duration = Duration::from_secs(30);

/// The policy for a file's bytes when Mneme sends none: Mneme's own for every kind but PDF.
/// Nothing may load, no script may run, and the file has an origin of its own.
pub const SANDBOX: &str = "default-src 'none'; style-src 'unsafe-inline'; sandbox";

/// Mneme's headers a download passes back to the browser as they are.
const PASSED: [HeaderName; 9] = [
    ETAG,
    CONTENT_RANGE,
    ACCEPT_RANGES,
    CONTENT_LENGTH,
    CONTENT_TYPE,
    CONTENT_DISPOSITION,
    CACHE_CONTROL,
    X_CONTENT_TYPE_OPTIONS,
    CONTENT_SECURITY_POLICY,
];

#[derive(Debug, Deserialize)]
pub struct At {
    at: Option<String>,
}

/// `GET` or `HEAD /wiki/raw/{path}[?at=]`: the file's bytes, streamed from Mneme as the
/// browser reads them. A link written as a page's file, `/wiki/raw/{path}.md`, is a link to
/// a page in a folder named `raw`.
pub async fn raw(
    shell: Shell,
    State(mneme): State<Mneme>,
    method: Method,
    Path(path): Path<String>,
    query: Result<Query<At>, QueryRejection>,
    headers: HeaderMap,
) -> Response {
    if markdown::is_page(&path) {
        return lost::found_by_path(&shell, &mneme, &format!("raw/{path}")).await;
    }
    let at = match query {
        Ok(Query(At { at: None })) => None,
        Ok(Query(At { at: Some(at) })) => match at.parse::<CommitId>() {
            Ok(at) => Some(at),
            Err(_) => return respond(&shell, &mneme, Err(Failure::BadQuery)),
        },
        Err(_) => return respond(&shell, &mneme, Err(Failure::BadQuery)),
    };
    let head = method == Method::HEAD;
    let method = if head { Method::HEAD } else { Method::GET };
    match mneme.raw(&path, at.as_ref(), method, &headers).await {
        Ok(answer) => relay(answer, head),
        Err(MnemeError::NotFound) => missing(&shell, &mneme, &path).await,
        Err(error) => respond(&shell, &mneme, Err(error.into())),
    }
}

/// Mneme's answer for a file as the browser's: its status, the headers in [`PASSED`], and
/// its bytes as a stream that reads the next piece from Mneme only when the browser is
/// ready for it, so no download is ever held in memory. Mneme's policy for the file is
/// kept as it is, and the middleware adds Atrium's own only where a response has none.
fn relay(answer: reqwest::Response, head: bool) -> Response {
    let status = answer.status();
    let mut headers = HeaderMap::new();
    for name in PASSED {
        for value in answer.headers().get_all(&name) {
            headers.append(name.clone(), value.clone());
        }
    }
    headers
        .entry(CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(SANDBOX));
    // The wiki is private to those who may read it, so no cache between them keeps a copy.
    if let Some(cache) = headers.get(CACHE_CONTROL).and_then(|v| v.to_str().ok())
        && !cache.split(',').any(|part| part.trim() == "private")
        && let Ok(private) = HeaderValue::from_str(&format!("private, {cache}"))
    {
        headers.insert(CACHE_CONTROL, private);
    }
    let body = if head || status == StatusCode::NOT_MODIFIED {
        Body::empty()
    } else if status == StatusCode::RANGE_NOT_SATISFIABLE {
        // Mneme's reason is JSON, never shown: the status and `Content-Range` say it.
        headers.remove(CONTENT_TYPE);
        headers.remove(CONTENT_LENGTH);
        Body::empty()
    } else {
        Body::from_stream(chunks(answer))
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

/// The body of Mneme's answer, a piece at a time. A Mneme that sends nothing for
/// [`STALL`] while the browser waits ends the download, which the browser sees cut short.
fn chunks(
    answer: reqwest::Response,
) -> impl futures_util::Stream<Item = Result<axum::body::Bytes, io::Error>> {
    futures_util::stream::unfold(Some(answer), |answer| async move {
        let mut answer = answer?;
        match tokio::time::timeout(STALL, answer.chunk()).await {
            Ok(Ok(Some(chunk))) => Some((Ok(chunk), Some(answer))),
            Ok(Ok(None)) => None,
            Ok(Err(error)) => Some((Err(io::Error::other(error)), None)),
            Err(_) => Some((Err(io::Error::other("Mneme stopped sending")), None)),
        }
    })
}

/// The not-found page for a file's bytes, which names the file that was deleted there, as
/// a link to a deleted page does.
async fn missing(shell: &Shell, mneme: &Mneme, path: &str) -> Response {
    let ways = [("All files".to_owned(), "/wiki/files".to_owned())];
    let Ok(detail) = mneme.file(path, 0, 1).await else {
        return shell.lost(None, &ways);
    };
    let Some(deleted) = detail.file.deleted else {
        return shell.lost(None, &ways);
    };
    let local = shell.local();
    let gone = html! {
        p.diff-sum {
            "deleted " (local.when_long(deleted))
            @if let Some(event) = detail.events.first() { " by " (event.author) }
        }
        p { a.ttl-gone href=(kind::page_href(path)) { (kind::name(path)) } ", at " code { (path) } "." }
    };
    shell.lost(Some(gone), &ways)
}

/// Every file a page links to or shows, as far as one listing finds them: the files under
/// the folder all of them are in. `None` from a Mneme that lists no files.
async fn known(mneme: &Mneme, linked: &[String]) -> Option<Option<Known>> {
    if linked.is_empty() {
        return Some(None);
    }
    let folder = common_folder(linked);
    let listing = FileListing {
        under: Some(&folder),
        words: None,
        sort: Some(FileSort::Path),
        order: None,
        offset: 0,
        limit: STEP,
    };
    match mneme.files(listing).await {
        Ok(None) => None,
        Ok(Some(found)) => {
            let complete = found.total <= u64::try_from(found.files.len()).unwrap_or(0);
            let files = found
                .files
                .into_iter()
                .map(|file| (file.path, KnownFile { size: file.size }))
                .collect();
            Some(Some(Known { files, complete }))
        }
        Err(_) => Some(None),
    }
}

/// The folder every path is in, `""` for the top.
fn common_folder(paths: &[String]) -> String {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let parts: Vec<&str> = kind::folder(path)
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        common = Some(match common {
            None => parts,
            Some(common) => common
                .iter()
                .zip(&parts)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    common.unwrap_or_default().join("/")
}

/// What a page's text needs to write its links to files: whether Mneme serves files, and
/// what one listing says of those the text links to.
pub struct PageFiles {
    served: bool,
    known: Option<Known>,
}

impl PageFiles {
    /// Asks Mneme about the files `markdown`, the text of the page at `path`, links to.
    pub async fn of(mneme: &Mneme, markdown: &str, path: &str) -> Self {
        let linked = markdown::files_linked(markdown, path);
        match known(mneme, &linked).await {
            None => Self {
                served: false,
                known: None,
            },
            Some(known) => Self {
                served: true,
                known,
            },
        }
    }

    /// How the text writes them, at the commit `at` for an old version.
    #[must_use]
    pub fn links<'a>(&'a self, at: Option<&'a str>) -> FileLinks<'a> {
        if self.served {
            FileLinks::Served {
                at,
                known: self.known.as_ref(),
            }
        } else {
            FileLinks::AsWritten
        }
    }
}

/// How the list reads its sort.
const fn by(sort: FileSort) -> &'static str {
    match sort {
        FileSort::Path => "by name",
        FileSort::Updated => "by last change",
        FileSort::Size => "by size",
    }
}

/// The two orders as a reader names them for a sort.
const fn orders(sort: FileSort) -> [(Order, &'static str); 2] {
    match sort {
        FileSort::Path => [(Order::Asc, "A to Z"), (Order::Desc, "Z to A")],
        FileSort::Updated => [(Order::Desc, "Newest first"), (Order::Asc, "Oldest first")],
        FileSort::Size => [
            (Order::Desc, "Largest first"),
            (Order::Asc, "Smallest first"),
        ],
    }
}

/// The name the list's addresses give a sort.
const fn sort_name(sort: FileSort) -> &'static str {
    match sort {
        FileSort::Path => "name",
        FileSort::Updated => "updated",
        FileSort::Size => "size",
    }
}

fn sort_of(name: Option<&str>) -> Result<FileSort, Failure> {
    match name {
        None | Some("" | "name") => Ok(FileSort::Path),
        Some("updated") => Ok(FileSort::Updated),
        Some("size") => Ok(FileSort::Size),
        Some(_) => Err(Failure::BadQuery),
    }
}

/// What the list shows: a folder, the words its names must hold, the sort and the page.
#[derive(Debug, Clone)]
struct Asked {
    under: String,
    words: String,
    sort: FileSort,
    order: Order,
    at: u64,
}

impl Asked {
    /// The list's address for `under`, keeping the rest unless `set` changes it.
    fn href(&self, under: &str, sort: FileSort, order: Order, at: u64) -> String {
        let page = if at > 1 {
            at.to_string()
        } else {
            String::new()
        };
        let sorted = (sort, order) != (FileSort::Path, Order::Asc);
        href(
            "/wiki/files",
            &[
                ("under", under),
                ("q", &self.words),
                ("sort", if sorted { sort_name(sort) } else { "" }),
                ("order", if sorted { order.name() } else { "" }),
                ("page", &page),
            ],
        )
    }
}

/// A folder inside the one listed: its name, and what its files add up to.
#[derive(Debug, Clone, Default)]
struct Folder {
    name: String,
    files: u64,
    size: u64,
    updated: i64,
    author: String,
}

enum Row<'a> {
    Folder(Folder),
    File(&'a File),
}

impl Row<'_> {
    fn name(&self) -> &str {
        match self {
            Self::Folder(folder) => &folder.name,
            Self::File(file) => kind::name(&file.path),
        }
    }

    fn size(&self) -> u64 {
        match self {
            Self::Folder(folder) => folder.size,
            Self::File(file) => file.size,
        }
    }

    fn updated(&self) -> i64 {
        match self {
            Self::Folder(folder) => folder.updated,
            Self::File(file) => file.updated,
        }
    }
}

/// Every live file under `under`, by path, up to [`MOST`], and how many there are in all;
/// `None` from a Mneme that lists no files.
async fn all_under(mneme: &Mneme, under: &str) -> Result<Option<(Vec<File>, u64)>, MnemeError> {
    let listing = |offset| FileListing {
        under: Some(under),
        words: None,
        sort: Some(FileSort::Path),
        order: None,
        offset,
        limit: STEP,
    };
    let Some(first) = mneme.files(listing(0)).await? else {
        return Ok(None);
    };
    let total = first.total;
    let mut files = first.files;
    while u64::try_from(files.len()).unwrap_or(u64::MAX) < total.min(MOST) {
        let offset = u64::try_from(files.len()).unwrap_or(0);
        match mneme.files(listing(offset)).await? {
            Some(more) if !more.files.is_empty() => files.extend(more.files),
            _ => break,
        }
    }
    Ok(Some((files, total)))
}

/// The folders and files directly in `under`, from every file under it, sorted as asked,
/// folders first.
fn rows<'a>(files: &'a [File], under: &str, sort: FileSort, order: Order) -> Vec<Row<'a>> {
    let prefix = if under.is_empty() {
        String::new()
    } else {
        format!("{under}/")
    };
    let mut folders: Vec<Folder> = Vec::new();
    let mut here: Vec<Row<'a>> = Vec::new();
    for file in files {
        let Some(rest) = file.path.strip_prefix(&prefix) else {
            continue;
        };
        match rest.split_once('/') {
            None => here.push(Row::File(file)),
            Some((name, _)) => {
                let index = folders
                    .iter()
                    .position(|folder| folder.name == name)
                    .unwrap_or_else(|| {
                        folders.push(Folder {
                            name: name.to_owned(),
                            ..Folder::default()
                        });
                        folders.len() - 1
                    });
                let folder = &mut folders[index];
                folder.files += 1;
                folder.size += file.size;
                if file.updated >= folder.updated {
                    folder.updated = file.updated;
                    folder.author.clone_from(&file.author);
                }
            }
        }
    }
    let mut all: Vec<Row<'a>> = folders.into_iter().map(Row::Folder).collect();
    let split = all.len();
    all.extend(here);
    let sort_part = |part: &mut [Row<'a>]| {
        match sort {
            FileSort::Path => part.sort_by(|a, b| a.name().cmp(b.name())),
            FileSort::Updated => part.sort_by_key(|row| Reverse(row.updated())),
            FileSort::Size => part.sort_by_key(|row| Reverse(row.size())),
        }
        if order != sort.default_order() {
            part.reverse();
        }
    };
    let (folders, here) = all.split_at_mut(split);
    sort_part(folders);
    sort_part(here);
    all
}

/// `Files / brand / print`, each folder a link but the last.
fn crumbs(under: &str, last_linked: bool) -> Markup {
    let parts: Vec<&str> = under.split('/').filter(|part| !part.is_empty()).collect();
    html! {
        p.crumbs {
            @if parts.is_empty() && !last_linked {
                "Files"
            } @else {
                a href="/wiki/files" { "Files" }
            }
            @for (i, part) in parts.iter().enumerate() {
                " " span aria-hidden="true" { "/" } " "
                @if i + 1 < parts.len() || last_linked {
                    a href=(kind::folder_href(&parts[..=i].join("/"))) { (part) }
                } @else {
                    (part)
                }
            }
        }
    }
}

/// The switch between the wiki's pages and its files, at the head of both lists.
#[must_use]
pub fn switch(files_now: bool, pages: u64, files: u64) -> Markup {
    html! {
        nav.wiki-switch aria-label="Pages or files" {
            a.seg href="/wiki" aria-current=[(!files_now).then_some("true")] { "Pages " span.n { (number(pages)) } }
            a.seg href="/wiki/files" aria-current=[files_now.then_some("true")] { "Files " span.n { (number(files)) } }
        }
    }
}

/// How many files there are, or `None` from a Mneme that lists none or did not say.
pub async fn file_count(mneme: &Mneme) -> Option<u64> {
    let listing = FileListing {
        under: None,
        words: None,
        sort: None,
        order: None,
        offset: 0,
        limit: 1,
    };
    mneme.files(listing).await.ok().flatten().map(|f| f.total)
}

pub async fn list(
    shell: Shell,
    State(mneme): State<Mneme>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = list_page(&shell, &mneme, query).await;
    respond(&shell, &mneme, shown)
}

fn kind_cell(path: Option<&str>) -> Markup {
    html! {
        td.kind-mark {
            @match path {
                Some(path) => span.file-kind { (kind::label(path)) },
                None => span.file-kind.folder-kind { "DIR" span.vh { ", a folder" } },
            }
        }
    }
}

fn row(shell: &Shell, row: &Row<'_>, under: &str, filtered: bool) -> Markup {
    let local = shell.local();
    match row {
        Row::Folder(folder) => {
            let path = if under.is_empty() {
                folder.name.clone()
            } else {
                format!("{under}/{}", folder.name)
            };
            html! {
                tr.folder-row {
                    (kind_cell(None))
                    td {
                        a.ttl href=(kind::folder_href(&path)) data-row { (folder.name) "/" }
                        span.pth { (count(folder.files, "file", "files")) }
                    }
                    td.num data-label="Size" { (bytes(folder.size)) }
                    td.when data-label="Changed" { (local.time(folder.updated, &local.when(folder.updated))) }
                    td.mono.hide-s data-label="By" { (folder.author) }
                }
            }
        }
        Row::File(file) => {
            let place = kind::folder(&file.path);
            let shown_folder = place
                .strip_prefix(under)
                .map_or(place, |rest| rest.trim_start_matches('/'));
            html! {
                tr {
                    (kind_cell(Some(&file.path)))
                    td {
                        a.ttl href=(kind::page_href(&file.path)) data-row data-carry { (kind::name(&file.path)) }
                        @if filtered && !shown_folder.is_empty() {
                            span.pth { a href=(kind::folder_href(place)) { (shown_folder) "/" } }
                        }
                    }
                    td.num data-label="Size" { (bytes(file.size)) }
                    td.when data-label="Changed" { (local.time(file.updated, &local.when(file.updated))) }
                    td.mono.hide-s data-label="By" { (file.author) }
                }
            }
        }
    }
}

/// The column heads; name, size and last change each a link that sorts by it.
fn head(asked: &Asked) -> Markup {
    let sorter = |sort: FileSort, label: &str, class: &str| {
        let current = asked.sort == sort;
        let order = if current {
            asked.order.flip()
        } else {
            sort.default_order()
        };
        html! {
            th scope="col" class=[(!class.is_empty()).then_some(class)]
                aria-sort=[current.then_some(if asked.order == Order::Asc { "ascending" } else { "descending" })]
            {
                a.sorter href=(asked.href(&asked.under, sort, order, 1)) { (label) }
            }
        }
    };
    html! {
        thead {
            tr {
                th scope="col" { "Kind" }
                (sorter(FileSort::Path, "Name", ""))
                (sorter(FileSort::Size, "Size", "num"))
                (sorter(FileSort::Updated, "Changed", ""))
                th.hide-s scope="col" { "By" }
            }
        }
    }
}

fn no_files_yet() -> Page {
    let body = html! {
        main #(MAIN) .wiki-index.files-index {
            header.page-head {
                h1 { "Files" }
                p.sub { "This wiki's Mneme does not list its files. A newer Mneme does." }
            }
        }
    };
    Page::new(
        "Files, Atrium wiki",
        "The wiki's files that are not pages.",
        body,
    )
}

/// What the list reads: the files it shows, how many pages and files the wiki has.
async fn read(
    mneme: &Mneme,
    asked: &Asked,
    offset: u64,
) -> Result<(Option<(Vec<File>, u64)>, u64, Option<u64>), MnemeError> {
    let pages = async { mneme.status().await.map(|status| status.pages) };
    let all = async { Ok(file_count(mneme).await) };
    if asked.words.is_empty() {
        tokio::try_join!(all_under(mneme, &asked.under), pages, all)
    } else {
        let listing = FileListing {
            under: Some(&asked.under),
            words: Some(&asked.words),
            sort: Some(asked.sort),
            order: Some(asked.order),
            offset,
            limit: PAGE_SIZE,
        };
        let (found, pages, all) = tokio::try_join!(mneme.files(listing), pages, all)?;
        Ok((found.map(|found| (found.files, found.total)), pages, all))
    }
}

/// The list's heading: the folder, or all files with a line on what they are.
fn list_head(asked: &Asked, total: u64, pages: u64, files: u64) -> Markup {
    html! {
        header.page-head {
            @if asked.under.is_empty() {
                h1 { "Files" }
                p.sub {
                    "Images, PDFs, archives and every other file kept beside the pages. "
                    (cap_count(files)) " on the record."
                }
            } @else {
                (crumbs(&asked.under, false))
                h1 { (kind::name(&asked.under)) }
                p.sub { (cap_count(total)) " in this folder and the folders inside it." }
            }
            (switch(true, pages, files))
        }
    }
}

/// The filter by name, a plain form that keeps the folder.
fn filter_form(asked: &Asked, where_words: &str) -> Markup {
    html! {
        form.file-find action="/wiki/files" method="get" role="search" aria-label="Filter files by name" {
            label.vh for="file-q" { "Filter the files" (where_words) " by name" }
            @if !asked.under.is_empty() { input type="hidden" name="under" value=(asked.under); }
            input #file-q .num-in name="q" type="search" value=(asked.words) autocomplete="off" spellcheck="false"
                placeholder={ "Filter by name" (where_words) };
            button.btn type="submit" { "Filter" }
            @if !asked.words.is_empty() { a.seg href=(kind::folder_href(&asked.under)) { "Clear" } }
        }
    }
}

/// The rows shown, or a line saying there are none.
fn table(shell: &Shell, asked: &Asked, shown: &[&Row<'_>], where_words: &str) -> Markup {
    let order_words = orders(asked.sort)
        .iter()
        .find(|(o, _)| *o == asked.order)
        .map_or("", |(_, words)| *words)
        .to_lowercase();
    let filtered = !asked.words.is_empty();
    html! {
        @if shown.is_empty() {
            p.files-empty {
                @if filtered { "No file" (where_words) " has “" (asked.words) "” in its name." }
                @else { "No files" (where_words) "." }
            }
        } @else {
            table.ledger.file-ledger {
                caption.vh { "Files" (where_words) ", sorted " (by(asked.sort)) ", " (order_words) ". Column heads change the sort." }
                (head(asked))
                tbody {
                    @for each in shown { (row(shell, each, &asked.under, filtered)) }
                }
            }
        }
    }
}

async fn list_page(
    shell: &Shell,
    mneme: &Mneme,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let params = params(query)?;
    let sort = sort_of(params.sort.as_deref())?;
    let order = choice(params.order.as_deref())?.unwrap_or(sort.default_order());
    let under = params
        .under
        .as_deref()
        .unwrap_or_default()
        .trim_matches('/')
        .to_owned();
    let words = params.q.as_deref().unwrap_or_default().trim().to_owned();
    let asked = Asked {
        under,
        words,
        sort,
        order,
        at: page_number(&params)?,
    };
    let offset = (asked.at - 1) * PAGE_SIZE;
    let filtered = !asked.words.is_empty();
    let (found, pages, files_in_all) = read(mneme, &asked, offset).await?;
    let Some((files, total)) = found else {
        return Ok(no_files_yet());
    };
    let listed: Vec<Row<'_>> = if filtered {
        files.iter().map(Row::File).collect()
    } else {
        rows(&files, &asked.under, sort, order)
    };
    let rows_total = if filtered {
        total
    } else {
        u64::try_from(listed.len()).unwrap_or(0)
    };
    let shown: Vec<&Row<'_>> = if filtered {
        listed.iter().collect()
    } else {
        listed
            .iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(usize::try_from(PAGE_SIZE).unwrap_or(16))
            .collect()
    };
    let at = asked.at;
    let page_count = rows_total.div_ceil(PAGE_SIZE).max(1);
    let first = (offset + 1).min(rows_total);
    let last = offset + u64::try_from(shown.len()).unwrap_or(0);
    let where_words = if asked.under.is_empty() {
        String::new()
    } else {
        format!(" in {}", asked.under)
    };
    let cut = !filtered && total > MOST;
    let body = html! {
        main #(MAIN) .wiki-index.files-index {
            (list_head(&asked, total, pages, files_in_all.unwrap_or(total)))
            section.wiki-list.surface data-surface aria-labelledby="list-h" {
                h2 #list-h .vh { "Files" (where_words) }
                (filter_form(&asked, &where_words))
                div.list-bar {
                    p.find-count {
                        @if filtered {
                            (if rows_total == 1 { "File " } else { "Files " }) b { (first) "–" (last) } " of " (number(rows_total))
                            (where_words) " named " q { (asked.words) } ", " (by(sort))
                        } @else {
                            "Folders and files " b { (first) "–" (last) } " of " (number(rows_total)) (where_words) ", " (by(sort))
                        }
                    }
                    nav.order aria-label="Order" {
                        span.order-label { "Order" }
                        @for (each, label) in orders(sort) {
                            a.seg href=(asked.href(&asked.under, sort, each, 1)) aria-current=[(each == order).then_some("true")] { (label) }
                        }
                    }
                }
                (table(shell, &asked, &shown, &where_words))
                @if cut {
                    p.files-empty { "This list holds the first " (number(MOST)) " files" (where_words) " by name. Filter by name to find the others." }
                }
                (pager(
                    "Pages of the list",
                    &format!("Page {at} of {page_count}"),
                    (at > 1).then(|| asked.href(&asked.under, sort, order, at - 1)).as_deref(),
                    (at < page_count).then(|| asked.href(&asked.under, sort, order, at + 1)).as_deref(),
                ))
            }
        }
    };
    Ok(Page::new(
        if asked.under.is_empty() {
            "Files, Atrium wiki".to_owned()
        } else {
            format!("{}, files, Atrium wiki", asked.under)
        },
        "The wiki's files that are not pages, by folder.",
        body,
    ))
}

/// `Six files`, `One file`.
fn cap_count(n: u64) -> String {
    atrium_ui::words::cap(&count(n, "file", "files"))
}

/// `GET /wiki/files/{path}`: one file, with a preview where a browser can show its kind, and
/// every version. A link written as a page's file, `/wiki/files/{path}.md`, is a link to a
/// page in a folder named `files`.
pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(path): Path<String>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    if markdown::is_page(&path) {
        return lost::found_by_path(&shell, &mneme, &format!("files/{path}")).await;
    }
    let shown = file_page(&shell, &mneme, &path, query).await;
    respond(&shell, &mneme, shown)
}

/// What an event did, as the history says it.
fn change_words(event: &FileEvent) -> (&'static str, Option<String>) {
    match event.kind {
        EventKind::Created => ("created", None),
        EventKind::Edited => ("edited", None),
        EventKind::Renamed => (
            "moved",
            event.from.as_deref().map(|from| format!("from {from}")),
        ),
        EventKind::Deleted => ("deleted", None),
        EventKind::Other => ("changed", None),
    }
}

/// The preview a browser can show of the file, its bytes from `raw`.
fn preview(file: &File, raw: &str, text: Option<&(String, bool)>) -> Markup {
    let name = kind::name(&file.path);
    let size = bytes(file.size);
    html! {
        @match Shown::of(&file.media) {
            Shown::Image => figure.file-preview.img-preview {
                img src=(raw) alt={ "The image " (name) } decoding="async";
            },
            Shown::Audio => div.file-preview.media-preview { audio controls preload="metadata" src=(raw) {} },
            Shown::Video => div.file-preview.media-preview { video controls playsinline preload="metadata" src=(raw) {} },
            Shown::Text => figure.code.text-preview {
                figcaption {
                    @match text {
                        Some((_, true)) => { "The first " (bytes(TEXT_START as u64)) " of " (size) }
                        _ => { "The whole text, " (size) }
                    }
                }
                pre tabindex="0" aria-label={ "The text of " (name) } {
                    code { @if let Some((text, _)) = text { (text) } }
                }
            },
            Shown::Pdf => div.file-preview.open-preview {
                p.open-kind { "PDF" }
                p { "A PDF opens in a tab of its own, where the browser's own viewer shows it." }
                p.btns {
                    a.btn href=(raw) { "Open " (name) }
                }
            },
            Shown::Download => div.file-preview.open-preview {
                p.open-kind { (kind::label(&file.path)) }
                p { "A browser cannot show this kind of file. Download it to open it." }
            },
        }
    }
}

/// The history: every event, newest first, with a download of each version.
fn history(shell: &Shell, detail: &FileDetail, offset: u64) -> Markup {
    let local = shell.local();
    html! {
        table.ledger {
            caption.vh { "Every change to the file, newest first" }
            thead {
                tr {
                    th scope="col" { "Version" }
                    th scope="col" { "When" }
                    th scope="col" { "Change" }
                    th scope="col" { "By" }
                    th.num scope="col" { "Size" }
                    th scope="col" { span.vh { "Download" } }
                }
            }
            tbody {
                @for (i, event) in detail.events.iter().enumerate() {
                    @let n = detail.total - offset - u64::try_from(i).unwrap_or(0);
                    @let (word, more) = change_words(event);
                    tr {
                        td { span.ver { "v" (n) } }
                        td.when data-label="When" {
                            (local.time(event.time, &format!("{} {}", local.date_short(event.time), local.hm(event.time))))
                        }
                        td.kind-cell data-label="Change" { (word) @if let Some(more) = more { small { (more) } } }
                        td.mono data-label="By" { (event.author) }
                        td.num data-label="Size" {
                            @if event.kind == EventKind::Deleted { span.quiet { "gone" } } @else { (bytes(event.size)) }
                        }
                        td.acts-cell {
                            @if event.kind != EventKind::Deleted {
                                span.acts {
                                    a href=(kind::raw_href(&event.path, Some(event.commit.as_str()))) download=(kind::name(&event.path)) {
                                        "Download" span.vh { " v" (n) ", " (bytes(event.size)) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn file_page(
    shell: &Shell,
    mneme: &Mneme,
    path: &str,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let at = page_number(&params(query)?)?;
    let offset = (at - 1) * EVENTS;
    let (detail, _) = tokio::try_join!(mneme.file(path, offset, EVENTS), mneme.status())?;
    let file = &detail.file;
    let live = file.deleted.is_none();
    let text = if live && Shown::of(&file.media) == Shown::Text {
        mneme
            .raw_start(path, TEXT_START)
            .await
            .ok()
            .map(|(start, more)| (String::from_utf8_lossy(&start).into_owned(), more))
    } else {
        None
    };
    let local = shell.local();
    let name = kind::name(&file.path);
    let folder = kind::folder(&file.path);
    let raw = kind::raw_href(&file.path, None);
    let newest = detail.events.first().filter(|_| offset == 0);
    let pages = detail.total.div_ceil(EVENTS).max(1);
    let link = |at: u64| href(&kind::page_href(path), &[("page", &at.to_string())]);
    let body = html! {
        main #(MAIN) .file-page {
            header.page-head {
                (crumbs(folder, true))
                h1 { (name) }
                p.sub {
                    @if let Some(deleted) = file.deleted {
                        "Deleted " (local.when_long(deleted))
                        @if let Some(event) = newest { " by " (event.author) }
                        ". Its history keeps every version it had."
                    } @else {
                        (kind::label(&file.path)) ", " (bytes(file.size)) ", "
                        (count(file.versions, "version", "versions")) ". "
                        @if let Some(event) = newest {
                            "Last " (change_words(event).0) " " (local.when_long(event.time)) " by " (event.author) "."
                        }
                    }
                }
            }
            @if live {
                section.file-view.surface data-surface aria-label="The file" {
                    (preview(file, &raw, text.as_ref()))
                    p.file-actions {
                        a.btn.download-btn href=(raw) download=(name) { "Download" span.size { (bytes(file.size)) } }
                    }
                }
            }
            aside.file-facts.surface.rail-r data-surface aria-labelledby="facts-h" {
                h2 #facts-h .rail-h { "This file" }
                dl.facts {
                    div { dt { "Path" } dd { (file.path) } }
                    div { dt { "Folder" } dd { a href=(kind::folder_href(folder)) { @if folder.is_empty() { "the top" } @else { (folder) } } } }
                    div { dt { "Kind" } dd { (kind::label(&file.path)) ", " (file.media) } }
                    div { dt { "Size" } dd { (bytes(file.size)) @if file.size >= 1024 { " (" (number(file.size)) " bytes)" } } }
                    div { dt { "Versions" } dd { (file.versions) } }
                    div { dt { "Created" } dd { (local.time(file.created, &local.date_short(file.created))) } }
                }
            }
            section.events.file-events.surface data-surface aria-labelledby="ev-h" {
                h2 #ev-h .rail-h { "History" }
                (history(shell, &detail, offset))
                @if pages > 1 {
                    (pager(
                        "Pages of the history",
                        &format!("Page {at} of {pages}"),
                        (at > 1).then(|| link(at - 1)).as_deref(),
                        (at < pages).then(|| link(at + 1)).as_deref(),
                    ))
                }
            }
        }
    };
    Ok(Page::new(
        format!("{name}, Atrium wiki"),
        "A file of the wiki, with its preview and every version.",
        body,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_folder_is_shared_by_every_path() {
        let paths = |list: &[&str]| list.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            common_folder(&paths(&["brand/logo.svg", "brand/print/a.pdf"])),
            "brand"
        );
        assert_eq!(common_folder(&paths(&["brand/a.svg", "brandy.txt"])), "");
        assert_eq!(common_folder(&paths(&["a/b/c.png"])), "a/b");
    }
}
