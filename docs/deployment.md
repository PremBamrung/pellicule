# Deployment

On the NAS Pellicule runs with **Docker Compose**, built on the NAS itself.
Tailscale runs on the host. Locally, plain `cargo run` is fine for development.

## Image

Multi-stage `Dockerfile`:

- **build**: `rust:1-slim-trixie` + `libvips-dev`, `cargo build --release --locked`
  with BuildKit cache mounts (registry + target), so rebuilds only recompile
  what changed.
- **runtime**: `debian:trixie-slim` + `libvips`, `ffmpeg`, `tini`. Runs as a
  non-root user; config at `/config/pellicule.toml` (`PELLICULE_CONFIG`).
  `HEALTHCHECK` runs `pellicule healthcheck` (probes `/healthz` on the
  private listener; the image has no curl).

| Container path | Purpose | Mount |
|---|---|---|
| `/photos` | Library | host folder, **read-only** |
| `/cache` | Previews, videos, SQLite | host folder (SSD preferred), writable by `PUID` |
| `/config/pellicule.toml` | Config | bundled; optionally bind-mount your own |

## On the NAS

```sh
git clone https://github.com/PremBamrung/pellicule.git
cd pellicule
cp .env.example .env        # set PHOTOS_DIR, CACHE_DIR, PUID/PGID
mkdir -p cache              # if CACHE_DIR=./cache
docker compose up -d --build

docker compose run --rm pellicule scan    # until serve scans on its own (M1)
docker compose logs -f
```

Update:

```sh
git pull && docker compose up -d --build
```

## Ports and Tailscale

| Host port | Container | Reach |
|---|---|---|
| `8080` | private listener | LAN and tailnet: `http://<nas>:8080` |
| `127.0.0.1:8081` | public listener | Only via Funnel |

Expose share links publicly from the host:

```sh
sudo tailscale funnel --bg 8081    # https://<nas>.<tailnet>.ts.net → 127.0.0.1:8081
tailscale funnel status
```

Because the public port is published on loopback only, nothing on the LAN can
reach it directly, and the public listener itself only routes `/s/<token>/…`.
Optionally, `tailscale serve` can give the private side HTTPS on the tailnet
(on a different port than Funnel's 443).

## Hardware video (later)

The N100's iGPU supports Intel Quick Sync. When video transcoding lands (M5),
uncomment the `devices: /dev/dri` block in `compose.yaml`; the render group of
the host must be able to use it (`group_add` with the host's `render` GID).

## Local development

```sh
brew install rustup vips ffmpeg
cp pellicule.example.toml pellicule.toml   # root = "./samples", cache = "./cache"
cargo run -- scan
cargo run -- serve
```

Or test the container on the Mac against the samples:

```sh
PHOTOS_DIR=./samples docker compose up --build
```

`samples/` and `cache/` are git- and docker-ignored.
