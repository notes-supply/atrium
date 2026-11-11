//! Atrium, a private homepage and dashboard. The binary in `main.rs` parses [`Config`] and
//! calls [`run`]; the tests build the same router with [`app`].

mod log;

use std::net::SocketAddr;
use std::time::Instant;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

/// Everything the server is told at start, from flags or the environment.
#[derive(Debug, Clone, clap::Parser)]
#[command(version, about)]
pub struct Config {
    /// The address to listen on.
    #[arg(long, env = "ATRIUM_ADDR", default_value = "0.0.0.0:8080")]
    pub addr: SocketAddr,
}

/// Serves until SIGTERM or Ctrl-C, then finishes the requests in flight.
///
/// # Errors
///
/// Fails when the address cannot be bound or the server stops on an I/O error.
pub async fn run(config: Config) -> Result<()> {
    let mut terminate = signal(SignalKind::terminate()).context("watching for SIGTERM")?;
    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("binding {}", config.addr))?;
    log::emit("listening", json!({ "addr": listener.local_addr()? }));
    axum::serve(listener, app(&config))
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

/// The whole site as one router.
pub fn app(_config: &Config) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn(request_log))
}

/// One line per request. The query string is left out, since it can hold search words.
async fn request_log(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    log::emit(
        "request",
        json!({
            "method": method,
            "path": path,
            "status": response.status().as_u16(),
            "ms": start.elapsed().as_millis(),
        }),
    );
    response
}
