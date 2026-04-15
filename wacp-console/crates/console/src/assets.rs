//! Frontend asset serving — embedded in production, disk-served in dev.
//!
//! Two modes, selected at startup via `--frontend-path`:
//! - `FrontendMode::Embedded`: assets are compiled into the release binary
//!   via `rust-embed`. The folder `wacp-console/frontend/dist/` must exist
//!   at cargo-build time; populated by Vite (`pnpm build`).
//! - `FrontendMode::Disk(path)`: read from a runtime-supplied directory.
//!   Intended for local dev where the SPA is rebuilt often and we'd rather
//!   skip the cargo rebuild cycle.
//!
//! Both modes serve `index.html` as the SPA history-mode fallback for any
//! path that doesn't map to a static file.

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../frontend/dist/"]
struct FrontendAssets;

#[derive(Clone)]
pub enum FrontendMode {
    Embedded,
    Disk(Arc<PathBuf>),
}

pub async fn serve_frontend(mode: FrontendMode, uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };

    match mode {
        FrontendMode::Disk(root) => serve_disk(&root, path).await,
        FrontendMode::Embedded => serve_embedded(path),
    }
}

async fn serve_disk(root: &Path, path: &str) -> Response {
    match fs::read(root.join(path)).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime_from_ext(path))], bytes).into_response(),
        Err(_) => match fs::read(root.join("index.html")).await {
            Ok(bytes) => {
                ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response()
            }
            Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
        },
    }
}

fn serve_embedded(path: &str) -> Response {
    if let Some(file) = FrontendAssets::get(path) {
        return (
            [(header::CONTENT_TYPE, mime_from_ext(path))],
            file.data.into_owned(),
        )
            .into_response();
    }
    match FrontendAssets::get("index.html") {
        Some(file) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            file.data.into_owned(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

fn mime_from_ext(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
