//! The client for Mneme's reading API, and the shapes Atrium reads from it. Every answer is
//! parsed into these types here, and fields Atrium does not use are ignored, so Mneme can add
//! fields without breaking Atrium.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Why a call to Mneme gave no answer to show.
#[derive(Debug, thiserror::Error)]
pub enum MnemeError {
    /// Mneme could not be reached, took too long, or failed on its side.
    #[error("Mneme is away: {0}")]
    Away(String),
    /// Mneme has no such page or commit.
    #[error("not found")]
    NotFound,
    /// Mneme could not read the request, such as a search it cannot parse.
    #[error("{0}")]
    Refused(String),
    /// Mneme answered with something Atrium cannot read.
    #[error("unexpected answer from Mneme: {0}")]
    Broken(String),
}

/// A page's id: 16 lowercase hexadecimal characters, stable across renames.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct PageId(String);

/// A commit's full id: 40 lowercase hexadecimal characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitId(String);

fn hex_of_length(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

macro_rules! hex_id {
    ($type:ident, $length:literal, $what:literal) => {
        impl FromStr for $type {
            type Err = String;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                if hex_of_length(text, $length) {
                    Ok(Self(text.to_owned()))
                } else {
                    Err(format!("{text:?} is not a {}", $what))
                }
            }
        }

        impl TryFrom<String> for $type {
            type Error = String;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                text.parse()
            }
        }

        impl From<$type> for String {
            fn from(id: $type) -> Self {
                id.0
            }
        }

        impl fmt::Display for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl $type {
            /// The id as text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

hex_id!(PageId, 16, "page id");
hex_id!(CommitId, 40, "commit id");

impl CommitId {
    /// The first seven characters, as Git abbreviates.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..7]
    }
}

/// A page as listings show it.
#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub path: String,
    pub title: String,
    pub words: u64,
    pub created: i64,
    /// The time of its last version.
    pub updated: i64,
    pub versions: u64,
    /// Words added plus removed over all versions.
    pub churn: u64,
    /// When it was deleted, if it was.
    pub deleted: Option<i64>,
}

/// One page of a listing of pages.
#[derive(Debug, Deserialize)]
pub struct Pages {
    pub total: u64,
    pub pages: Vec<Page>,
}

/// A page with its text and activity.
#[derive(Debug, Deserialize)]
pub struct PageDetail {
    pub page: Page,
    /// The current text, or `None` for a deleted page.
    pub markdown: Option<String>,
    /// The last 7, 30 and 90 days and all time, in that order.
    pub windows: Vec<Window>,
}

/// A page's activity over some days.
#[derive(Debug, Deserialize)]
pub struct Window {
    /// `None` for all time.
    pub days: Option<u32>,
    pub versions: u64,
    /// Runs of versions with gaps under 30 minutes.
    pub sessions: u64,
    pub added: u64,
    pub removed: u64,
}

/// What an event did to a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Created,
    Edited,
    Renamed,
    Deleted,
    /// A kind this version of Atrium does not know.
    #[serde(other)]
    Other,
}

impl EventKind {
    /// The kind as a past-tense word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Edited => "edited",
            Self::Renamed => "renamed",
            Self::Deleted => "deleted",
            Self::Other => "changed",
        }
    }
}

/// A commit's change to one page.
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub commit: CommitId,
    pub page: PageId,
    /// The page's path after the event.
    pub path: String,
    pub time: i64,
    pub author: String,
    pub kind: EventKind,
    /// The old path, for a rename.
    #[serde(default)]
    pub from: Option<String>,
    /// Whether the event changed the text.
    pub version: bool,
    pub words: u64,
    pub added: u64,
    pub removed: u64,
    /// The commit's `Overlap:` trailer: the edit the autosave service set aside.
    #[serde(default)]
    pub overlap: Option<String>,
}

/// One page of events, newest first.
#[derive(Debug, Deserialize)]
pub struct Events {
    pub total: u64,
    pub events: Vec<Event>,
}

/// A page's text at a commit.
#[derive(Debug, Deserialize)]
pub struct Version {
    pub page: PageId,
    /// The commit whose event left this text.
    pub commit: CommitId,
    pub markdown: String,
}

