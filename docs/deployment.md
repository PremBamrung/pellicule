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
| `/config/pellicule.toml` | Config | `docker/pellicule.toml`, or the file named by `PELLICULE_TOML` in `.env` |

## On the NAS

```sh
git clone https://github.com/PremBamrung/pellicule.git
cd pellicule
cp .env.example .env        # set PHOTOS_DIR, CACHE_DIR, PUID/PGID
mkdir -p cache              # if CACHE_DIR=./cache
docker compose up -d --build

docker compose logs -f
```

`serve` scans the library in the background when it starts. Scans can then be
started, followed and cancelled from the **Library** page
(`http://<nas>:8080/library`): *Rescan* processes new, changed and removed
photos, *Rebuild everything* reprocesses all of them. The same from the
command line, while the server is stopped or idle:

```sh
docker compose exec pellicule pellicule scan            # changes only
docker compose exec pellicule pellicule scan --rebuild  # everything
```

Verified on 2026-10-15 with Docker Desktop on a Mac (arm64): image builds,
health check passes, 200 Fujifilm JPEGs scanned with 2 workers in 3 minutes
(about 1.8 s per photo per worker, before pipeline v3), ~216 MB RAM during
the scan, all UI checks pass. Not yet run on the N100 itself.

### Scan speed and CPU

A scan processes `[scan] workers` photos in parallel (default: half the CPU
cores, so 2 on the N100) and, on Linux, runs its workers at low priority
(`nice 10`) so the gallery and the NAS stay responsive. The first scan of a
large library takes a while. On an M4 Pro a 26 MP JPEG takes about 0.42 s
per worker (the Library page shows this "per photo" time for every scan); an
N100 core is roughly 3× slower, so expect about 1.3 s per photo per worker,
or around 2 hours for 10,000 photos with 2 workers (to be measured). Later
scans only process new and changed photos. Lower `workers` for a gentler
scan, raise it for a faster one.

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

**The private port has no login.** Anyone who can reach `:8080` sees the
whole gallery and can start or cancel scans; the trust boundary is the home
LAN and the tailnet. Never port-forward it on the router or run it on a
network you don't trust (to restrict it to the tailnet, publish it on the
NAS's tailnet IP in `compose.yaml`, or bind it to loopback and use
`tailscale serve`). Only the public listener is meant for the internet.

Because the public port is published on loopback only, nothing on the LAN can
reach it directly, and the public listener itself only routes `/s/<token>/…`.
Optionally, `tailscale serve` can give the private side HTTPS on the tailnet
(on a different port than Funnel's 443).

## Hardware video (later)

The N100's iGPU supports Intel Quick Sync. When video transcoding lands (M5),
uncomment the `devices: /dev/dri` block in `compose.yaml`; the render group of
the host must be able to use it (`group_add` with the host's `render` GID).

## Local development

The same `compose.yaml` runs on the Mac, with a git-ignored `.env` pointing
at a small library and binding the private port to loopback; see
[development](development.md).

`samples/` and `cache/` are git- and docker-ignored.
