# Roadmap

Milestones are small and each ends with something usable. Last updated
2026-10-15.

## Where we are

Pellicule runs locally on a 200-photo prototype library (two albums, Fujifilm
X-T3/X-E3 JPEGs) and is ready to be tried on the NAS with Docker Compose.

| Area | Status |
|---|---|
| Scan & index | ✅ incremental, content-keyed, crash/unmount-safe |
| Derivatives | ✅ 3 WebP sizes + JPEG zoom size at scan time, ThumbHash, average colour |
| Browsing | ✅ home photos, album cards (cover flip, colours, dates, gear), justified grid, day sections, viewer |
| Filters | ✅ day, camera, lens, focal range (in the browser, URL state) |
| Library page | ✅ rescan / rebuild / cancel, live progress, history, failures |
| Look | ✅ white by default + dark toggle, Geist, blur-up placeholders, transitions |
| Docker | ✅ image + compose, verified end to end on the Mac; ⏳ not yet run on the NAS |
| Fujifilm recipe | ✅ film simulation on the label, recipe panel, Film filter; ⏳ film on album cards |
| Share links (Funnel) | ⏳ |
| `index.md` / essays | ⏳ |
| Video | ⏳ |

## Done

### M0 — Scaffold
- Cargo project, CLI (`scan`, `serve`, `healthcheck`), TOML config, logging.
- Docs, Docker Compose deployment, two listeners (private / public).

### M1–M3 thin slice — browse the library
- SQLite index (albums, media, standard EXIF), incremental scan with fast-key
  skip, whole-file BLAKE3 content keys, empty-root guard, missing files marked.
- libvips: all four variants at scan time (HDD-friendly), converted to
  sRGB, ThumbHash + average colour. Fixture tests (orientation, wide gamut,
  alpha, no upscale) pass on libvips 8.16 and 8.18.
- Pages: home, album (sub-albums + justified grid), PhotoSwipe viewer (4096
  only on zoom, museum-style label), background scan when `serve` starts.

### Frontend pass 1
- White wall by default, dark toggle remembered per browser.
- Album filters: day, camera, lens, 35mm-equivalent focal range; faceted
  counts, no dead ends, URL state.

### Library page & scan tools
- `/library`: rescan, rebuild everything, cancel, live progress bar (rate,
  ETA), history of every scan, files that failed.
- One scan at a time; `scan_run` history; `[scan] workers` (default half the
  cores) and low-priority workers on Linux; `scan --rebuild`.

### Frontend pass 3
- Home photos: six random photos crossfading, each linking to its place in
  the album (`/a/<album>#p=<key>`); replaced by rows of photos in pass 4.

### M4 — Fujifilm recipe
- MakerNote parser (`src/meta/fuji.rs`): film simulation, grain, colour
  chrome, dynamic range, white balance and its shift, tone, colour,
  sharpness, noise reduction, clarity; Fujifilm photos only.
- Golden tests: MakerNote-only fixtures (serial zeroed) for every recipe in
  the library, checked against exiftool; robustness against truncated and
  corrupted notes.
- `meta_version`: a reader change re-reads metadata on the next scan
  without touching derivatives (506 photos in 0.34 s).
- Viewer: film simulation on the label; **Recipe** button / `r` (panel on
  desktop, bottom sheet on phones). Albums: a **Film** filter menu.

### Docker workflow
- Build, run and test in Docker on the NAS's Debian and libvips
  (`docker build --target test .`); local `.env` for the Mac.

### Frontend pass 6
- Album cards: cover and colour band only; the strip of four photos is gone,
  the cover still flips through them under the pointer.
- Album grid aligned with the page margins instead of running to the window
  edge.

### Faster scans
- Pipeline v3: the 4096 px zoom size as JPEG q85, WebP at effort 2. A full
  scan of the 506-photo prototype library with 2 workers: 1m 49s instead of
  6m 27s (0.42 s per photo per worker on an M4 Pro).
- Per-photo time recorded for every scan and shown in the Library history.

### Frontend pass 5
- Home: a hairline and an "Albums" heading (stats on the same line) between
  the photos and the albums; a row of photos narrower than the page is
  centred.
- Album cards: a strip of four photos spread through the album, a band of
  the album's colours, date range and shooting days, sub-album count,
  camera models and focal range; the pointer flips the cover through the
  strip.

### Frontend pass 4
- Home: rows of whole, uncropped photos (portraits too) filling the width,
  crossfading as a row; larger wordmark (28–44 px).
- Album bar: the toolbar stays at the top once the title has gone, with the
  wordmark, album and current day; day headings no longer stick.
- Viewer: photo permalinks (`#p=<key>`, reload and shared links open it),
  Back closes it, focus returns to the photo, label hidden while zoomed,
  `?` lists the keys; named photo links for screen readers.
- Library: consecutive scans with no changes fold into one row; two-line
  rows on phones.
- Brotli / gzip for pages, CSS and JS; album pages prefetched on hover.

### Frontend pass 2
- Geist throughout, large tight titles.
- Home opens on a random photograph (framed, about half the screen), then
  library stats and album covers; album titles morph from the home page
  (View Transitions).
- Blurred ThumbHash placeholders fading into the photo.
- Day sections with a sticky, frosted day label (moved into the album bar
  in pass 4).
- Grid density S / M / L (remembered); viewer slideshow (play button, space).

## Next

### Deploy on the NAS
- `docker compose up` on the N100; measure scan speed and memory; check
  permissions and disk spin-down behaviour with the real library.
- Filesystem watcher + nightly sweep (scans are manual or on start today).

### M4 — Fujifilm recipe (rest)
- Film simulations on album cards ("Classic Chrome · Acros"), if the cards
  stay calm with it.
- D-Range Priority and the in-camera rating, once a body that records them
  can be tested.

### M6 — Sharing (public listener)
- Share tokens (password, expiry), managed from the Library page.
- Public listener serving only shared subtrees; GPS never exposed.
- Hardening checklist from [sharing.md](sharing.md).

### Albums as exhibitions
- `index.md` front matter: title, cover, order, hidden, intro text.
- `layout: essay` rendering from the `index.md` body.

### M5 — Video
- ffmpeg: scene-aware poster, 720p H.264/AAC preview with faststart,
  transcoding only when required, HDR tone-mapping; range requests.

### Hardening
- Directory-mtime short-circuit, garbage collection of stale derivatives
  (dry run and grace period first; the cache only grows today).
- Cancel a scan during the directory walk, not only between files.
- Integration tests on a fixture library: scan → SQLite → page → derivative,
  rename / change / remove + rescan, empty root, cancel, route checks.
- CI: `cargo fmt --check`, `clippy -D warnings`, tests, Docker build +
  `/healthz` smoke test.
- Docs: label what's planned vs implemented in architecture / sharing / exif.
- Scale, only if NAS measurements call for it: batch the walk transaction,
  DB access off a single mutex, `ORDER BY random()` for the home photo.
- Colour: keep wide gamut as Display-P3, Fuji Adobe RGB without ICC, startup
  format probe.

## Later / maybe

- Film simulation pages (`/film/classic-chrome`) and per-album film summary.
- Previous / next album links, original download (and zooming into it).
- Contact-sheet album layout (no film decoration such as sprocket holes).
- Map view (private only), search by EXIF.
- AVIF twins; on-the-fly HLS (go-vod approach) if long videos need it.
- Integration hook for the culling tool ("publish album" → rescan path).
- Optional simple login on the private side.
