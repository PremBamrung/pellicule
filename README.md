# Pellicule

A slim, fast, self-hosted photography gallery written in Rust.

Pellicule serves a curated folder of photos and videos as a quiet, image-first
gallery — closer to a photographer's portfolio than to a file browser. It reads
your folders **read-only** and never imposes its own structure: a folder is an
album, an optional `index.md` turns an album into an exhibition, and everything
Pellicule generates lives in a separate cache directory you can delete at any time.

> **Status (October 2026):** browsable prototype. Scanning, previews, the
> gallery (home, albums, viewer, filters) and the Library page work locally;
> the Fujifilm recipe is in; next are running it on the NAS and share links.
> See the [roadmap](docs/roadmap.md).

## Goals

- **Your folders are the source of truth.** No import, no ingestion, no renaming.
  Mount the library read-only; Pellicule only writes to its cache.
- **Fast everywhere.** Pre-generated, right-sized previews (thumbnail → screen →
  original only on zoom), tiny placeholders, and server-computed layouts so pages
  render instantly — even through a bandwidth-limited Tailscale Funnel.
- **Beautiful by default.** Justified grid for browsing, an optional essay /
  exhibition layout for curated albums, an unobtrusive viewer, colour-managed
  previews (wide gamut kept).
- **Made for photographers.** EXIF shown with care, and a Fujifilm "recipe
  card" decoded from the MakerNote: film simulation, grain, colour chrome,
  dynamic range, white balance shift, tone…
- **Private by default, shareable on purpose.** The full gallery is only
  reachable on your tailnet; a second, separate listener serves nothing but the
  albums you explicitly share via token links.
- **Small.** One binary, SQLite cache, no external database, no ML, no accounts
  system beyond what two people and some share links need.

## Non-goals

Uploading, phone backup, face recognition, editing, culling or sorting. Pellicule
is for the *curated* end of a workflow; sorting/culling happens upstream.

## How it works (in one picture)

```
NAS library (read-only)            Pellicule                         Viewers
/photos/2025-hokkaido/    ──scan──▶  SQLite index + cache  ──▶ :8080 tailnet (everything)
  index.md (optional)               previews, posters,     ──▶ :8081 Funnel (share links only)
  DSCF1234.JPG                      transcoded videos
```

## Documentation

- [Architecture](docs/architecture.md) — components, data flow, crates
- [Decisions](docs/decisions.md) — the main choices and why, in one table
- [Library conventions](docs/conventions.md) — folders, `index.md`, ignored files
- [Cache & derivatives](docs/cache.md) — sizes, formats, cache layout, invalidation
- [EXIF & Fujifilm metadata](docs/exif.md) — what we read and how we display it
- [Frontend](docs/frontend.md) — design principles, grid, viewer, essay layout
- [Access & sharing](docs/sharing.md) — tailnet vs Funnel, share links
- [Inspirations](docs/inspirations.md) — what we learned from other galleries
- [Deployment](docs/deployment.md) — Docker Compose on the NAS, Tailscale Funnel
- [Development](docs/development.md) — local setup, prototype library, checks
- [Roadmap](docs/roadmap.md) — milestones

## Quick start

On the NAS (Docker Compose v2, Tailscale on the host); the full steps, with
what to decide first and what to expect, are in
[deployment](docs/deployment.md#first-installation-on-the-nas):

```sh
git clone git@github.com:PremBamrung/pellicule.git && cd pellicule
cp .env.example .env              # PHOTOS_DIR, CACHE_DIR (SSD), PUID/PGID
docker compose up -d --build     # first build ~10–20 min, then scans in the background
```

Private gallery on `http://<nas>:8080` (LAN / tailnet); scans are started and
followed from its **Library** page. The private side has **no login**: the
LAN / tailnet is the trust boundary, so never port-forward it.

Configuration: `docker/pellicule.toml` (container paths; which host ports
reach the listeners is set in `.env`). For other settings, start from
`pellicule.example.toml` and point `PELLICULE_TOML` at your copy:

```toml
[library]
root  = "/photos"               # PHOTOS_DIR, mounted read-only
cache = "/cache"                # CACHE_DIR: previews, videos, SQLite

[server]
private_addr = "0.0.0.0:8080"   # published as PRIVATE_PORT: LAN / tailnet, no login
public_addr  = "0.0.0.0:8081"   # published on host 127.0.0.1 only, for `tailscale funnel`

[scan]
# workers = 2                   # default: half the CPU cores
```

## Development

Built, run and tested in Docker, on the same Debian and libvips as the NAS.
With a local `.env` pointing at a small library (see
[development](docs/development.md)):

```sh
docker compose up -d --build     # http://localhost:8080
docker build --target test .     # fmt, clippy, tests
```

## License

MIT — see [LICENSE](LICENSE). Ideas borrowed from other projects are credited
in [docs/inspirations.md](docs/inspirations.md).