/// The changed lines between two texts of a page.
#[derive(Debug, Deserialize)]
pub struct Diff {
    /// `None` when the page did not exist before.
    pub from: Option<CommitId>,
    pub to: CommitId,
    /// `None` when a text is too long to compare.
    pub lines: Option<Vec<DiffLine>>,
}

/// One line of a diff.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum DiffLine {
    Same(String),
    Removed(Vec<Span>),
    Added(Vec<Span>),
    /// This many unchanged lines left out.
    Skipped(u64),
}

/// Part of a changed line, marked where its words changed.
#[derive(Debug, Deserialize)]
pub struct Span {
    pub text: String,
    pub changed: bool,
}

/// One page of search results, best first unless sorted otherwise.
#[derive(Debug, Deserialize)]
pub struct Hits {
    pub total: u64,
    pub hits: Vec<Hit>,
}

/// A page that matched a search.
#[derive(Debug, Deserialize)]
pub struct Hit {
    pub page: Page,
    pub snippet: Vec<Run>,
}

/// Part of a snippet, marked where it matched.
#[derive(Debug, Deserialize)]
pub struct Run {
    pub text: String,
    pub hit: bool,
}

/// Every machine that may push.
#[derive(Debug, Deserialize)]
pub struct Machines {
    pub machines: Vec<Machine>,
}

/// What a machine's autosave service last did and said. Every field but `name` and `current`
/// is `None` until the machine has done that.
#[derive(Debug, Deserialize)]
pub struct Machine {
    pub name: String,
    /// When it last made a Git request.
    #[serde(default)]
    pub seen: Option<i64>,
    /// When its last report arrived.
    #[serde(default)]
    pub reported: Option<i64>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub head: Option<String>,
    /// Whether its reported head is the server's.
    #[serde(default)]
    pub current: bool,
}

/// What a listing of pages is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    Title,
    Updated,
    Created,
    Versions,
    Words,
    /// Words added plus removed over all versions.
    Churn,
}

impl Sort {
    /// Every sort, in the order a table shows them.
    pub const ALL: [Self; 6] = [
        Self::Title,
        Self::Updated,
        Self::Created,
        Self::Versions,
        Self::Words,
        Self::Churn,
    ];

    /// The name Mneme and Atrium's own URLs use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Updated => "updated",
            Self::Created => "created",
            Self::Versions => "versions",
            Self::Words => "words",
            Self::Churn => "churn",
        }
    }

    /// The order Mneme uses when none is given: A to Z for titles, largest first otherwise.
    #[must_use]
    pub const fn default_order(self) -> Order {
        match self {
            Self::Title => Order::Asc,
            _ => Order::Desc,
        }
    }
}

/// Ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    Asc,
    Desc,
}

impl Order {
    /// The name Mneme and Atrium's own URLs use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    /// The other order.
    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

/// How a listing is sorted and which part of it to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    /// `None` for Mneme's default: title for pages, best match for a search.
    pub sort: Option<Sort>,
    pub order: Option<Order>,
    pub offset: u64,
    pub limit: u64,
}

impl Listing {
    fn query(self) -> Vec<(&'static str, String)> {
        let mut query = vec![
            ("offset", self.offset.to_string()),
            ("limit", self.limit.to_string()),
        ];
        if let Some(sort) = self.sort {
            query.push(("sort", sort.name().to_owned()));
        }
        if let Some(order) = self.order {
            query.push(("order", order.name().to_owned()));
        }
        query
    }
}

/// Mneme's reading API at one base URL. Every call gives up after the timeout.
#[derive(Debug, Clone)]
pub struct Mneme {
    http: Client,
    base: String,
}

impl Mneme {
    /// A client for the Mneme at `base`, such as `http://mneme:8080`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(base: &str, timeout: Duration) -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .build()?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        })
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, MnemeError> {
        let away = |error: reqwest::Error| MnemeError::Away(error.to_string());
        let response = self
            .http
            .get(format!("{}/v1/{path}", self.base))
            .query(query)
            .send()
            .await
            .map_err(away)?;
        let status = response.status();
        let body = response.bytes().await.map_err(away)?;
        match status {
            StatusCode::OK => serde_json::from_slice(&body)
                .map_err(|error| MnemeError::Broken(format!("{path}: {error}"))),
            StatusCode::NOT_FOUND => Err(MnemeError::NotFound),
            StatusCode::BAD_REQUEST => Err(MnemeError::Refused(
                serde_json::from_slice::<ErrorBody>(&body).map_or_else(
                    |_| "Mneme refused the request".to_owned(),
                    |body| body.error,
                ),
            )),
            status if status.is_server_error() => {
                Err(MnemeError::Away(format!("{path} answered {status}")))
            }
            status => Err(MnemeError::Broken(format!("{path} answered {status}"))),
        }
    }

