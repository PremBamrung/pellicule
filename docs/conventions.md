# Library conventions

Pellicule reads a library root (e.g. `/photos`) **read-only**. It never writes,
renames or moves anything inside it. These conventions are all optional — a plain
tree of folders with JPEGs is a valid library.

## Folders are albums

```
/photos
├── 2025-02 Hokkaido/          → album "2025-02 Hokkaido"
│   ├── index.md               → optional: title, cover, text, essay layout
│   ├── DSCF1234.JPG
│   ├── DSCF1240.JPG
│   └── day-2/                 → nested album
│       └── DSCF1301.JPG
├── Portraits/
└── _drafts/                   → ignored (leading underscore)
```

- Every directory containing media (or sub-albums) is an album.
- Nested directories are nested albums; the URL mirrors the path
  (`/a/2025-02%20Hokkaido/day-2`).
- Default album title = folder name. Default order = by capture date
  (EXIF `DateTimeOriginal`), falling back to file name.
- Default cover = first image in order, unless `index.md` says otherwise.

## Ignored entries

- Names starting with `.` or `_` (files and directories).
- Common junk: `@eaDir`, `#recycle`, `Thumbs.db`, `.DS_Store`, `*.xmp` (read later
  maybe, never served), `*.pp3`, `*.tmp`.
- An empty `.pellicule-ignore` (or sigal-style `.nomedia`) file in a directory
  hides it and its children.

## Supported media

| Kind | Extensions | Notes |
|---|---|---|
| Images | `jpg jpeg png webp avif heic heif tif tiff` | JPEG is the primary target (Fujifilm/Canon) |
| Videos | `mp4 mov m4v mkv webm` | Pre-transcoded to web previews at scan time |
| RAW | `raf cr2 cr3 …` | Ignored for now (curated library is JPEG) |

## `index.md` — album sidecar

A Markdown file with YAML front matter. Everything is optional.

```markdown
---
title: Hokkaido, winter
subtitle: Two weeks of snow and silence
date: 2025-02            # shown instead of the computed date range
cover: DSCF1240.JPG
layout: essay            # grid (default) | essay
sort: date               # date (default) | -date | name (natural order)
order:                   # optional: these first, in this order; the rest follow `sort`
  - DSCF1240.JPG
  - DSCF1234.JPG
hidden: false            # hide from listings (still reachable by URL / share link)
captions:
  DSCF1234.JPG: Otaru canal at dusk
---

We landed in Sapporo in the middle of a snowstorm…
```

### Essay layout

With `layout: essay`, the Markdown body defines the sequence. Images are referenced
with standard Markdown image syntax, relative to the album folder, so the file
still reads well on its own (and renders on GitHub):

```markdown
Intro paragraph, rendered as text.

![](DSCF1234.JPG)                       ← one image on its own line: wide

![](DSCF1240.JPG) ![](DSCF1241.JPG)     ← several on one line: a row, sized to fit

More text between images.

![Caption here](DSCF1301.JPG)           ← alt text becomes the caption
```

Media in the folder that the essay doesn't reference are shown afterwards in a
regular grid under "More from this album" (configurable later).

Default cover: `cover` → in-camera rating (highest) → first landscape image →
first item → a sub-album's cover.

## Stability rules

- Pellicule identifies a file by its **path relative to the root**, but its
  previews are named by **content** (see [cache.md](cache.md)): renaming or
  moving a file makes a new item that reuses the existing previews instantly.
- Share links point to **album paths**, so renaming a shared folder breaks its
  link — by design, it's easy to re-share.
- If the library root is empty or unmounted, a scan aborts instead of treating
  everything as deleted; files that disappear are marked missing, not dropped.
