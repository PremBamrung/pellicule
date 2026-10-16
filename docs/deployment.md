# Deployment

On the NAS Pellicule runs with **Docker Compose**, built on the NAS itself.
Tailscale runs on the host. The Mac uses the same `compose.yaml` for
development (see [development](development.md)).

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

## First installation on the NAS

### Requirements

- Docker with **Compose v2** (`docker compose version`); its builder
  (BuildKit) is needed for the cache mounts in the `Dockerfile`.
- `git`, and access to the repository (SSH key or token if it is private).
- Free space on the SSD for the cache: about **2 MB per photo** (previews
  in three sizes plus the 4096 px zoom size), so ~20 GB for 10,000 photos.
- Tailscale on the host, for access away from home (optional at first).

### Decide three things

1. **The library folder** (`PHOTOS_DIR`): the curated photos only. A folder
   is an album; everything under it is visible to anyone who can reach the
   private port. It is mounted read-only.
2. **The cache folder** (`CACHE_DIR`): on the SSD, so browsing never wakes
   the hard drives. It holds previews and the SQLite index and can be
   deleted at any time (the next scan rebuilds it, which takes hours).
3. **Who can reach the private port** (`PRIVATE_PORT`, no login): `8080`
   for the home LAN and the tailnet, `<NAS tailnet IP>:8080` for the
   tailnet only.

### Install

```sh
docker compose version                      # Compose v2?
git clone git@github.com:PremBamrung/pellicule.git
cd pellicule

id -u; id -g                                # this user must be able to read the photos
mkdir -p /path/on/ssd/pellicule-cache       # owned by that user

cp .env.example .env
```

Set in `.env`:

```sh
PHOTOS_DIR=/path/to/curated/photos
CACHE_DIR=/path/on/ssd/pellicule-cache
PUID=1000            # id -u
PGID=1000            # id -g
PRIVATE_PORT=8080    # or <NAS tailnet IP>:8080
TZ=Europe/Paris
```

The bundled `docker/pellicule.toml` needs no change. To change settings
such as `[scan] workers`, copy `pellicule.example.toml` next to
`compose.yaml` under another name and set `PELLICULE_TOML` to it in `.env`.

```sh
docker compose up -d --build
docker compose logs -f
```

### What to expect

- **First build**: Rust compiles all the dependencies once, likely 10–20
  minutes on the N100. Later builds only recompile what changed.
- **First scan** starts by itself with the server. Follow it on the
  **Library** page (`http://<nas>:8080/library`): progress, rate, ETA, and
  for every scan the time per photo. Estimate below; the gallery is usable
  while it runs, showing photos as they are processed. The hard drives are
  read for the whole scan.
- **New photos are not picked up automatically yet** (no file watcher):
  press *Rescan* on the Library page, or restart the container.
- **Leave Tailscale Funnel off** until share links exist (M6): the public
  port has nothing to serve yet.

### Check afterwards

- `docker compose ps`: the container is `healthy`.
- Library page: no failed files; note the time per photo.
- `docker stats pellicule`: memory during and after the scan.
- After the scan, browse a while: the hard drives should be able to spin
  down (previews come from the SSD).
- `docker compose logs`: no permission errors on `/photos` or `/cache`.

### Scans

`serve` scans the library in the background when it starts. Scans can then be
started, followed and cancelled from the **Library** page: *Rescan*
processes new, changed and removed photos, *Rebuild everything* reprocesses
all of them. From the command line, while the server is idle:

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

## Update

```sh
git pull && docker compose up -d --build
```

A new version may re-read metadata (seconds) or regenerate previews (like
the first scan) on its next start; the commit or the roadmap says so.

## Ports and Tailscale

| Host port | Container | Reach |
|---|---|---|
| `8080` | private listener | LAN and tailnet: `http://<nas>:8080` |
| `127.0.0.1:8081` | public listener | Only via Funnel |

Once share links exist (M6), expose them publicly from the host:

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
