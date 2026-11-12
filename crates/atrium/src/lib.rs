//! Atrium, a private homepage and dashboard. The binary in `main.rs` parses [`Config`] and
//! calls [`run`]; the tests build the same router with [`app`].

mod assets;
mod features;
mod local;
mod log;
mod middleware;
mod visit;

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use atrium_core::Viewer;
use atrium_ui::{Chrome, Footnote, Shell};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use clap::builder::ArgPredicate;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::oneshot;

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
    /// Who reaches this listener: `owner`, the person this Atrium belongs to, sees private
    /// features, and `anonymous` only public ones. `tailnet` is another name for `owner`.
    /// `owner` by default with `--wiki-folder`.
    #[arg(
        long,
        env = "ATRIUM_VIEWER",
        required = false,
        required_unless_present = "wiki_folder",
        default_value_if("wiki_folder", ArgPredicate::IsPresent, "owner")
    )]
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
    #[arg(long, env = "ATRIUM_MNEME_URL", required_unless_present_any = ["demo", "wiki_folder"])]
    pub mneme_url: Option<String>,
    /// A wiki folder on this machine, a Git clone, to read through a `mneme serve --folder`
    /// that Atrium starts and stops itself. The pages show the whole wiki, so `--addr` must
    /// then be a loopback address.
    #[arg(
        long,
        env = "ATRIUM_WIKI_FOLDER",
        value_name = "DIR",
        conflicts_with_all = ["mneme_url", "demo"]
    )]
    pub wiki_folder: Option<PathBuf>,
    /// The Mneme binary that `--wiki-folder` starts.
    #[arg(
        long,
        env = "ATRIUM_MNEME_BIN",
        value_name = "PATH",
        default_value = "mneme"
    )]
    pub mneme_bin: PathBuf,
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

/// Serves until SIGTERM or Ctrl-C, then finishes the requests in flight. With
/// `--wiki-folder` it first starts a Mneme for the folder, and stops it last.
///
/// # Errors
///
/// Fails when the address cannot be bound, the server stops on an I/O error, or the Mneme
/// it started cannot start or stops while Atrium runs.
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watching for SIGTERM")?;
    let mut mneme = None;
    let router = match &config.wiki_folder {
        Some(folder) => {
            if !config.addr.ip().is_loopback() {
                bail!(
                    "--wiki-folder shows the whole wiki to whoever reaches Atrium, with no \
                     login, so --addr must be a loopback address such as 127.0.0.1:8080, not {}",
                    config.addr
                );
            }
            // A stop asked for while Mneme builds its index drops it, which kills it.
            let started = tokio::select! {
                started = local::Mneme::start(&config.mneme_bin, folder) => started?,
                () = asked_to_stop(&mut terminate) => {
                    log::emit("stopped", json!({}));
                    return Ok(());
                }
            };
            let router = site(&config, started.url(), Clock::System)?;
            mneme = Some(started);
            router
        }
        None => app(&config).await?,
    };
    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("binding {}", config.addr))?;
    log::emit(
        "listening",
        json!({ "addr": listener.local_addr()?, "viewer": config.viewer.to_string() }),
    );
    let (stop, stopping) = oneshot::channel::<()>();
    let server = axum::serve(listener, router).with_graceful_shutdown(async {
        let _ = stopping.await;
    });
    let mut server = pin!(server.into_future());
    tokio::select! {
        biased;
        () = asked_to_stop(&mut terminate) => {}
        ended = &mut server => return ended.context("serving"),
        // Atrium is no use without it, so it stops too, and a supervisor restarts both.
        error = stopped(&mut mneme) => return Err(error),
    }
    let _ = stop.send(());
    server.await.context("serving")?;
    if let Some(mneme) = mneme {
        mneme.stop().await;
    }
    log::emit("stopped", json!({}));
    Ok(())
}

/// Until SIGTERM or Ctrl-C.
async fn asked_to_stop(terminate: &mut Signal) {
    tokio::select! {
        _ = terminate.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}

/// Why the Mneme that Atrium started stopped, once it has; never without one.
async fn stopped(mneme: &mut Option<local::Mneme>) -> anyhow::Error {
    match mneme {
        Some(mneme) => mneme.stopped().await,
        None => std::future::pending().await,
    }
}

/// The whole site for the viewer `config` names, as one router, in front of the Mneme at
/// `--mneme-url` or the demo's. A wiki folder's Mneme is started by [`run`].
///
/// # Errors
///
/// Fails when a feature cannot be built from the configuration, or the configuration names
/// a wiki folder.
pub async fn app(config: &Config) -> Result<Router> {
    match (config.demo, &config.mneme_url) {
        (Some(demo), _) => {
            let url = atrium_wiki::demo::serve(demo == Demo::Away).await?;
            site(config, &url, demo_clock())
        }
        (None, Some(url)) => site(config, url, Clock::System),
        (None, None) => bail!("run starts the Mneme of a wiki folder and serves in front of it"),
    }
}

/// The whole site in front of the Mneme at `mneme_url`, on `clock`.
fn site(config: &Config, mneme_url: &str, clock: Clock) -> Result<Router> {
    let mut assets = Assets::default();
    let (stylesheet, fonts) = assets.add_styles();
    let scripts = assets.add_scripts();
    let site = Arc::new(Site::new(
        features::registry(config, mneme_url)?,
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
        Config::try_parse_from([&["atrium", "--viewer", "owner"], args].concat()).unwrap()
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
