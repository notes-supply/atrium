//! Atrium, a private homepage and dashboard. The binary in `main.rs` parses [`Config`] and
//! calls [`run`]; the tests build the same router with [`app`].

mod assets;
mod features;
mod log;
mod middleware;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use atrium_core::Viewer;
use atrium_ui::{Chrome, Shell};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

use crate::assets::Assets;
use crate::features::Site;

/// Everything the server is told at start, from flags or the environment.
#[derive(Debug, Clone, clap::Parser)]
#[command(version, about)]
pub struct Config {
    /// The address to listen on.
    #[arg(long, env = "ATRIUM_ADDR", default_value = "0.0.0.0:8080")]
    pub addr: SocketAddr,
    /// Who reaches this listener: `tailnet` sees private features, `anonymous` only public ones.
    #[arg(long, env = "ATRIUM_VIEWER")]
    pub viewer: Viewer,
    /// The host names requests may name, separated by commas. Health checks may name any.
    #[arg(
        long,
        env = "ATRIUM_ALLOWED_HOSTS",
        value_delimiter = ',',
        default_value = "localhost"
    )]
    pub allowed_hosts: Vec<String>,
}

/// Serves until SIGTERM or Ctrl-C, then finishes the requests in flight.
///
/// # Errors
///
/// Fails when the address cannot be bound or the server stops on an I/O error.
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watching for SIGTERM")?;
    let router = app(&config);
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
pub fn app(config: &Config) -> Router {
    let mut assets = Assets::default();
    let stylesheet = assets.add("atrium", "css", atrium_ui::STYLESHEET.as_bytes());
    let site = Arc::new(Site::new(features::registry(), config.viewer));
    let chrome = Arc::new(Chrome {
        nav: site.nav(),
        commands: site.commands(),
        stylesheet,
    });
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
        .layer(Extension(site))
        .layer(Extension(chrome))
        .layer(Extension(config.viewer))
        .layer(axum::middleware::from_fn_with_state(
            Arc::from(config.allowed_hosts.as_slice()),
            middleware::known_host,
        ));
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(pages)
        .layer(axum::middleware::from_fn(middleware::headers))
        .layer(axum::middleware::from_fn(middleware::request_log))
}

async fn not_found(shell: Shell) -> Response {
    shell.not_found()
}
