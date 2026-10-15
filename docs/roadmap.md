# Roadmap

Milestones are small and each ends with something usable.

## M0 — Scaffold ✅ (in progress)

- Cargo project, CLI (`scan`, `serve`), config file, logging.
- Docs (this folder).
- Minimal server that renders a placeholder page.

## M1 — Scan & index

- Walk the library, apply ignore rules, detect media types.
- SQLite schema: albums, media, exif, derivatives, shares.
- Incremental rescan: directory-mtime short-circuit, fast key, partial-BLAKE3
  content key; empty-root guard; missing files marked, not deleted.
- Parse `index.md` front matter.

## M2 — Derivatives

- libvips pipeline: `t`/`m` eager at scan, `l`/`x` lazy with single-flight and
  negative cache; orientation applied, metadata stripped, colour-managed
  (incl. Fuji Adobe RGB without ICC).
- ThumbHash + average colour + display dimensions per image.
- Bounded parallelism; resumable; garbage collection of stale cache entries.
- Startup format probe.

## M3 — Browse (private listener)

- Home, album grid (server-side justified layout), sub-albums.
- PhotoSwipe viewer with progressive loading.
- Immutable, content-addressed derivative URLs; ETags for HTML.

## M4 — EXIF

- Standard EXIF fields via `nom-exif`.
- Fujifilm MakerNote parser (film simulation, grain, color chrome, DR…).
- Golden tests against exiftool JSON from sample files of each body.
- Caption line + recipe card.

## M5 — Video

- ffmpeg: scene-aware poster, duration, 720p H.264/AAC preview with faststart,
  transcoding only when required, HDR tone-mapping.
- Range requests; video in grid and viewer.

## M6 — Sharing (public listener)

- Share tokens (password, expiry), CLI + private UI to manage them.
- Public listener serving only shared subtrees; GPS never exposed.
- Hardening checklist from [sharing.md](sharing.md).

## M7 — Essay layout & polish

- `layout: essay` rendering from `index.md` body.
- View Transitions, themes, typography pass, mobile pass.

## M8 — Packaging

- Dockerfile (Debian slim + libvips + ffmpeg), compose example for the NAS.
- Filesystem watcher (optional) or scheduled rescans.

## Later / maybe

- Film simulation pages (`/film/classic-chrome`) and per-album film summary.
- Filmstrip / contact-sheet album layouts.
- Map view (private only), search by EXIF (camera, lens).
- AVIF twins; on-the-fly HLS (go-vod approach) if long videos need it.
- Integration hook for the culling tool ("publish album" → rescan path).
- Optional simple login on the private side.
