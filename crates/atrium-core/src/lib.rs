//! The contract between the server and its features.
//!
//! A feature implements [`Feature`] and is listed once in the server's registry. The server
//! builds its router, navigation and home page from that list alone.

mod visit;

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Instant;

use axum::Router;
use maud::Markup;

pub use crate::visit::{Choice, Entrance, Intensity, Local, Prefs, Theme, Visit};

/// Who is asking. A listener serves one kind of viewer, set when the server starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    /// Anyone who may see private features, such as everyone the team's tailnet lets reach
    /// it. Its name is `member`; `tailnet`, its first name, and `owner`, as v0.1.0 called it,
    /// are other names for it.
    Member,
    /// Anyone else.
    Anonymous,
}

impl FromStr for Viewer {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "member" | "owner" | "tailnet" => Ok(Self::Member),
            "anonymous" => Ok(Self::Anonymous),
            _ => Err(format!(
                "unknown viewer {name:?}: expected member or anonymous"
            )),
        }
    }
}

impl fmt::Display for Viewer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Member => "member",
            Self::Anonymous => "anonymous",
        })
    }
}

/// Which viewers may see a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Members only.
    Private,
    /// Every viewer.
    Public,
}

impl Access {
    /// Whether `viewer` may see what this guards.
    #[must_use]
    pub const fn admits(self, viewer: Viewer) -> bool {
        match self {
            Self::Public => true,
            Self::Private => matches!(viewer, Viewer::Member),
        }
    }
}

/// A link in the site's navigation. Its place in the list is its key: the first is `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavEntry {
    /// The link's text.
    pub label: &'static str,
    /// Its longer name in the palette, such as "Recent changes" for "Changes".
    pub title: &'static str,
    /// Where it leads.
    pub href: &'static str,
}

/// What a feature is given to say its part of the home page.
#[derive(Debug, Clone)]
pub struct HomeContext {
    pub visit: Visit,
    /// When every tile must be ready. A tile that is not ready by then is left out.
    pub deadline: Instant,
}

/// A feature's part of the home page.
#[derive(Debug, Default)]
pub struct HomeOffer {
    /// Clauses of the opening sentence, each a sentence of its own.
    pub said: Vec<Markup>,
    /// A drawing beside the sentence.
    pub figure: Option<Markup>,
    /// Tiles below, in order.
    pub tiles: Vec<Markup>,
    /// Whether the feature is out of reach, which changes the page's look.
    pub away: bool,
}

/// Where a feature searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Search {
    /// Where a search form sends its words, as `q`.
    pub action: &'static str,
    /// Every page the palette offers as the reader types, as JSON:
    /// `[{ "t": title, "path", "m": when, "href" }]`.
    pub pages: &'static str,
}

/// A feature's part of the home page, once it is ready.
pub type HomeFuture<'a> = Pin<Box<dyn Future<Output = HomeOffer> + Send + 'a>>;

/// A part of the site. Everything but [`Feature::id`], [`Feature::name`] and
/// [`Feature::routes`] has a default, and the default access is [`Access::Private`].
pub trait Feature: Send + Sync {
    /// A short, stable name, unique across the site.
    fn id(&self) -> &'static str;

    /// The name a person reads.
    fn name(&self) -> &'static str;

    /// Who may see the feature: its routes, links and part of the home page.
    fn access(&self) -> Access {
        Access::Private
    }

    /// The feature's links in the site navigation, in order.
    fn nav(&self) -> &'static [NavEntry] {
        &[]
    }

    /// Every route the feature answers. The server guards them all with [`Feature::access`].
    fn routes(&self) -> Router;

    /// Where a search form sends its words and the palette finds its pages, when the
    /// feature searches.
    fn search(&self) -> Option<Search> {
        None
    }

    /// The feature's part of the home page.
    fn home<'a>(&'a self, _context: &'a HomeContext) -> HomeFuture<'a> {
        Box::pin(async { HomeOffer::default() })
    }

    /// A line for every page's footer, from what the feature already knows.
    fn footnote(&self, _local: &Local) -> Option<Markup> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_admits_only_members() {
        assert!(Access::Private.admits(Viewer::Member));
        assert!(!Access::Private.admits(Viewer::Anonymous));
    }

    #[test]
    fn public_admits_everyone() {
        assert!(Access::Public.admits(Viewer::Member));
        assert!(Access::Public.admits(Viewer::Anonymous));
    }

    #[test]
    fn viewer_names_read_back() {
        for viewer in [Viewer::Member, Viewer::Anonymous] {
            assert_eq!(viewer.to_string().parse(), Ok(viewer));
        }
        assert_eq!(Viewer::Member.to_string(), "member");
        assert_eq!("tailnet".parse(), Ok(Viewer::Member), "its first name");
        assert_eq!("owner".parse(), Ok(Viewer::Member), "v0.1.0's name");
        assert!("everyone".parse::<Viewer>().is_err());
    }
}
