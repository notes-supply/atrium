//! The home page: the date, one sentence that sums up the state of things, the command line,
//! and every feature's drawing and tiles below.

use std::sync::Arc;
use std::time::{Duration, Instant};

use atrium_core::{Feature, HomeContext, NavEntry};
use atrium_ui::components::cmd_line;
use atrium_ui::{MAIN, Page, Shell};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use maud::html;

use super::Site;

/// How long home waits for its tiles. A tile that is not ready by then is shown as away.
const BUDGET: Duration = Duration::from_millis(500);

pub struct Home;

impl Feature for Home {
    fn id(&self) -> &'static str {
        "home"
    }

    fn name(&self) -> &'static str {
        "Home"
    }

    fn nav(&self) -> &'static [NavEntry] {
        &[NavEntry {
            label: "Home",
            href: "/",
        }]
    }

    fn routes(&self) -> Router {
        Router::new().route("/", get(home))
    }
}

/// The greeting the sentence opens with.
fn greeting(hour: u8) -> &'static str {
    match hour {
        0..5 => "Still up.",
        5..12 => "Good morning.",
        12..18 => "Good afternoon.",
        _ => "Good evening.",
    }
}

async fn home(shell: Shell, Extension(site): Extension<Arc<Site>>) -> Response {
    let context = HomeContext {
        visit: shell.visit().clone(),
        deadline: Instant::now() + BUDGET,
    };
    let offers = site.home(&context).await;
    let away = offers.iter().any(|(_, offer)| offer.away);
    let local = shell.local();
    let today = local.now().date();
    let names: Vec<String> = offers.iter().map(|(name, _)| name.to_lowercase()).collect();
    let tiles_label = if away {
        format!("While the {} is away", names.join(" and "))
    } else {
        format!("The {}", names.join(" and "))
    };
    let body = html! {
        main #(MAIN) .home.away[away] {
            section.hero aria-label="Today" {
                div.lede {
                    h1.label { time datetime=(today) { (local.date_long(local.seconds())) } }
                    p.sentence {
                        span data-module="greet" { (greeting(local.hour())) }
                        @for (_, offer) in &offers {
                            @for clause in &offer.said { " " (clause) }
                        }
                    }
                    (cmd_line("cmd", shell.search()))
                }
                @for (_, offer) in &offers {
                    @if let Some(figure) = &offer.figure { (figure) }
                }
            }
            section.tiles aria-label=(tiles_label) {
                @for (_, offer) in &offers {
                    @for tile in &offer.tiles { (tile) }
                }
            }
        }
    };
    let mut page = Page::new(
        "Atrium",
        "Home: what changed, what wants you, and where you left off.",
        body,
    );
    page.since = !away;
    if away {
        page.description = "The wiki is briefly out of reach. The rest of Atrium works.";
        page.foot = Some(html! {
            "The " (names.join(" and ")) " did not answer. Everything else here is served by Atrium itself."
        });
    }
    shell.page(page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_greeting_follows_the_hour() {
        assert_eq!(greeting(3), "Still up.");
        assert_eq!(greeting(9), "Good morning.");
        assert_eq!(greeting(14), "Good afternoon.");
        assert_eq!(greeting(20), "Good evening.");
    }
}
