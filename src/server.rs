//! Two listeners, one process. The public router only ever mounts share
//! routes, so nothing unshared can leak through Funnel. See docs/sharing.md.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use askama::Template;
use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;

pub async fn run(config: Config) -> Result<()> {
    let private = serve(config.server.private_addr, private_router(), "private");
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

fn private_router() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/healthz", get(|| async { "ok" }))
}

fn public_router() -> Router {
    Router::new()
        .route("/s/{token}", get(shared_album))
        .fallback(not_found)
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate;

async fn home() -> Response {
    render(HomeTemplate)
}

async fn shared_album(Path(_token): Path<String>) -> Response {
    // M6: resolve token → album subtree. Unknown tokens are a plain 404.
    not_found().await
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn render(template: impl Template) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!(%err, "template render failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
