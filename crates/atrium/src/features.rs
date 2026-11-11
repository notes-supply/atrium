//! The registry: every feature, listed once. The router, the navigation, the home page and
//! the command palette all come from this list.

mod clock;
mod home;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use atrium_core::{Command, Feature, NavEntry, Tile, Viewer};
use atrium_wiki::Wiki;
use futures_util::future::join_all;

use crate::Config;

/// Every feature, in the order the navigation lists them.
pub fn registry(config: &Config) -> Result<Vec<Arc<dyn Feature>>> {
    let timeout = Duration::from_millis(config.mneme_timeout_ms);
    Ok(vec![
        Arc::new(home::Home),
        Arc::new(Wiki::new(&config.mneme_url, timeout)?),
        Arc::new(clock::Clock),
    ])
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
            .filter_map(|feature| {
                feature.nav().map(|href| NavEntry {
                    label: feature.name(),
                    href,
                })
            })
            .collect()
    }

    pub fn commands(&self) -> Vec<Command> {
        self.visible()
            .flat_map(|feature| feature.commands().iter().copied())
            .collect()
    }

    /// Every visible feature's tiles, fetched at once, in registry order.
    pub async fn tiles(&self) -> Vec<Tile> {
        join_all(self.visible().map(|feature| feature.tiles()))
            .await
            .into_iter()
            .flatten()
            .collect()
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
        let features = registry(&config()).unwrap();
        assert_eq!(unique(features.iter().map(|feature| feature.id())), Ok(()));
    }

    #[test]
    fn command_keys_are_unique() {
        let site = Site::new(registry(&config()).unwrap(), Viewer::Tailnet);
        let commands = site.commands();
        assert_eq!(
            unique(commands.iter().filter_map(|command| command.keys)),
            Ok(())
        );
    }
}
