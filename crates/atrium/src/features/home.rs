//! The home page: the date, one sentence that sums up the state of things, the command line,
//! and every feature's drawing and tiles below.

use std::sync::Arc;
use std::time::{Duration, Instant};

use atrium_core::{Feature, HomeContext, NavEntry};
use atrium_ui::Shell;
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};

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

async fn home(shell: Shell, Extension(site): Extension<Arc<Site>>) -> Response {
    let context = HomeContext {
        visit: shell.visit().clone(),
        deadline: Instant::now() + BUDGET,
    };
    let offers = site.home(&context).await;
    shell.page(atrium_ui::home(&shell, &offers))
}
