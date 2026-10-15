# Development

How to work on Pellicule locally (macOS), and the conventions used so far.

## Setup

```sh
brew install rustup vips ffmpeg
rustup default stable
fish_add_path /opt/homebrew/opt/rustup/bin ~/.cargo/bin   # fish; rustup is keg-only
```

`.cargo/config.toml` adds `/opt/homebrew/lib` to the linker search path on
macOS so the `libvips` crate finds Homebrew's libvips. It has no effect on
Linux / Docker.

## Prototype library

Real photos stay out of git (`samples*`, `cache/` and `pellicule.toml` are
ignored). For quick iterations use a small subset made of **hard links**
(no extra disk space, originals untouched):

```sh
mkdir -p "samples-proto/Japon"
ls samples/Japon | sort | head -100 | while read -r f; do ln -f "samples/Japon/$f" "samples-proto/Japon/$f"; done
```

Local `pellicule.toml` (ignored):

```toml
[library]
root  = "/Users/prem/misc/pellicule/samples-proto"
cache = "/Users/prem/misc/pellicule/cache"

[server]
private_addr = "127.0.0.1:8080"
public_addr  = "127.0.0.1:8081"

[scan]
workers = 2   # keep the Mac usable while scanning
```

Switching `root` between libraries is cheap: files that disappear are only
marked missing, and derivatives are named by content, so switching back
reuses everything already processed.

## Run

```sh
cargo build --release
./target/release/pellicule serve        # http://localhost:8080, scans on start
./target/release/pellicule scan         # CLI scan (stop `serve` first)
./target/release/pellicule scan --rebuild
```

Release builds embed `static/` and `templates/`; rebuild after editing them.
Debug builds (`cargo run -- serve`) read `static/` from disk.

## Checks

```sh
cargo fmt && cargo clippy --all-targets && cargo test
node scripts/check-ui.mjs                # with `serve` running
```

`scripts/check-ui.mjs` drives headless Chrome (no npm dependencies) through
home, album, filters, density, viewer, slideshow, theme and the Library page,
prints ok/FAIL per check and saves screenshots to `target/ui-check/`. It never
starts a scan. Headless Chrome cannot go narrower than 500 px: use
`--window-size=500,…` for "mobile" screenshots.

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
| `src/db.rs` | SQLite open + migrations |
| `src/pages.rs` | page queries and view models (filters, day sections, hero…) |
| `src/server.rs` | listeners, routes, static assets, Library API |
| `templates/` | askama templates |
| `static/app.css`, `static/app.js` | the whole frontend |
| `scripts/check-ui.mjs` | browser check |
