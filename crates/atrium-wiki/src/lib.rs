//! The wiki feature: a reader for the wiki that Mneme holds.

mod markdown;
pub mod mneme;
mod routes;
#[cfg(any(test, feature = "stub"))]
pub mod stub;

use std::time::Duration;

use atrium_core::{Command, Feature, Tile, TileSize, Tiles};
use axum::Router;

use crate::mneme::Mneme;

/// Changes the home page's tile lists.
const TILE_CHANGES: u64 = 5;

/// The wiki, read from the Mneme at one URL.
pub struct Wiki {
    mneme: Mneme,
}

impl Wiki {
    /// A wiki read from the Mneme at `url`, giving up on each call after `timeout`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(url: &str, timeout: Duration) -> Result<Self, reqwest::Error> {
        Ok(Self {
            mneme: Mneme::new(url, timeout)?,
        })
    }
}

impl Feature for Wiki {
    fn id(&self) -> &'static str {
        "wiki"
    }

    fn name(&self) -> &'static str {
        "Wiki"
    }

    fn nav(&self) -> Option<&'static str> {
        Some("/wiki")
    }

    fn routes(&self) -> Router {
        routes::router(self.mneme.clone())
    }

    fn tiles(&self) -> Tiles<'_> {
        Box::pin(async {
            let body = routes::latest(&self.mneme, TILE_CHANGES).await;
            vec![Tile {
                id: "wiki-changes",
                title: "Recent changes",
                size: TileSize::Wide,
                body,
                link: Some("/wiki/changes"),
            }]
        })
    }

    fn commands(&self) -> &'static [Command] {
        &[
            Command {
                label: "Wiki pages",
                keys: Some("g w"),
                href: "/wiki",
            },
            Command {
                label: "Recent changes",
                keys: Some("g c"),
                href: "/wiki/changes",
            },
            Command {
                label: "Search the wiki",
                keys: Some("g s"),
                href: "/wiki/search",
            },
            Command {
                label: "Machines",
                keys: Some("g m"),
                href: "/wiki/machines",
            },
        ]
    }
}
