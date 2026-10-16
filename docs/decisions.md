# Decisions

The main technical choices behind Pellicule and why they were made. Details
live in the linked docs; this page is the short version.

| Topic | Decision | Why |
|---|---|---|
| Scope | Curated, read-only gallery: folder = album, no import, no upload, no culling | Sorting and culling happen upstream; the gallery only shows finished work. See [conventions](conventions.md). |
| Language | Rust | Memory safety for untrusted file parsing and public share links, small and predictable RAM on a NAS, one binary. See [architecture](architecture.md#why-rust). |
| Image processing | libvips; three WebP variants (480 / 960 / 2048, effort 2) and a JPEG zoom size (4096) | Shrink-on-load is the biggest speed win on 26 MP JPEGs; WebP encodes ~10× faster than AVIF on a NAS CPU; the zoom size as JPEG and effort 2 make scans ~3.5× faster. See [cache](cache.md). |
| When to generate | Everything at scan time, cache on SSD | The library lives on HDDs that spin down: browsing, viewing and zooming must never wake them. |
| Cache identity | BLAKE3 of the whole file | Renamed or moved files reuse their previews; URLs never go stale. A partial hash (first/last 64 KiB) was dropped: two files with the same ends would share previews. |
| Index | SQLite in the cache directory | One file, no server, disposable (a rescan rebuilds it). |
| Scans | One at a time, half the CPU cores by default, low priority; history recorded | The NAS stays responsive; progress and failures are visible on the Library page. |
| EXIF | `nom-exif` + own Fujifilm MakerNote parser | No gallery reads Fuji film simulations; the library is almost all Fujifilm. See [exif](exif.md). |
| Access | Two listeners: private (LAN / tailnet) and public (share links only, behind Tailscale Funnel) | The public side cannot route to anything unshared. See [sharing](sharing.md). |
| Location data | Shown on the private side only, never on share links | Privacy by default. |
| Video | Pre-transcoded 720p H.264 previews at scan time (planned) | Plays everywhere, light on Funnel's limited bandwidth. |
| Frontend | Server-rendered HTML (askama) + vanilla JS + PhotoSwipe 5, pure-CSS justified grid | Fast first paint, tiny payload, no build toolchain. See [frontend](frontend.md). |
| Look | White wall by default (dark on request), Geist only, quiet chrome | An art-gallery feel: the photographs carry the page. |
| Filters | In the browser, state in the URL | Instant, bookmarkable, no server round trip for albums up to a few thousand photos. |
| Deployment | Docker Compose built on the NAS, Tailscale on the host | Simple updates (`git pull && docker compose up -d --build`), no registry. See [deployment](deployment.md). |
| Borrowed code | Ideas only from GPL/AGPL projects; MIT/Apache code may be adapted | Keeps Pellicule MIT. See [inspirations](inspirations.md). |
