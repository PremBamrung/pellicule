# Pellicule

A slim, fast, self-hosted photography gallery written in Rust.

Pellicule serves a curated folder of photos and videos as a quiet, image-first
gallery — closer to a photographer's portfolio than to a file browser. It reads
your folders **read-only** and never imposes its own structure: a folder is an
album, an optional `index.md` turns an album into an exhibition, and everything
Pellicule generates lives in a separate cache directory you can delete at any time.

> Status: early design / scaffold. Nothing usable yet.

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
- [Library conventions](docs/conventions.md) — folders, `index.md`, ignored files
- [Cache & derivatives](docs/cache.md) — sizes, formats, cache layout, invalidation
- [EXIF & Fujifilm metadata](docs/exif.md) — what we read and how we display it
- [Frontend](docs/frontend.md) — design principles, grid, viewer, essay layout
- [Access & sharing](docs/sharing.md) — tailnet vs Funnel, share links
- [Inspirations](docs/inspirations.md) — what we learned from other galleries
- [Deployment](docs/deployment.md) — Docker Compose on the NAS, Tailscale Funnel
- [Roadmap](docs/roadmap.md) — milestones

## Quick start

On the NAS (Docker Compose, Tailscale on the host):

```sh
cp .env.example .env              # PHOTOS_DIR, CACHE_DIR, PUID/PGID
docker compose up -d --build
docker compose run --rm pellicule scan
sudo tailscale funnel --bg 8081   # public share links
```

Private gallery on `http://<nas>:8080` (LAN / tailnet). See [deployment](docs/deployment.md).

Configuration (`docker/pellicule.toml` in the container, `pellicule.toml` locally):

```toml
[library]
root  = "/photos"               # mounted read-only
cache = "/var/lib/pellicule"    # previews, videos, sqlite

[server]
private_addr = "0.0.0.0:8080"   # reachable on LAN / tailnet
public_addr  = "127.0.0.1:8081" # exposed with `tailscale funnel 8081`
```

## Development

Requires Rust (stable), libvips and ffmpeg.

```sh
brew install rustup vips ffmpeg   # macOS
cp pellicule.example.toml pellicule.toml   # point root at ./samples
cargo run -- scan
cargo run -- serve
```

## License

MIT — see [LICENSE](LICENSE). Ideas borrowed from other projects are credited
in [docs/inspirations.md](docs/inspirations.md).
