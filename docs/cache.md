# Cache & derivatives

Everything Pellicule generates lives under the cache directory. The library is
never written to. Deleting the cache is always safe; it only costs a rescan.

```
<cache>/
├── pellicule.db            SQLite (WAL): albums, media, exif, derivatives, shares
├── d/ab/cd/<key>-t.v1.webp grid thumbnail
├── d/ab/cd/<key>-m.v1.webp grid 2× / cover
├── d/ab/cd/<key>-l.v1.webp viewer
├── d/ab/cd/<key>-x.v1.webp zoom (lazy)
├── d/ab/cd/<key>-p.v1.webp video poster
└── d/ab/cd/<key>-v720.v1.mp4  video preview
```

## Identity: two keys

| Key | Made of | Used for |
|---|---|---|
| **Fast key** | `(relative path, size, mtime_ns)` (+ inode on local filesystems) | "Unchanged → skip" without reading the file |
| **Content key** | BLAKE3 of `size ‖ first 64 KiB ‖ last 64 KiB` | Naming derivatives |

- The content key is computed only for new or changed fast keys. It's cheap even
  on a NAS (two small reads), and because derivatives are named by content,
  **moving or renaming a file reuses its cache** and URLs never go stale.
- The pipeline version (`v1`) is part of every variant name. Changing sizes or
  quality bumps it; old files are removed by garbage collection.

## Image variants

All variants fit inside a long-edge box, never upscale, and have orientation applied.

| Variant | Long edge | Format | When | Typical size | Use |
|---|---|---|---|---|---|
| `t` | 480 | WebP q75 | Scan (eager) | 15–35 KB | Grid tiles |
| `m` | 960 | WebP q78 | Scan (eager) | 60–120 KB | Grid on 2× screens, album covers |
| `l` | 2048 | WebP q80, 4:4:4 chroma | First request (lazy) | 250–500 KB | Viewer; **largest variant on public shares** |
| `x` | 4096 | WebP q82 | First request (lazy) | 1–2 MB | Zoom on the private side |
| original | — | as-is | — | — | Private side, or shares with `allow_download` |

Plus, per image, stored in SQLite:
- **ThumbHash** (~25 bytes) and **average colour**: placeholder and tile background.
- **Display dimensions** (after orientation), taken from the decoded image header,
  never from EXIF tags, so the grid can be laid out before anything loads.

AVIF twins (served via `<picture>`) are a possible later addition. libwebp encodes
roughly 10× faster, which matters on NAS CPUs.

## Pipeline (libvips)

- `vips_thumbnail` with **shrink-on-load**: JPEGs are DCT-downscaled while
  decoding, which is the single biggest speed win on 24–40 MP files.
- **Decode once, cascade**: `m` is produced first, `t` is made from `m`.
- Metadata is stripped from every derivative (no EXIF, no GPS).
- **Colour management**:
  - sRGB sources → sRGB, profile stripped (smallest output).
  - Wide-gamut sources (Adobe RGB, Display-P3) → Display-P3 with a compact
    embedded profile (~0.5 KB).
  - Fujifilm caveat: bodies set to Adobe RGB may write **no ICC profile** and
    signal it only with EXIF `ColorSpace = 0xFFFF` + Interop index `R03`. These
    are imported with a bundled Adobe RGB (1998)-compatible profile; without
    this they render desaturated. To verify on a real sample.
- **Startup probe**: decode a tiny bundled sample of each format at boot and
  disable formats the local libvips can't handle.

## Video

Done with the `ffmpeg` CLI at scan time, one global ffmpeg semaphore.

- **Preview**: H.264 High, `-crf 26 -preset medium -maxrate 2.5M -bufsize 5M`,
  720p cap (`scale=-2:'min(720,ih)'`, swapped for portrait), `yuv420p`,
  AAC 128k, `-movflags +faststart`, `-map_metadata -1`. Roughly 1–2 MB per 10 s.
- **Transcode only when required**: sources already H.264 8-bit 4:2:0 within
  the size and bitrate caps are served as-is.
- **HDR** (HLG/PQ) sources are tone-mapped, or they look washed out.
- **Poster**: a representative frame (scene-aware `thumbnail` filter, skipping
  black frames), then through the image pipeline for `p` + ThumbHash.
- Optional 1080p rendition for the private side; on-the-fly HLS deferred
  (go-vod, Apache-2.0, is the reference if we ever need it).

## Generation strategy

- **Eager at scan**: metadata, dimensions, ThumbHash, `t`, `m`, video poster and preview.
- **Lazy on first request**: `l`, `x`. A single-flight map keyed by
  `(content key, variant)` prevents duplicate work; a **negative cache** (TTL
  ~10 min) stops a corrupt file from being retried on every request.
- CPU-bound work runs on a bounded pool (`cpus − 1`), never on the async runtime.

## Rescans & safety

- **Directory-mtime short-circuit**: a directory whose mtime hasn't changed has
  had no files added, removed or renamed; only a periodic full stat sweep
  catches in-place edits.
- Files watched with `notify` when Pellicule runs on the NAS itself; polling
  otherwise (inotify misses changes made by other NFS/SMB clients).
- Missing files are marked `missing_since` rather than deleted, and **a scan
  aborts if the library root is empty or unmounted**. A flaky mount must never
  wipe covers, captions or share links.
- **Garbage collection**: derivatives not referenced by any current media row
  (or from an old pipeline version) are deleted after a grace period.

## HTTP caching

- Derivative URLs are content-addressed (`/d/<key>-<variant>.<ext>`):
  `Cache-Control: public, max-age=31536000, immutable`, `ETag`, early `304`.
- Videos and originals support `Range` (206).
- HTML: `Cache-Control: no-cache` + weak ETag (album update time + build id),
  so repeat views over Funnel are cheap 304s.
- Static assets: hashed names, embedded in the binary, pre-compressed (brotli).
  Images and videos are never re-compressed.
