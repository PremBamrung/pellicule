mod config;
mod db;
mod derive;
mod library;
mod meta;
mod pages;
mod scan;
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
    #[arg(
        short,
        long,
        global = true,
        env = "PELLICULE_CONFIG",
        default_value = "pellicule.toml"
    )]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Index the library and generate derivatives into the cache.
    Scan {
        /// Reprocess every file, not only new or changed ones.
        #[arg(long)]
        rebuild: bool,
    },
    /// Start the private (tailnet) and public (share links) listeners.
    Serve,
    /// Exit 0 if the running server answers on its private listener.
    Healthcheck,
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

    // One libvips instance for the whole process. Parallelism comes from
    // processing several files at once, so each operation stays
    // single-threaded; the operation cache only grows memory during a scan.
    let vips = libvips::VipsApp::new("pellicule", false)?;
    vips.concurrency_set(1);
    vips.cache_set_max(0);
    // Never dropped: dropping it calls `vips_shutdown`, and when `main`
    // returns (an error, Ctrl-C, `docker stop`) scan workers may still be
    // inside libvips. The OS reclaims everything at exit.
    std::mem::forget(vips);

    match cli.command {
        Command::Scan { rebuild } => {
            tracing::info!(
                root = %config.library.root.display(),
                cache = %config.library.cache.display(),
                "scanning library"
            );
            let mut conn = db::open(&config.library.cache)?;
            let options = scan::ScanOptions {
                trigger: scan::Trigger::Cli,
                rebuild,
            };
            let control = scan::ScanControl::default();
            control.try_start(options);
            scan::mark_interrupted(&conn)?;
            scan::run(&config, &mut conn, options, &control)?;
        }
        Command::Serve => server::run(config).await?,
        Command::Healthcheck => server::healthcheck(&config).await?,
    }

    Ok(())
}