    /// Live pages, sorted and paged.
    ///
    /// # Errors
    ///
    /// Any [`MnemeError`].
    pub async fn pages(&self, listing: Listing) -> Result<Pages, MnemeError> {
        self.get("pages", &listing.query()).await
    }

    /// A page with its text and activity.
    ///
    /// # Errors
    ///
    /// [`MnemeError::NotFound`] for an unknown page, or any other [`MnemeError`].
    pub async fn page(&self, id: &PageId) -> Result<PageDetail, MnemeError> {
        self.get(&format!("pages/{id}"), &[]).await
    }

    /// A page's events, newest first.
    ///
    /// # Errors
    ///
    /// [`MnemeError::NotFound`] for an unknown page, or any other [`MnemeError`].
    pub async fn versions(
        &self,
        id: &PageId,
        offset: u64,
        limit: u64,
    ) -> Result<Events, MnemeError> {
        let query = [("offset", offset.to_string()), ("limit", limit.to_string())];
        self.get(&format!("pages/{id}/versions"), &query).await
    }

    /// A page's text at a commit.
    ///
    /// # Errors
    ///
    /// [`MnemeError::NotFound`] when the page had no text then, or any other [`MnemeError`].
    pub async fn version(&self, id: &PageId, commit: &CommitId) -> Result<Version, MnemeError> {
        self.get(&format!("pages/{id}/versions/{commit}"), &[])
            .await
    }

    /// The diff between two texts of a page. Without `to`, the current text; without `from`,
    /// the text before the last change at or before `to`.
    ///
    /// # Errors
    ///
    /// [`MnemeError::NotFound`] for an unknown page or commit, or any other [`MnemeError`].
    pub async fn diff(
        &self,
        id: &PageId,
        from: Option<&CommitId>,
        to: Option<&CommitId>,
    ) -> Result<Diff, MnemeError> {
        let query: Vec<_> = [("from", from), ("to", to)]
            .into_iter()
            .filter_map(|(name, commit)| commit.map(|commit| (name, commit.to_string())))
            .collect();
        self.get(&format!("pages/{id}/diff"), &query).await
    }

    /// Events across all pages, newest first.
    ///
    /// # Errors
    ///
    /// Any [`MnemeError`].
    pub async fn changes(&self, offset: u64, limit: u64) -> Result<Events, MnemeError> {
        let query = [("offset", offset.to_string()), ("limit", limit.to_string())];
        self.get("changes", &query).await
    }

    /// Pages that match `words`, changed within `days` if given.
    ///
    /// # Errors
    ///
    /// [`MnemeError::Refused`] for a search Mneme cannot read, or any other [`MnemeError`].
    pub async fn search(
        &self,
        words: &str,
        days: Option<u32>,
        listing: Listing,
    ) -> Result<Hits, MnemeError> {
        let mut query = listing.query();
        query.push(("q", words.to_owned()));
        if let Some(days) = days {
            query.push(("changed_within_days", days.to_string()));
        }
        self.get("search", &query).await
    }

