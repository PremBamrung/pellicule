# Development

How to work on Pellicule on the Mac, and the conventions used so far.

## Setup

Everything is built, run and tested in Docker, on the same Debian trixie and
libvips as the NAS, so the Mac's tools never decide whether something works
(Homebrew's libvips 8.18 and Debian's 8.16 behave differently, see below).
Docker Desktop must be running (`open -a Docker`).

Local, git-ignored files point the stock `compose.yaml` at the prototype
library:

```sh
# .env
PHOTOS_DIR=./samples-proto
CACHE_DIR=./cache
PELLICULE_TOML=./pellicule.docker.toml
PUID=501            # id -u
PGID=20             # id -g
PRIVATE_PORT=127.0.0.1:8080   # loopback only: the private side has no login
PUBLIC_PORT=8081
```

```toml
# pellicule.docker.toml (container paths)
[library]
root  = "/photos"
cache = "/cache"

[server]
private_addr = "0.0.0.0:8080"
public_addr  = "0.0.0.0:8081"

[scan]
workers = 2   # keep the Mac usable while scanning
```

`PRIVATE_PORT` keeps `127.0.0.1:` on a laptop: `compose.yaml` publishes the
private port on every interface, which is right on the NAS but would open
the gallery to any Wi-Fi the Mac joins.

A Rust toolchain on the Mac (`brew install rustup vips`) is only for the
editor (rust-analyzer); `.cargo/config.toml` adds `/opt/homebrew/lib` to the
linker search path there and has no effect in Docker.

## Prototype library

Real photos stay out of git (`samples*`, `cache/`, `.env` and the local
configs are ignored). For quick iterations use a small subset made of **hard
links** (no extra disk space, originals untouched):

```sh
mkdir -p "samples-proto/Japon"
ls samples/Japon | sort | head -100 | while read -r f; do ln -f "samples/Japon/$f" "samples-proto/Japon/$f"; done
```

Switching `PHOTOS_DIR` between libraries is cheap: files that disappear are
only marked missing, and derivatives are named by content, so switching back
reuses everything already processed. The SQLite index stores paths relative
to the library root, so the same `cache/` works whatever the root is mounted
as.

## Run

```sh
docker compose up -d --build     # http://localhost:8080, scans on start
docker compose logs -f
docker compose down
```

Rebuilding after an edit takes a few seconds: the build keeps cargo's
registry and `target/` in BuildKit cache mounts, so only what changed is
recompiled. Templates and `static/` are embedded in the binary, so every
change needs `up -d --build`.

One-off commands run in the same image:

```sh
docker compose run --rm pellicule scan
docker compose run --rm pellicule scan --rebuild
```

## Checks

```sh
docker build --target test .              # fmt --check, clippy -D warnings, cargo test
node scripts/check-ui.mjs                 # with the container running
```

The `test` stage builds on the same base as the release build (Rust and
Debian's libvips), so the derivative tests run against the NAS's libvips.
`docker compose up` never builds it. It also has exiftool, the oracle for
the Fujifilm golden tests; run it from that image:

```sh
docker build --target test -t pellicule:test .
docker run --rm -v "$PWD":/work -w /work pellicule:test exiftool -j -G1 -n -FujiFilm:all <file>
# regenerate tests/fixtures/fuji/ from the full samples
docker run --rm -v "$PWD":/src -v "$PWD/samples":/photos:ro -w /src \
  pellicule:test sh scripts/make-fuji-fixtures.sh
```

`scripts/check-ui.mjs` drives the Mac's headless Chrome (no npm dependencies)
through home, album cards, filters, density, viewer, slideshow, theme and the
Library page, prints ok/FAIL per check and saves screenshots to
`target/ui-check/`. It is only a client of the server, so it stays on the
Mac. It never starts a scan. Headless Chrome cannot go narrower than 500 px:
use `--window-size=500,…` for "mobile" screenshots.

## libvips versions

Homebrew has libvips 8.18, the Docker image 8.16 (Debian trixie): one more
reason to build and test in Docker only. The `libvips` crate is generated
for 8.17+, and its `*_with_opts` functions send **every** optional argument
by name, including ones renamed after 8.16 (`input-profile` was
`import-profile`), so on 8.16 the whole call fails. Rule: only call binding
functions **without** options (`ops::thumbnail`, `ops::thumbnail_image`,
`ops::icc_transform`), and pass save options in libvips' filename syntax
(`image_write_to_file("l.webp[Q=80,effort=2,keep=none]")`).

Never create a temporary `VipsApp` (e.g. `VipsApp.error_clear()`): dropping
one calls `vips_shutdown` for the whole process. Use the raw
`libvips::bindings` error functions instead, as `derive::take_vips_error` does.

The derivative tests run the real pipeline on `tests/fixtures/` (made by
`scripts/make-fixtures.sh`).

## Conventions

- Commit messages: imperative subject, short bullet body; no co-author lines.
- Docs live in `docs/`; keep [roadmap.md](roadmap.md) (status table) current
  when a milestone lands.
- Vendored frontend files live in `static/vendor/` with their license files;
  fonts in `static/fonts/` with their OFL text.
- Code from GPL/AGPL projects is never copied (see
  [inspirations.md](inspirations.md)); MIT/Apache code may be adapted.

## Where things are

| Path | What |
|---|---|
| `src/main.rs` | CLI, libvips init |
| `src/config.rs` | `pellicule.toml` |
| `src/library.rs` | media detection, ignore rules |
| `src/scan.rs` | scan engine, progress / cancel (`ScanControl`), `scan_run` history |
| `src/derive.rs` | libvips variants, content keys, ThumbHash |
| `src/meta.rs` | EXIF extraction and formatting |
| `src/meta/fuji.rs` | Fujifilm MakerNote recipe |
| `src/db.rs` | SQLite open + migrations |
| `src/pages.rs` | page queries and view models (filters, day sections, hero…) |
| `src/server.rs` | listeners, routes, static assets, Library API |
| `templates/` | askama templates |
| `static/app.css`, `static/app.js` | the whole frontend |
| `scripts/check-ui.mjs` | browser check |
