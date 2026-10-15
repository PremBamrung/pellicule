# Inspirations

Pellicule borrows ideas from several open-source galleries. This page records
what we took from where, and what we deliberately avoided. Projects were read,
not run, in October 2026.

## Licenses

| Project | License | What we may do |
|---|---|---|
| [PiGallery2](https://github.com/bpatrik/pigallery2) | MIT | Adapt ideas and code (keep notice) |
| [sigal](https://github.com/saimn/sigal) | MIT | Adapt ideas and code |
| [thumbsup](https://github.com/thumbsup/thumbsup) | MIT | Adapt ideas and code |
| [HomeGallery](https://github.com/xemle/home-gallery) | MIT | Adapt ideas and code |
| [go-vod](https://github.com/pulsejet/go-vod) | Apache-2.0 | Port with attribution (if we ever do HLS) |
| [spis](https://github.com/gbbirkisson/spis) | GPL-3.0 | Ideas only |
| [Cimaise](https://github.com/fabiodalez-dev/Cimaise) | GPL-3.0 | Ideas only |
| [Photoview](https://github.com/photoview/photoview) | AGPL-3.0 | Ideas only |
| [Immich](https://github.com/immich-app/immich), [Memories](https://github.com/pulsejet/memories) | AGPL-3.0 | Ideas only |
| exiftool `FujiFilm.pm` | GPL / Artistic | Facts (tag IDs, value meanings) only |

Re-implementing a technique or reusing a numeric default is fine; copying
functions, files or templates from GPL/AGPL projects is not.

## What we took

| Idea | From | Where it lands |
|---|---|---|
| Directory-first, folder = album | PiGallery2, Photoview, sigal | [conventions](conventions.md) |
| `index.md` album sidecar with front matter | sigal | [conventions](conventions.md) |
| `.nomedia` / ignore marker files | sigal, HomeGallery | [conventions](conventions.md) |
| Content-keyed derivative cache `ab/cd/<hash>-<variant>` | HomeGallery, spis | [cache](cache.md) |
| Fast-key skip `(path, size, mtime)` before hashing | HomeGallery | [cache](cache.md) |
| Directory-mtime short-circuit; abort scan on empty root | PiGallery2 | [cache](cache.md) |
| Mark missing instead of delete | spis | [cache](cache.md) |
| libvips thumbnail with shrink-on-load | PiGallery2, Immich, HomeGallery (via sharp) | [cache](cache.md) |
| Decode once, cascade sizes | HomeGallery, Immich | [cache](cache.md) |
| Keep wide gamut + 4:4:4 chroma for previews | Immich | [cache](cache.md) |
| Startup format probe | PiGallery2 | [cache](cache.md) |
| Lazy single-variant generation with lock + negative cache | Cimaise | [cache](cache.md) |
| Store display dims per media and per derivative | PiGallery2, Photoview | [cache](cache.md) |
| ThumbHash + average colour | Immich, HomeGallery | [frontend](frontend.md) |
| Pure-CSS justified rows (`flex-grow: aspect`) | Photoview, Cimaise | [frontend](frontend.md) |
| PhotoSwipe 5 with full-res only on zoom | Memories, sigal, PiGallery2's behaviour | [frontend](frontend.md) |
| Filmstrip / contact-sheet styling, cyclic span grids | Cimaise | [frontend](frontend.md) |
| Transcode only when required; 720p faststart; HDR tone-map | Immich, HomeGallery | [cache](cache.md) |
| Scene-aware / black-frame-skipping video posters | Immich, sigal | [cache](cache.md) |
| Scope-check every media request on a share | PiGallery2 | [sharing](sharing.md) |
| Password-hash fingerprint in the share cookie | Cimaise | [sharing](sharing.md) |
| Global ffmpeg semaphore | thumbsup (issue #283) | [cache](cache.md) |

## What we avoid

| Pitfall | Seen in |
|---|---|
| Short share tokens (8 chars); bcrypt checked on every media request; plaintext share passwords | Photoview, Immich |
| Trusting `127.0.0.1` as authenticated (Funnel traffic arrives from localhost) | HomeGallery |
| Exposing GPS on public shares | PiGallery2, Photoview |
| Thumbnail URLs without a version, so edits stay stale for a year | PiGallery2 |
| Dimensions from EXIF tags (corrupt EXIF → squashed tiles) | PiGallery2 #516 |
| Hand-rolled EXIF orientation table (mirrored cases wrong); hard-coded timezone | spis |
| Dropping ICC profiles (colour shifts) | sigal, HomeGallery, Cimaise |
| Eager loading of hundreds of thumbnails (crashes iOS Safari) | sigal #497 |
| Loading the 3840 px variant in the lightbox | Cimaise |
| Whole index in RAM | thumbsup #74, HomeGallery #134 |
| Shutter speed formatting broken at ≥ 1 s | Photoview |
