//! The wiki feature: a reader for the wiki that Mneme holds.

mod article;
mod changes;
mod data;
pub mod demo;
mod diff;
mod history;
pub mod home;
mod list;
mod lost;
pub mod machines;
mod markdown;
pub mod mneme;
mod routes;
mod search;
pub mod stub;

use std::time::Duration;

use atrium_core::{Feature, HomeContext, HomeFuture, Local, NavEntry, Search};
use axum::Router;
use maud::{Markup, html};

use crate::mneme::Mneme;

/// The wiki, read from the Mneme at one URL.
pub struct Wiki {
    mneme: Mneme,
}

impl Wiki {
    /// A wiki read from the Mneme at `url`, giving up on each call after `timeout`, whose
    /// pages call the place that holds the record `record`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(url: &str, timeout: Duration, record: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            mneme: Mneme::new(url, timeout)?.named(record),
        })
    }

    /// The client, which the demo tells what it heard before it started.
    #[must_use]
    pub const fn mneme(&self) -> &Mneme {
        &self.mneme
    }
}

impl Feature for Wiki {
    fn id(&self) -> &'static str {
        "wiki"
    }

    fn name(&self) -> &'static str {
        "Wiki"
    }

    fn nav(&self) -> &'static [NavEntry] {
        &[
            NavEntry {
                label: "Wiki",
                title: "Wiki, all pages",
                href: "/wiki",
            },
            NavEntry {
                label: "Changes",
                title: "Recent changes",
                href: "/wiki/changes",
            },
            NavEntry {
                label: "Search",
                title: "Search",
                href: "/wiki/search",
            },
            NavEntry {
                label: "Machines",
                title: "Machines",
                href: "/wiki/machines",
            },
        ]
    }

    fn routes(&self) -> Router {
        routes::router(self.mneme.clone())
    }

    fn search(&self) -> Option<Search> {
        Some(Search {
            action: "/wiki/search",
            pages: "/wiki/index.json",
        })
    }

    fn home<'a>(&'a self, context: &'a HomeContext) -> HomeFuture<'a> {
        Box::pin(home::offer(&self.mneme, context))
    }

    fn footnote(&self, local: &Local) -> Option<Markup> {
        let status = self.mneme.known_status()?;
        Some(html! {
            "The record on " (self.mneme.record()) ": " (atrium_ui::words::number(status.pages)) " pages, "
            (atrium_ui::words::number(status.commits)) " commits"
            @if let Some(newest) = status.head_time { ", the newest " (local.when_long(newest)) }
            "."
        })
    }
}
