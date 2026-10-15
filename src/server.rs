//! Two listeners, one process. The public router only ever mounts share
//! routes, so nothing unshared can leak through Funnel. See docs/sharing.md.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use askama::Template;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use rusqlite::Connection;
use rust_embed::RustEmbed;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use crate::pages::{self, Layout};
use crate::{db, derive, scan};

pub async fn run(config: Config) -> Result<()> {
    let conn = db::open(&config.library.cache)?;
    let state = AppState {
        cache: Arc::new(config.library.cache.clone()),
        db: Arc::new(Mutex::new(conn)),
        layout: Arc::new(Layout {
            css_url: asset_url("app.css"),
            js_url: asset_url("app.js"),
        }),
    };

    // Index in the background so the gallery is up immediately; pages fill
    // in as the scan commits.
    let scan_config = config.clone();
    tokio::task::spawn_blocking(move || {
        let result = db::open(&scan_config.library.cache)
            .and_then(|mut conn| scan::run(&scan_config, &mut conn));
        if let Err(err) = result {
            tracing::error!("background scan failed: {err:#}");
        }
    });

    let private = serve(config.server.private_addr, private_router(state), "private");
    let public = serve(config.server.public_addr, public_router(), "public");

    tokio::select! {
        result = private => result,
        result = public => result,
        _ = shutdown_signal() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
}

/// Ctrl-C locally, SIGTERM from `docker stop`.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}

/// Probes the private listener's `/healthz`; used by the Docker HEALTHCHECK
/// since the runtime image has no curl.
pub async fn healthcheck(config: &Config) -> Result<()> {
    let mut addr = config.server.private_addr;
    if addr.ip().is_unspecified() {
        addr.set_ip(if addr.is_ipv4() {
            Ipv4Addr::LOCALHOST.into()
        } else {
            Ipv6Addr::LOCALHOST.into()
        });
    }

    let probe = async {
        let mut stream = TcpStream::connect(addr).await?;
        stream
            .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        anyhow::Ok(response)
    };
    let response = tokio::time::timeout(Duration::from_secs(3), probe)
        .await
        .context("healthcheck timed out")??;

    if !response.starts_with(b"HTTP/1.1 200") {
        bail!(
            "unhealthy: {}",
            String::from_utf8_lossy(&response[..response.len().min(64)])
        );
    }
    Ok(())
}

async fn serve(addr: SocketAddr, router: Router, name: &str) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {name} listener on {addr}"))?;
    tracing::info!(%addr, "{name} listener ready");
    axum::serve(listener, router).await?;
    Ok(())
}

fn private_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/a/{*path}", get(album))
        .route("/d/{file}", get(derivative))
        .route("/static/{*path}", get(static_asset))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(not_found)
        .with_state(state)
}

fn public_router() -> Router {
    Router::new()
        .route("/s/{token}", get(shared_album))
        .fallback(not_found)
}

#[derive(Clone)]
struct AppState {
    cache: Arc<PathBuf>,
    db: Arc<Mutex<Connection>>,
    layout: Arc<Layout>,
}

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

/// `/static/<path>?v=<content hash>`, so assets can be cached forever.
fn asset_url(path: &str) -> String {
    let version = Assets::get(path)
        .map(|file| hex(&file.metadata.sha256_hash()[..5]))
        .unwrap_or_default();
    format!("/static/{path}?v={version}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Runs a page query on the blocking pool (SQLite is synchronous).
async fn with_db<T: Send + 'static>(
    state: &AppState,
    query: impl FnOnce(&Connection, &Layout) -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    let db = state.db.clone();
    let layout = state.layout.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().expect("database mutex poisoned");
        query(&conn, &layout)
    })
    .await?
}

async fn home(State(state): State<AppState>) -> Response {
    let html = with_db(&state, |conn, layout| {
        Ok(pages::home(conn, layout)?.render()?)
    })
    .await;
    html_response(html)
}

async fn album(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    let html = with_db(&state, move |conn, layout| {
        pages::album(conn, layout, &path)?
            .map(|page| page.render())
            .transpose()
            .map_err(Into::into)
    })
    .await;
    match html {
        Ok(Some(html)) => html_response(Ok(html)),
        Ok(None) => not_found().await,
        Err(err) => html_response(Err(err)),
    }
}

fn html_response(html: anyhow::Result<String>) -> Response {
    match html {
        Ok(html) => ([(header::CACHE_CONTROL, "no-cache")], Html(html)).into_response(),
        Err(err) => {
            tracing::error!("rendering page: {err:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Derivatives are content-addressed, so their URLs never change meaning:
/// cache them forever.
async fn derivative(
    State(state): State<AppState>,
    Path(file): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((key, rest)) = file.split_once('-') else {
        return not_found().await;
    };
    let valid = key.len() == 32
        && key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && rest
            .split_once('.')
            .and_then(|(variant, _)| derive::variant(variant))
            .is_some_and(|variant| derive::file_name(key, variant) == file);
    if !valid {
        return not_found().await;
    }

    let etag = format!("\"{file}\"");
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|value| value.as_bytes() == etag.as_bytes())
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }

    let path = state
        .cache
        .join("d")
        .join(&key[0..2])
        .join(&key[2..4])
        .join(&file);
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/webp".to_string()),
                (header::CACHE_CONTROL, IMMUTABLE.to_string()),
                (header::ETAG, etag),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => not_found().await,
    }
}

const IMMUTABLE: &str = "public, max-age=31536000, immutable";

async fn static_asset(
    Path(path): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let Some(file) = Assets::get(&path) else {
        return not_found().await;
    };
    let etag = format!("\"{}\"", hex(&file.metadata.sha256_hash()[..8]));
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|value| value.as_bytes() == etag.as_bytes())
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    let cache_control = if query.contains_key("v") {
        IMMUTABLE
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, content_type(&path).to_string()),
            (header::CACHE_CONTROL, cache_control.to_string()),
            (header::ETAG, etag),
        ],
        file.data,
    )
        .into_response()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("svg") => "image/svg+xml",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

async fn shared_album(Path(_token): Path<String>) -> Response {
    // M6: resolve token → album subtree. Unknown tokens are a plain 404.
    not_found().await
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}
