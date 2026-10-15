# Architecture

Pellicule is a single Rust binary with two jobs: **scan** a read-only library into
a cache, and **serve** that cache over two HTTP listeners.

```
            ┌────────────────────────── pellicule ──────────────────────────┐
            │                                                               │
 library ──▶│ scanner ─▶ metadata (EXIF, Fuji MakerNote, index.md) ─┐       │
 (ro)       │    │                                                  ▼       │
            │    └──────▶ derivative workers (libvips, ffmpeg) ─▶ cache/    │
            │                                                 + SQLite      │
            │                                                      │        │
            │            private router (:8080) ◀──────────────────┤        │
            │            public router  (:8081) ◀── shares only ───┘        │
            └───────────────────────────────────────────────────────────────┘
```

## Components

| Module | Responsibility |
|---|---|
| `config` | Load `pellicule.toml` (library root, cache dir, listener addresses). |
| `library` | Conventions: media detection, ignore rules, album discovery. |
| `scan` *(M1)* | Walk the tree, diff against SQLite by (path, size, mtime), enqueue work. |
| `meta` *(M1/M4)* | EXIF via a Rust crate + own Fujifilm MakerNote parser; `index.md` front matter. |
| `derive` *(M2/M5)* | libvips previews + ThumbHash; ffmpeg posters and video previews. |
| `db` *(M1)* | SQLite (single file in cache dir): albums, media, exif, derivatives, shares. |
| `layout` *(M3)* | Server-side justified-row computation from aspect ratios. |
| `server` | axum routers. Private: everything. Public: `/s/{token}/…` only. |
| `templates/` | askama templates (compiled into the binary). |
| `static/` *(M3)* | CSS/JS/fonts, embedded and served with content-hashed URLs. |

## Data flow

1. **Scan** walks the library (respecting [conventions](conventions.md)),
   skipping directories whose mtime is unchanged, and compares each file's fast
   key `(relative_path, size, mtime)` with the DB. It aborts if the root is
   empty or unmounted.
2. New/changed files get a content key (partial BLAKE3), metadata (EXIF,
   Fujifilm recipe, display dimensions, capture date) and derivative jobs.
   Disappeared files are marked missing; unreferenced cache entries are
   garbage-collected later.
3. **Derivative workers** (bounded pool, CPU-sized; one global ffmpeg
   semaphore) produce the eager variants into the cache and record them in
   SQLite. See [cache.md](cache.md).
4. **Serve** reads SQLite + cache. Large variants (`l`, `x`) are generated on
   first request, deduplicated, with a negative cache for broken files.

## Key decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language | Rust | Single binary, memory safety for an internet-facing service, fun. |
| HTTP | axum + tokio | Mainstream, small, good ergonomics. |
| Templates | askama | Compile-time templates, no JS framework. |
| Images | libvips (bindings) | Fastest/lowest-memory resizer; ICC, HEIC, orientation handled. |
| Video | ffmpeg CLI at scan time | Pre-transcoded previews: instant playback, light on Funnel. |
| Storage | SQLite | One file, zero ops, plenty fast for a personal library. |
| Source of truth | Filesystem | Cache is disposable; deleting it only costs a rescan. |
| Exposure | Two listeners | Public side cannot route to unshared content. |

## Crates (planned)

| Area | Crate |
|---|---|
| HTTP | `axum`, `tower-http` (`fs`, compression, headers), `axum-extra` (cookies) |
| Templates / assets | `askama`, `rust-embed` |
| Database | `rusqlite` (bundled SQLite) + `deadpool-sqlite` |
| Walk / watch / hash | `walkdir`, `notify` + `notify-debouncer-full`, `blake3` |
| EXIF | `nom-exif` + own Fujifilm MakerNote parser |
| Images | `libvips` bindings; `thumbhash` |
| Video | `ffmpeg` / `ffprobe` CLI via `tokio::process` |
| Sharing | `argon2`, `rand`, `tower_governor`; `moka` for small caches |

Frontend (vendored, MIT): PhotoSwipe 5, ThumbHash decoder.

## Relationship to the culling tool

Pellicule and the sorting/culling tool are separate projects sharing conventions:
folder = album, `index.md` sidecar format, ignore rules. A "publish" step in the
culling tool can simply write a folder (and optionally `index.md`) into the
library and ask Pellicule to rescan that path (CLI or a small private endpoint).
