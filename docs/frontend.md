# Frontend

Pellicule should feel like walking into a gallery, not opening a file manager.
The frontend is deliberately small: **server-rendered HTML + a little vanilla JS**,
no SPA framework, no Node toolchain.

## Stack

| Piece | Choice | Why |
|---|---|---|
| Templates | [askama](https://github.com/askama-rs/askama) (compiled Jinja-like templates) | Type-checked at build time, zero runtime cost |
| Grid layout | **Pure-CSS justified rows**: the server emits each tile's aspect ratio, CSS `flex-grow` does the rest | No layout shift, no layout JS, responsive for free |
| Page transitions | CSS View Transitions API (cross-document) | App-like morph from thumbnail to viewer; graceful fallback |
| Viewer / lightbox | [PhotoSwipe v5](https://photoswipe.com) (MIT) | Zoom, swipe, keyboard, loads full-res only on zoom |
| Placeholders | Average colour as tile background + [ThumbHash](https://evanw.github.io/thumbhash/) (MIT) above the fold | Correct colours before the image arrives, zero JS for first paint |
| Assets | Embedded in the binary (`rust-embed`), content-hashed URLs, pre-compressed | One file to deploy, immutable caching |

## Justified grid

```html
<a class="tile" style="--ar:1.5; background:#7a6a58" href="…">
  <img src="/d/<key>-t.v1.webp"
       srcset="/d/<key>-t.v1.webp 480w, /d/<key>-m.v1.webp 960w"
       sizes="(max-width: 600px) 50vw, 33vw"
       width="480" height="320" loading="lazy" decoding="async" alt="">
</a>
```

```css
.grid  { display: flex; flex-wrap: wrap; gap: 4px; }
.tile  { flex: calc(var(--ar) * 1000) 1 calc(var(--ar) * var(--row-h)); }
.grid::after { content: ""; flex-grow: 1e6; }   /* keeps the last row from stretching */
```

No JS virtualization below ~2k items: native `loading="lazy"` plus
`content-visibility: auto` on row blocks. The first image gets
`fetchpriority="high"`.

## Filters

Album pages offer up to four menus under the title: **Day**, **Camera**,
**Lens** and **Focal length**. A menu only appears when the album has at least
two values for it.

- Days read like "Sat 7 Oct" (with the year when the album spans several).
- Focal length uses 35mm-equivalent ranges, since zooms record values like
  30.8mm: wide (under 35), normal (35–70), short tele (70–135), tele (135+).
- Several values can be picked per menu (OR); menus combine (AND).
- Filtering happens in the browser and is instant: tiles are hidden, the grid
  re-flows, and the viewer only steps through matching photos.
- Counts reflect the other active filters, and options that would empty the
  grid are disabled, so a choice never leads to a dead end.
- State lives in the URL (`?lens=XF35mmF1.4+R&day=2023-10-07`): bookmarkable,
  shareable, restored on reload. "Clear filters" resets it.
- The server only adds `data-f-*` attributes to each tile; no extra queries.

## Viewer

PhotoSwipe opens instantly from the already-loaded grid thumbnail (`msrc`), then
loads `l` (2048). Its `srcset` includes `x`, so the 4096 px variant (or the
original on the private side) is fetched **only when zooming**. Neighbouring
slides are preloaded (±1). Back button closes the viewer.

## Transitions

Cross-document View Transitions (`@view-transition { navigation: auto; }`):
the album cover morphs into the album header, a tile into the photo. Names are
assigned only to the clicked element (in `pageswap` / `pagereveal`), never to
hundreds of tiles. Browsers without support simply navigate; reduced-motion is
respected. Inside the viewer, PhotoSwipe's own zoom animation is used.

Target budget: **< 30 KB** of first-party JS+CSS (gzipped), PhotoSwipe loaded
lazily when the viewer opens.

## Design principles

1. **The image is the interface.** A white gallery wall by default (a neutral
   dark wall is one click away and remembered per browser; the OS setting is
   ignored), generous margins, no borders, no drop shadows, minimal chrome.
   The viewer follows the theme.
2. **Typography carries the identity.** One serif for titles and essay text
   (e.g. a classic book face), one quiet sans for UI and EXIF; small caps /
   letter-spacing for metadata.
3. **Nothing jumps.** Every image has width/height and a ThumbHash before it
   loads.
4. **Right pixels, right time.** `srcset`/`sizes` pick the smallest adequate
   derivative; originals only on explicit zoom or download.
5. **Color is accurate.** Derivatives are colour-managed: sRGB stays sRGB,
   wide-gamut sources keep a Display-P3 profile (see [cache.md](cache.md)).
6. **Keyboard and touch are first-class.** ←/→, Esc, `i` for info, swipe, pinch.

## Pages

- **Home** — album covers as large tiles (title, date range, count), most recent
  first. Optional "featured" albums later.
- **Album (grid)** — title block, then a justified grid (target row height
  ~280–340 px desktop, ~180 px mobile). Sub-albums as a row of covers above.
- **Album (essay)** — text and images in the sequence of `index.md`: full-width
  single images, rows of 2–3, captions in the margin on wide screens.
  Further layouts to explore later: filmstrip (sprocket-hole band, horizontal
  scroll-snap) and contact sheet, which suit a Fuji / film identity.
- **Viewer** — PhotoSwipe; caption line with key EXIF, `i` toggles a detail panel.
- **Share pages** — same templates, no navigation outside the shared subtree.

## EXIF presentation

A single discreet line under the image in the viewer, e.g.

```
X-T5 · XF23mmF1.4 R LM WR · 23 mm (35 mm eq.) · ƒ/2 · 1/500 s · ISO 160 · Classic Chrome
```

`i` opens the **recipe card** (glass side panel on desktop, bottom sheet on
mobile): film simulation, grain, colour chrome, dynamic range, WB shift,
highlight/shadow tone… rendered inline, no extra request. See [exif.md](exif.md).
