//! The contract between the server and its features.
//!
//! A feature implements [`Feature`] and is listed once in the server's registry. The server
//! builds its router, navigation, home page and command palette from that list alone.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use axum::Router;
use maud::Markup;

/// Who is asking. A listener serves one kind of viewer, set when the server starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    /// Someone who reached Atrium over the tailnet: the owner.
    Tailnet,
    /// Anyone else.
    Anonymous,
}

impl FromStr for Viewer {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "tailnet" => Ok(Self::Tailnet),
            "anonymous" => Ok(Self::Anonymous),
            _ => Err(format!(
                "unknown viewer {name:?}: expected tailnet or anonymous"
            )),
        }
    }
}

impl fmt::Display for Viewer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tailnet => "tailnet",
            Self::Anonymous => "anonymous",
        })
    }
}

/// Which viewers may see a feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The tailnet only.
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
            Self::Private => matches!(viewer, Viewer::Tailnet),
        }
    }
}

/// How much room a tile takes on the home page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileSize {
    /// One column.
    Narrow,
    /// Two columns where there is room.
    Wide,
}

/// A box on the home page that a feature fills.
#[derive(Debug, Clone)]
pub struct Tile {
    /// Unique across the site, and the tile's element id on the home page.
    pub id: &'static str,
    /// The tile's heading.
    pub title: &'static str,
    /// How much room it takes.
    pub size: TileSize,
    /// What it shows.
    pub body: Markup,
    /// Where its heading leads, if anywhere.
    pub link: Option<&'static str>,
}

/// An entry in the command palette. Every command is also a plain link on each page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    /// What the palette lists.
    pub label: &'static str,
    /// Keys that run it, such as `g w`, typed one after the other.
    pub keys: Option<&'static str>,
    /// Where it leads.
    pub href: &'static str,
}

/// A link in the site's navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavEntry {
    /// The link's text.
    pub label: &'static str,
    /// Where it leads.
    pub href: &'static str,
}

/// The tiles a feature offers, once they are ready.
pub type Tiles<'a> = Pin<Box<dyn Future<Output = Vec<Tile>> + Send + 'a>>;

/// A part of the site. Everything but [`Feature::id`], [`Feature::name`] and
/// [`Feature::routes`] has a default, and the default access is [`Access::Private`].
pub trait Feature: Send + Sync {
    /// A short, stable name, unique across the site.
    fn id(&self) -> &'static str;

    /// The name a person reads.
    fn name(&self) -> &'static str;

    /// Who may see the feature: its routes, link, tiles and commands.
    fn access(&self) -> Access {
        Access::Private
    }

    /// Where the feature's link in the site navigation leads, if it has one.
    fn nav(&self) -> Option<&'static str> {
        None
    }

    /// Every route the feature answers. The server guards them all with [`Feature::access`].
    fn routes(&self) -> Router;

    /// The tiles the feature puts on the home page.
    fn tiles(&self) -> Tiles<'_> {
        Box::pin(async { Vec::new() })
    }

    /// The feature's entries in the command palette.
    fn commands(&self) -> &'static [Command] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_admits_only_the_tailnet() {
        assert!(Access::Private.admits(Viewer::Tailnet));
        assert!(!Access::Private.admits(Viewer::Anonymous));
    }

    #[test]
    fn public_admits_everyone() {
        assert!(Access::Public.admits(Viewer::Tailnet));
        assert!(Access::Public.admits(Viewer::Anonymous));
    }

    #[test]
    fn viewer_names_read_back() {
        for viewer in [Viewer::Tailnet, Viewer::Anonymous] {
            assert_eq!(viewer.to_string().parse(), Ok(viewer));
        }
        assert!("everyone".parse::<Viewer>().is_err());
    }
}
