# Cache & derivatives

Everything Pellicule generates lives under the cache directory. The library is
never written to. Deleting the cache is always safe; it only costs a rescan.

```
<cache>/
├── pellicule.db            SQLite (WAL): albums, media, exif, derivatives, shares
├── d/ab/cd/<key>-t.v2.webp grid thumbnail
├── d/ab/cd/<key>-m.v2.webp grid 2× / cover
├── d/ab/cd/<key>-l.v2.webp viewer
├── d/ab/cd/<key>-x.v2.webp zoom
├── d/ab/cd/<key>-p.v2.webp video poster
└── d/ab/cd/<key>-v720.v2.mp4  video preview
```

## Identity: two keys

| Key | Made of | Used for |
|---|---|---|
| **Fast key** | `(relative path, size, mtime_ns)` (+ inode on local filesystems) | "Unchanged → skip" without reading the file |
| **Content key** | BLAKE3 of the whole file | Naming derivatives |

- The content key is computed only for new or changed fast keys. Those files
  are read in full for decoding anyway, so hashing all of it costs little, and
  two different files can never share derivatives. Because derivatives are
  named by content, **moving or renaming a file reuses its cache** and URLs
  never go stale.
- The pipeline version (`v2`) is part of every variant name. Changing sizes,
  quality or processing bumps it, and every file is reprocessed on the next
  scan; old files stay on disk until garbage collection exists (planned).

## Image variants

All variants fit inside a long-edge box, never upscale, and have orientation applied.

| Variant | Long edge | Format | When | Typical size | Use |
|---|---|---|---|---|---|
| `t` | 480 | WebP q75 | Scan (eager) | 15–35 KB | Grid tiles |
| `m` | 960 | WebP q78 | Scan (eager) | 60–120 KB | Grid on 2× screens, album covers |
| `l` | 2048 | WebP q80, 4:4:4 chroma | Scan (eager) | 250–500 KB | Viewer; **largest variant on public shares** |
| `x` | 4096 | WebP q82, 4:4:4 chroma | Scan (eager) | 1–2 MB | Zoom on the private side |
| original | — | as-is | — | — | Download only (private side, or shares with `allow_download`) |

**Why everything is eager:** the library lives on HDDs that spin down, the
cache on the NAS SSD. With every variant generated at scan time, browsing,
viewing and zooming never wake the disks; only a scan or an original download
does. Budget roughly 2–2.5 MB of SSD per photo for all four variants.

Plus, per image, stored in SQLite:
- **ThumbHash** (~25 bytes) and **average colour**: placeholder and tile background.
- **Display dimensions** (after orientation), taken from the decoded image header,
  never from EXIF tags, so the grid can be laid out before anything loads.

AVIF twins (served via `<picture>`) are a possible later addition. libwebp encodes
roughly 10× faster, which matters on NAS CPUs.

## Pipeline (libvips)

- `vips_thumbnail` with **shrink-on-load**: JPEGs are DCT-downscaled while
  decoding, which is the single biggest speed win on 24–40 MP files.
- **Decode once, cascade**: the original is decoded once into `x` (kept in
  memory), then `l` is made from `x`, `m` from `l`, `t` from `m`.
- Metadata is stripped from every derivative (no EXIF, no GPS).
- **Colour management**:
  - Implemented: every derivative is sRGB with no profile. A source with an
    embedded profile (Adobe RGB, Display-P3…) is converted with it; a source
    without one is taken as sRGB.
  - **Planned**: wide-gamut sources (Adobe RGB, Display-P3) → Display-P3 with
    a compact embedded profile (~0.5 KB).
  - **Planned**, Fujifilm caveat: bodies set to Adobe RGB may write **no ICC profile** and
    signal it only with EXIF `ColorSpace = 0xFFFF` + Interop index `R03`. These
    are imported with a bundled Adobe RGB (1998)-compatible profile; without
    this they render desaturated. To verify on a real sample.
- **Startup probe** (**planned**): decode a tiny bundled sample of each format at boot and
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

- **At scan**: metadata, dimensions, ThumbHash, every image variant, video
  poster and preview. Files are written to a temporary name and renamed, so a
  crash never leaves a truncated file behind an immutable URL.
- If all variants for a content key already exist (a renamed or moved file),
  nothing is re-encoded; the placeholder is recomputed from the cached `t`.
- Files are processed in parallel (CPU count − 1 workers, each libvips
  operation single-threaded); results are committed to SQLite in batches.
- Failed files are recorded with their error and retried on the next scan.

## Rescans & safety

- **Directory-mtime short-circuit** (**planned**; today every scan walks the
  whole tree): a directory whose mtime hasn't changed has
  had no files added, removed or renamed; only a periodic full stat sweep
  catches in-place edits.
- Rescans are triggered by Pellicule starting and a manual rescan (button /
  CLI). **Planned**: a filesystem watcher
  (`notify`; it sees changes made on the NAS through the bind mount without
  polling, so it doesn't wake the disks) and one nightly full sweep for anything the watcher missed (e.g. edits over SMB).
  No periodic polling.
- Missing files are marked `missing_since` rather than deleted, and **a scan
  aborts if the library root is empty or unmounted**. A flaky mount must never
  wipe covers, captions or share links.
- **Garbage collection** (**planned**; today the cache only grows): derivatives not referenced by any current media row
  (or from an old pipeline version) are deleted after a grace period.

## HTTP caching

- Derivative URLs are content-addressed (`/d/<key>-<variant>.<ext>`):
  `Cache-Control: public, max-age=31536000, immutable`, `ETag`, early `304`.
- Videos and originals support `Range` (206).
- HTML: `Cache-Control: no-cache` + weak ETag (album update time + build id),
  so repeat views over Funnel are cheap 304s.
- Static assets: hashed names, embedded in the binary, pre-compressed (brotli).
  Images and videos are never re-compressed.