    /// Every machine and what its autosave service last reported.
    ///
    /// # Errors
    ///
    /// Any [`MnemeError`].
    pub async fn machines(&self) -> Result<Machines, MnemeError> {
        self.get("machines", &[]).await
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::{self, Stub};

    const ALPHA: &str = "4226c96dd1386f3a";
    const HEAD: &str = "faf510a1fb457f1af5496ca64dea77ed68ddb2f8";

    async fn client(stub: Stub) -> Mneme {
        let base = stub::serve(stub).await.base;
        Mneme::new(&base, Duration::from_millis(500)).unwrap()
    }

    fn listing() -> Listing {
        Listing {
            sort: None,
            order: None,
            offset: 0,
            limit: 50,
        }
    }

    #[tokio::test]
    async fn reads_every_example_response() {
        let mneme = client(Stub::Fixtures).await;
        let alpha: PageId = ALPHA.parse().unwrap();
        let head: CommitId = HEAD.parse().unwrap();

        let pages = mneme.pages(listing()).await.unwrap();
        assert_eq!(pages.total, 6);
        assert_eq!(pages.pages[2].deleted, Some(1_788_223_800));

        let page = mneme.page(&alpha).await.unwrap();
        assert_eq!(page.page.title, "Alpha");
        assert_eq!(
            page.windows.iter().map(|w| w.days).collect::<Vec<_>>(),
            [Some(7), Some(30), Some(90), None]
        );

        let versions = mneme.versions(&alpha, 0, 50).await.unwrap();
        assert_eq!(versions.events[1].kind, EventKind::Renamed);
        assert_eq!(versions.events[1].from.as_deref(), Some("alpha.md"));

        let version = mneme.version(&alpha, &head).await.unwrap();
        assert_eq!(version.commit.short(), "0735657");

        let diff = mneme.diff(&alpha, None, None).await.unwrap();
        let lines = diff.lines.unwrap();
        assert!(matches!(lines[0], DiffLine::Skipped(3)));
        assert!(matches!(&lines[4], DiffLine::Added(spans) if spans[0].text == "And a third."));

        let changes = mneme.changes(0, 50).await.unwrap();
        assert_eq!(changes.total, 13);
        assert_eq!(changes.events[1].overlap.as_deref(), Some("aeolus"));

        let hits = mneme.search("harbours", None, listing()).await.unwrap();
        assert_eq!(hits.hits[0].snippet[1].text, "harbours");
        assert!(hits.hits[0].snippet[1].hit);

        let machines = mneme.machines().await.unwrap();
        assert_eq!(machines.machines[1].state.as_deref(), Some("blocked"));
    }

    #[tokio::test]
    async fn sends_the_listing_and_search_to_mneme() {
        let stub = stub::serve(Stub::Fixtures).await;
        let mneme = Mneme::new(&stub.base, Duration::from_millis(500)).unwrap();
        let sorted = Listing {
            sort: Some(Sort::Words),
            order: Some(Order::Asc),
            offset: 50,
            limit: 25,
        };
        mneme.search("boats -sails", Some(7), sorted).await.unwrap();
        assert_eq!(
            stub.requests(),
            [
                "/v1/search?offset=50&limit=25&sort=words&order=asc&q=boats+-sails&changed_within_days=7"
            ]
        );
    }

    #[tokio::test]
    async fn ignores_fields_it_does_not_know() {
        let machines = client(Stub::Fixtures).await.machines().await.unwrap();
        assert_eq!(
            machines.machines.len(),
            3,
            "the stub adds `skipped` to each"
        );
    }

    #[tokio::test]
    async fn an_unknown_page_is_not_found() {
        let other: PageId = "0123456789abcdef".parse().unwrap();
        let error = client(Stub::Fixtures).await.page(&other).await.unwrap_err();
        assert!(matches!(error, MnemeError::NotFound), "{error:?}");
    }

    #[tokio::test]
    async fn a_search_mneme_cannot_read_is_refused_with_its_reason() {
        let error = client(Stub::Fixtures)
            .await
            .search("\"open", None, listing())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, MnemeError::Refused(why) if why.contains("quote")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_slow_mneme_is_away() {
        let error = client(Stub::Slow(Duration::from_secs(5)))
            .await
            .machines()
            .await
            .unwrap_err();
        assert!(matches!(error, MnemeError::Away(_)), "{error:?}");
    }

    #[tokio::test]
    async fn a_mneme_that_is_down_is_away() {
        let error = client(Stub::Down).await.machines().await.unwrap_err();
        assert!(matches!(error, MnemeError::Away(_)), "{error:?}");
    }

    #[tokio::test]
    async fn a_mneme_that_fails_is_away() {
        let error = client(Stub::Failing).await.machines().await.unwrap_err();
        assert!(matches!(error, MnemeError::Away(_)), "{error:?}");
    }

    #[test]
    fn ids_are_lowercase_hex_of_their_length() {
        assert!(ALPHA.parse::<PageId>().is_ok());
        assert!("4226C96DD1386F3A".parse::<PageId>().is_err());
        assert!("../status".parse::<PageId>().is_err());
        assert!(ALPHA.parse::<CommitId>().is_err());
        assert!(HEAD.parse::<CommitId>().is_ok());
    }
}
