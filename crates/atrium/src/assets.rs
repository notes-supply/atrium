//! Static files under names that carry a hash of their content, so a browser may keep each
//! one forever.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::middleware::hex;

/// The module loader, the only script a page loads.
pub const LOADER: &[u8] = include_bytes!("../../../web/loader.js");
/// The palette's glue and module, as `mise run build:wasm` leaves them.
pub const PALETTE_GLUE: &[u8] = include_bytes!("../../../target/web/palette.js");
pub const PALETTE_WASM: &[u8] = include_bytes!("../../../target/web/palette_bg.wasm");

/// Every asset by its hashed name, such as `atrium.0123456789abcdef.css`.
#[derive(Debug, Default)]
pub struct Assets(HashMap<String, Asset>);

#[derive(Debug)]
struct Asset {
    bytes: &'static [u8],
    content_type: &'static str,
}

impl Assets {
    /// Adds `bytes` as `stem` with `extension`, and returns the URL it is served at.
    pub fn add(&mut self, stem: &str, extension: &str, bytes: &'static [u8]) -> String {
        let content_type = match extension {
            "css" => "text/css; charset=utf-8",
            "js" => "text/javascript; charset=utf-8",
            "wasm" => "application/wasm",
            _ => "application/octet-stream",
        };
        let name = format!("{stem}.{}.{extension}", hex(&Sha256::digest(bytes)[..8]));
        let url = format!("/assets/{name}");
        self.0.insert(
            name,
            Asset {
                bytes,
                content_type,
            },
        );
        url
    }
}

/// Serves one asset, cached for a year and never revalidated.
pub async fn serve(State(assets): State<Arc<Assets>>, Path(name): Path<String>) -> Response {
    match assets.0.get(&name) {
        Some(asset) => (
            [
                (CONTENT_TYPE, asset.content_type),
                (CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            asset.bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
