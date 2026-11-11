//! The registry: every feature, listed once. The router, the navigation, the home page and
//! the command palette all come from this list.

mod home;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use atrium_core::{Feature, HomeContext, HomeOffer, Local, NavEntry, Viewer};
use atrium_ui::components::away_tile;
use atrium_wiki::Wiki;
use futures_util::future::join_all;
use maud::Markup;

use crate::{Config, Demo};

/// How much longer than the tiles' deadline home waits for a feature that overruns it.
const GRACE: Duration = Duration::from_millis(100);

/// Every feature, in the order the navigation lists them.
pub fn registry(config: &Config, mneme_url: &str) -> Result<Vec<Arc<dyn Feature>>> {
    let timeout = Duration::from_millis(config.mneme_timeout_ms);
    let wiki = Wiki::new(mneme_url, timeout)?;
    if config.demo == Some(Demo::Away) {
        wiki.mneme().heard_at(atrium_wiki::demo::LAST_HEARD);
    }
    Ok(vec![Arc::new(home::Home), Arc::new(wiki)])
}

/// The registry as one listener's viewer sees it.
pub struct Site {
    all: Vec<Arc<dyn Feature>>,
    viewer: Viewer,
}

impl Site {
    pub fn new(all: Vec<Arc<dyn Feature>>, viewer: Viewer) -> Self {
        Self { all, viewer }
    }

    /// Every feature, including those this viewer may not see: their routes are mounted and
    /// refused by the access check.
    pub fn all(&self) -> &[Arc<dyn Feature>] {
        &self.all
    }

    fn visible(&self) -> impl Iterator<Item = &Arc<dyn Feature>> {
        self.all
            .iter()
            .filter(|feature| feature.access().admits(self.viewer))
    }

    pub fn nav(&self) -> Vec<NavEntry> {
        self.visible()
            .flat_map(|feature| feature.nav().iter().copied())
            .collect()
    }

    /// Where search forms send their words: the first visible feature that searches.
    pub fn search(&self) -> Option<&'static str> {
        self.visible().find_map(|feature| feature.search())
    }

    /// The footer's line: the first visible feature's that has one.
    pub fn footnote(&self, local: &Local) -> Option<Markup> {
        self.visible().find_map(|feature| feature.footnote(local))
    }

    /// Every visible feature's part of home, fetched at once, in registry order, each with
    /// its name. A feature that overruns the deadline is shown as away.
    pub async fn home(&self, context: &HomeContext) -> Vec<(&'static str, HomeOffer)> {
        let deadline = tokio::time::Instant::from_std(context.deadline) + GRACE;
        join_all(self.visible().map(|feature| async move {
            let offer = tokio::time::timeout_at(deadline, feature.home(context))
                .await
                .unwrap_or_else(|_| HomeOffer {
                    tiles: vec![away_tile(feature.id(), feature.name())],
                    ..HomeOffer::default()
                });
            (feature.name(), offer)
        }))
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn unique<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), &'a str> {
        let mut seen = HashSet::new();
        names
            .into_iter()
            .try_for_each(|name| seen.insert(name).then_some(()).ok_or(name))
    }

    fn config() -> Config {
        use clap::Parser;
        Config::try_parse_from([
            "atrium",
            "--viewer",
            "tailnet",
            "--mneme-url",
            "http://mneme",
        ])
        .unwrap()
    }

    #[test]
    fn feature_ids_are_unique() {
        let features = registry(&config(), "http://mneme").unwrap();
        assert_eq!(unique(features.iter().map(|feature| feature.id())), Ok(()));
    }

    #[test]
    fn navigation_addresses_are_unique() {
        let site = Site::new(
            registry(&config(), "http://mneme").unwrap(),
            Viewer::Tailnet,
        );
        assert_eq!(unique(site.nav().iter().map(|entry| entry.href)), Ok(()));
    }
}
