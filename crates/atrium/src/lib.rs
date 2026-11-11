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
use atrium_ui::{Chrome, Module, Shell};
use axum::body::Bytes;
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
    /// Mneme's base URL, such as `http://mneme:8080`.
    #[arg(long, env = "ATRIUM_MNEME_URL")]
    pub mneme_url: String,
    /// How long one call to Mneme may take, in milliseconds.
    #[arg(long, env = "ATRIUM_MNEME_TIMEOUT_MS", default_value_t = 3000)]
    pub mneme_timeout_ms: u64,
}

/// Serves until SIGTERM or Ctrl-C, then finishes the requests in flight.
///
/// # Errors
///
/// Fails when the address cannot be bound or the server stops on an I/O error.
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watching for SIGTERM")?;
    let router = app(&config)?;
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
pub fn app(config: &Config) -> Result<Router> {
    let mut assets = Assets::default();
    let (stylesheet, _fonts) = assets.add_styles();
    let loader = assets.add("loader", "js", Bytes::from_static(assets::LOADER));
    let palette = Module {
        glue: assets.add("palette", "js", Bytes::from_static(assets::PALETTE_GLUE)),
        wasm: assets.add(
            "palette_bg",
            "wasm",
            Bytes::from_static(assets::PALETTE_WASM),
        ),
    };
    let site = Arc::new(Site::new(features::registry(config)?, config.viewer));
    let chrome = Arc::new(Chrome {
        nav: site.nav(),
        commands: site.commands(),
        stylesheet,
        loader,
        palette,
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
    Ok(Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(pages)
        .layer(axum::middleware::from_fn(middleware::headers))
        .layer(axum::middleware::from_fn(middleware::request_log)))
}

async fn not_found(shell: Shell) -> Response {
    shell.not_found()
}
