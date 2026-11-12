//! Atrium, a private homepage and dashboard. The binary in `main.rs` parses [`Config`] and
//! calls [`run`]; the tests build the same router with [`app`].

mod assets;
mod features;
mod log;
mod middleware;
mod visit;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use atrium_core::Viewer;
use atrium_ui::{Chrome, Footnote, Shell};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

use jiff::tz::TimeZone;

use crate::assets::Assets;
use crate::features::Site;
use crate::visit::{Clock, Visits};

/// Everything the server is told at start, from flags or the environment.
#[derive(Debug, Clone, clap::Parser)]
#[command(version, about)]
pub struct Config {
    /// The address to listen on. Loopback only by default; the image sets `0.0.0.0:8080`.
    #[arg(long, env = "ATRIUM_ADDR", default_value = "127.0.0.1:8080")]
    pub addr: SocketAddr,
    /// Who reaches this listener: `tailnet` sees private features, `anonymous` only public ones.
    #[arg(long, env = "ATRIUM_VIEWER")]
    pub viewer: Viewer,
    /// The host names requests may name, separated by commas, each with any port. An IPv6
    /// address may be written with or without its brackets. Health checks may name any.
    #[arg(
        long,
        env = "ATRIUM_ALLOWED_HOSTS",
        value_delimiter = ',',
        default_values = ["localhost", "127.0.0.1", "[::1]"]
    )]
    pub allowed_hosts: Vec<String>,
    /// Mneme's base URL, such as `http://mneme:8080`.
    #[arg(long, env = "ATRIUM_MNEME_URL", required_unless_present = "demo")]
    pub mneme_url: Option<String>,
    /// How long one call to Mneme may take, in milliseconds.
    #[arg(long, env = "ATRIUM_MNEME_TIMEOUT_MS", default_value_t = 3000)]
    pub mneme_timeout_ms: u64,
    /// The reader's time zone, for every time a page shows and the light of the hour. The
    /// system's by default, from `TZ` or `/etc/localtime`, or UTC when it names none; the
    /// demo's own is Europe/Stockholm.
    #[arg(long, env = "ATRIUM_TIME_ZONE", value_parser = zone)]
    pub time_zone: Option<TimeZone>,
    /// Serve the design's invented wiki instead of a Mneme, with the clock held at its time.
    /// `away` shows the wiki out of reach.
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "wiki")]
    pub demo: Option<Demo>,
}

/// What the demo shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Demo {
    /// The design's invented wiki.
    Wiki,
    /// The same, with the wiki out of reach.
    Away,
}

fn zone(name: &str) -> Result<TimeZone, String> {
    TimeZone::get(name).map_err(|error| format!("{name}: {error}"))
}

impl Config {
    /// The reader's time zone: the one named, or the demo's, whose clock is set in it, or the
    /// system's, or UTC when the system names none.
    #[must_use]
    pub fn zone(&self) -> TimeZone {
        match (&self.time_zone, self.demo) {
            (Some(zone), _) => zone.clone(),
            (None, Some(_)) => atrium_wiki::demo::zone(),
            (None, None) => TimeZone::try_system().unwrap_or(TimeZone::UTC),
        }
    }
}

/// Serves until SIGTERM or Ctrl-C, then finishes the requests in flight.
///
/// # Errors
///
/// Fails when the address cannot be bound or the server stops on an I/O error.
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watching for SIGTERM")?;
    let router = app(&config).await?;
    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("binding {}", config.addr))?;
    log::emit(
        "listening",
        json!({ "addr": listener.local_addr()?, "viewer": config.viewer.to_string() }),
    );
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        })
        .await?;
    log::emit("stopped", json!({}));
    Ok(())
}

/// The whole site for the viewer `config` names, as one router.
///
/// # Errors
///
/// Fails when a feature cannot be built from the configuration.
pub async fn app(config: &Config) -> Result<Router> {
    let mut assets = Assets::default();
    let (stylesheet, fonts) = assets.add_styles();
    let scripts = assets.add_scripts();
    let (mneme_url, clock) = match config.demo {
        Some(demo) => (
            atrium_wiki::demo::serve(demo == Demo::Away).await?,
            demo_clock(),
        ),
        None => (config.mneme_url.clone().unwrap_or_default(), Clock::System),
    };
    let site = Arc::new(Site::new(
        features::registry(config, &mneme_url)?,
        config.viewer,
    ));
    let footnotes = Arc::clone(&site);
    let chrome = Arc::new(Chrome {
        nav: site.nav(),
        search: site.search(),
        stylesheet,
        fonts,
        scripts,
        footnote: Footnote(Arc::new(move |local| footnotes.footnote(local))),
    });
    let visits = Visits {
        clock,
        zone: config.zone(),
    };
    let pages = site
        .all()
        .iter()
        .fold(Router::new(), |router, feature| {
            router.merge(middleware::guard(feature.routes(), feature.access()))
        })
        .route(
            "/assets/{name}",
            get(assets::serve).with_state(Arc::new(assets)),
        )
        .fallback(not_found)
        .layer(axum::middleware::from_fn(middleware::etag))
        .layer(axum::middleware::from_fn_with_state(visits, visit::visit))
        .layer(Extension(site))
        .layer(Extension(chrome))
        .layer(Extension(config.viewer))
        .layer(axum::middleware::from_fn_with_state(
            Arc::from(config.allowed_hosts.as_slice()),
            middleware::known_host,
        ));
    Ok(Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(pages)
        .layer(axum::middleware::from_fn(middleware::headers))
        .layer(axum::middleware::from_fn(middleware::request_log)))
}

/// The demo's now and the visit before it, from the design's invented content.
fn demo_clock() -> Clock {
    Clock::Fixed {
        now: atrium_wiki::demo::NOW,
        last: atrium_wiki::demo::LAST_VISIT,
    }
}

async fn not_found(shell: Shell) -> Response {
    shell.not_found()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn parse(args: &[&str]) -> Config {
        Config::try_parse_from([&["atrium", "--viewer", "tailnet"], args].concat()).unwrap()
    }

    #[test]
    fn it_listens_on_loopback_for_loopback_names_by_default() {
        let config = parse(&["--mneme-url", "http://mneme"]);
        assert_eq!(config.addr, SocketAddr::from(([127, 0, 0, 1], 8080)));
        assert_eq!(config.allowed_hosts, ["localhost", "127.0.0.1", "[::1]"]);
        let named = parse(&[
            "--mneme-url",
            "http://mneme",
            "--allowed-hosts",
            "a.example,b",
        ]);
        assert_eq!(named.allowed_hosts, ["a.example", "b"]);
    }

    #[test]
    fn the_demo_keeps_its_zone_unless_another_is_named() {
        assert_eq!(
            parse(&["--demo"]).zone().iana_name(),
            Some("Europe/Stockholm")
        );
        assert_eq!(
            parse(&["--demo", "--time-zone", "Asia/Tokyo"])
                .zone()
                .iana_name(),
            Some("Asia/Tokyo")
        );
    }
}
