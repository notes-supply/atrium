//! A clock: the server's time in UTC, as a tile and a page.

use std::time::{SystemTime, UNIX_EPOCH};

use atrium_core::{Command, Feature, Tile, TileSize, Tiles};
use atrium_ui::{Shell, time};
use axum::Router;
use axum::response::Response;
use axum::routing::get;
use maud::{Markup, html};

pub struct Clock;

fn now() -> Markup {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    html! { p { (time(i64::try_from(seconds).unwrap_or(i64::MAX))) " UTC" } }
}

impl Feature for Clock {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn name(&self) -> &'static str {
        "Clock"
    }

    fn nav(&self) -> Option<&'static str> {
        Some("/clock")
    }

    fn routes(&self) -> Router {
        Router::new().route("/clock", get(page))
    }

    fn tiles(&self) -> Tiles<'_> {
        Box::pin(async {
            vec![Tile {
                id: "clock",
                title: "Time",
                size: TileSize::Narrow,
                body: now(),
                link: Some("/clock"),
            }]
        })
    }

    fn commands(&self) -> &'static [Command] {
        &[Command {
            label: "Clock",
            keys: Some("g t"),
            href: "/clock",
        }]
    }
}

async fn page(shell: Shell) -> Response {
    shell.page("Clock", html! { h1 { "Clock" } (now()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_time_in_utc() {
        let shown = now().into_string();
        assert!(shown.starts_with("<p><time datetime=\""), "{shown}");
        assert!(shown.contains("Z\">"), "a UTC timestamp: {shown}");
        assert!(shown.ends_with("</time> UTC</p>"), "{shown}");
    }
}
