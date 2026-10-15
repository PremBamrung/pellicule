mod config;
mod library;
mod server;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// A slim, fast, self-hosted photography gallery.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, global = true, default_value = "pellicule.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Index the library and generate derivatives into the cache.
    Scan,
    /// Start the private (tailnet) and public (share links) listeners.
    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("pellicule=info")),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;

    match cli.command {
        Command::Scan => {
            tracing::info!(
                root = %config.library.root.display(),
                cache = %config.library.cache.display(),
                "scanning library"
            );
            let summary = library::scan(&config.library.root)?;
            tracing::info!(
                albums = summary.albums,
                images = summary.images,
                videos = summary.videos,
                "scan complete"
            );
        }
        Command::Serve => server::run(config).await?,
    }

    Ok(())
}
