use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub library: LibraryConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub scan: ScanConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ScanConfig {
    /// Files processed in parallel. Defaults to half the CPU cores, so a
    /// scan never takes the whole machine.
    workers: Option<usize>,
}

impl ScanConfig {
    pub fn workers(&self) -> usize {
        self.workers
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get() / 2))
            .max(1)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryConfig {
    /// Root of the photo library. Only ever read, never written.
    pub root: PathBuf,
    /// Where previews, transcoded videos and the SQLite index live.
    pub cache: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    /// Full gallery; reachable on LAN / tailnet.
    pub private_addr: SocketAddr,
    /// Share links only; meant to be exposed with `tailscale funnel`.
    pub public_addr: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            private_addr: ([0, 0, 0, 0], 8080).into(),
            public_addr: ([127, 0, 0, 1], 8081).into(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_defaults_apply() {
        let config: Config = toml::from_str(
            r#"
            [library]
            root = "/photos"
            cache = "/var/lib/pellicule"
            "#,
        )
        .unwrap();
        assert_eq!(config.server.private_addr.port(), 8080);
        assert!(config.server.public_addr.ip().is_loopback());
    }
}
