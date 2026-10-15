# Roadmap

Milestones are small and each ends with something usable.

## M0 — Scaffold ✅

- Cargo project, CLI (`scan`, `serve`), config file, logging.
- Docs (this folder).
- Minimal server that renders a placeholder page.

## Thin slice (M1 + M2 + M3) — browse the library

First visible version, hardened afterwards.

- SQLite index (albums, media, standard EXIF), incremental scan with fast-key
  skip, partial-BLAKE3 content keys, empty-root guard, missing files marked.
- libvips: all four WebP variants at scan time, ThumbHash + average colour.
- Pages: home (album covers), album (sub-albums + justified grid), viewer
  (PhotoSwipe, zoom loads 4096 only on demand, museum-style label).
- Background scan when `serve` starts.

## Hardening (rest of M1–M3)

- `index.md` front matter (title, cover, order, hidden).
- Rescans: filesystem watcher, rescan button + CLI, nightly sweep.
- Directory-mtime short-circuit, garbage collection of stale derivatives.
- Colour edge cases (Fuji Adobe RGB without ICC), startup format probe.

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
