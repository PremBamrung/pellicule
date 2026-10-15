//! Two listeners, one process. The public router only ever mounts share
//! routes, so nothing unshared can leak through Funnel. See docs/sharing.md.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use askama::Template;
use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use tokio::net::TcpListener;

use crate::config::Config;

pub async fn run(config: Config) -> Result<()> {
    let private = serve(config.server.private_addr, private_router(), "private");
    let public = serve(config.server.public_addr, public_router(), "public");

    tokio::select! {
        result = private => result,
        result = public => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutting down");
            Ok(())
        }
    }
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
    Router::new().route("/", get(home))
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
