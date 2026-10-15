# Roadmap

Milestones are small and each ends with something usable. Last updated
2026-10-15.

## Where we are

Pellicule runs locally on a 200-photo prototype library (two albums, Fujifilm
X-T3/X-E3 JPEGs) and is ready to be tried on the NAS with Docker Compose.

| Area | Status |
|---|---|
| Scan & index | ✅ incremental, content-keyed, crash/unmount-safe |
| Derivatives | ✅ 4 WebP sizes at scan time, ThumbHash, average colour |
| Browsing | ✅ home hero, album covers, justified grid, day sections, viewer |
| Filters | ✅ day, camera, lens, focal range (in the browser, URL state) |
| Library page | ✅ rescan / rebuild / cancel, live progress, history, failures |
| Look | ✅ white by default + dark toggle, Geist, blur-up placeholders, transitions |
| Docker | ✅ image + compose; ⏳ not yet run on the NAS |
| Fujifilm recipe | ⏳ next |
| Share links (Funnel) | ⏳ |
| `index.md` / essays | ⏳ |
| Video | ⏳ |

## Done

### M0 — Scaffold
- Cargo project, CLI (`scan`, `serve`, `healthcheck`), TOML config, logging.
- Docs, Docker Compose deployment, two listeners (private / public).

### M1–M3 thin slice — browse the library
- SQLite index (albums, media, standard EXIF), incremental scan with fast-key
  skip, partial-BLAKE3 content keys, empty-root guard, missing files marked.
- libvips: all four WebP variants at scan time (HDD-friendly), ThumbHash +
  average colour.
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

### Frontend pass 2
- Geist throughout, large tight titles.
- Home opens on a random full-width photograph, then library stats and album
  covers; album pages open with a full-width cover banner that the home cover
  morphs into (View Transitions).
- Blurred ThumbHash placeholders fading into the photo.
- Day sections with a sticky, frosted day label.
- Grid density S / M / L (remembered); viewer slideshow (play button, space).

## Next

### Deploy on the NAS
- `docker compose up` on the N100; measure scan speed and memory; check
  permissions and disk spin-down behaviour with the real library.
- Filesystem watcher + nightly sweep (scans are manual or on start today).

### M4 — Fujifilm recipe
- Fujifilm MakerNote parser (film simulation, grain, colour chrome, DR, WB
  shift, tone, clarity…) with golden tests against exiftool output.
- Recipe card in the viewer (`i`), film simulation filter.

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
- Directory-mtime short-circuit, garbage collection of stale derivatives.
- Colour edge cases (Fuji Adobe RGB without ICC), startup format probe.
- Gzip/brotli for HTML (album pages are ~4 KB per photo before compression).

## Later / maybe

- Film simulation pages (`/film/classic-chrome`) and per-album film summary.
- Photo permalinks in the URL, previous / next album links, original download.
- Filmstrip / contact-sheet album layouts.
- Map view (private only), search by EXIF.
- AVIF twins; on-the-fly HLS (go-vod approach) if long videos need it.
- Integration hook for the culling tool ("publish album" → rescan path).
- Optional simple login on the private side.
