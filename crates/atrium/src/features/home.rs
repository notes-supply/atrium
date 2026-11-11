//! The home page: every tile the registry's features offer.

use std::sync::Arc;

use atrium_core::{Command, Feature, TileSize};
use atrium_ui::Shell;
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use maud::html;

use super::Site;

pub struct Home;

impl Feature for Home {
    fn id(&self) -> &'static str {
        "home"
    }

    fn name(&self) -> &'static str {
        "Home"
    }

    fn nav(&self) -> Option<&'static str> {
        Some("/")
    }

    fn routes(&self) -> Router {
        Router::new().route("/", get(home))
    }

    fn commands(&self) -> &'static [Command] {
        &[Command {
            label: "Home",
            keys: Some("g h"),
            href: "/",
        }]
    }
}

async fn home(shell: Shell, Extension(site): Extension<Arc<Site>>) -> Response {
    let tiles = site.tiles().await;
    shell.page(
        "Home",
        html! {
            h1 { "Atrium" }
            @if tiles.is_empty() {
                p { "Nothing here yet." }
            } @else {
                ul.tiles {
                    @for tile in &tiles {
                        li id=(tile.id) .tile-wide[tile.size == TileSize::Wide] {
                            article {
                                h2 {
                                    @match tile.link {
                                        Some(href) => a href=(href) { (tile.title) },
                                        None => (tile.title),
                                    }
                                }
                                (tile.body)
                            }
                        }
                    }
                }
            }
        },
    )
}
