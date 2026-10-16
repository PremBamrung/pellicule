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
| Placeholders | Average colour as background, then a [ThumbHash](https://evanw.github.io/thumbhash/) (MIT) blur decoded near the viewport, then the photo fading in | Correct colours before anything loads; no flash of empty boxes |
| Typeface | [Geist](https://vercel.com/font) (OFL), self-hosted variable font | One family: large and tight for titles, small and quiet for UI |
| Assets | Embedded in the binary (`rust-embed`), content-hashed URLs | One file to deploy, immutable caching |
| Compression | `tower-http` brotli / gzip for pages, CSS, JS and JSON; images, fonts and videos pass through | Album pages are ~1 KB per photo uncompressed; Funnel is slow |
| Prefetch | Speculation rules: album pages are fetched on hover (Chrome; ignored elsewhere) | Albums open instantly, no JS |

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
.grid  { display: flex; flex-wrap: wrap; gap: var(--gap); }   /* 16 px, 8 px on phones */
.tile  { flex: calc(var(--ar) * 100) 1 calc(var(--ar) * var(--row) * 0.85); }
.grid::after { content: ""; flex-grow: 1e6; }   /* keeps the last row from stretching */
```

The grid keeps the same side margins as the header and the title, and
16 px between photos (8 px on phones; the home rows use the same gap):
photos hang on the wall with space around them, each one a work rather than
a cell of a contact sheet.

Tiles start at 0.85x the target row height and grow to fill the row, so a
row takes one more photo instead of stretching a few wide ones: rows land
between about 0.85x and 1.2x the target (1x to 1.5x with the basis at the
target). The last row stays at 0.85x.

No JS virtualization below ~2k items: native `loading="lazy"` plus
`content-visibility: auto` on row blocks. The first image gets
`fetchpriority="high"`.

## Home and album cards

The home page opens on rows of whole, uncropped photos picked at random,
taking turns (a row narrower than the page is centred). A hairline and an
**Albums** heading, with the library stats on the same line, close the photos
and open the albums.

Each album card, the same on the home page and for sub-albums:

- **Cover**: the first landscape photo (portraits crop badly in 3:2).
- **Colour band**: twelve average colours sampled through the album in
  order, from the colour already stored per photo. Each album gets a
  fingerprint at no extra cost.
- **Summary**: `3 albums · 100 photographs · 6–14 Oct 2023 · 5 days`.
  Sub-albums only when there are some; days are distinct shooting days,
  shown when there are several.
- **Gear**: `X-T3 · X-E3 · 35–84mm`: the three most used camera models,
  then the 35mm-equivalent focal range.

With a mouse, moving across the cover flips it, left to right, through four
photos spread evenly through the album's timeline (beginning, middle, end of
a trip), the same on every visit; albums with fewer than four other photos
don't flip. They are not shown as thumbnails: the card stays a cover and a
colour band. Larger versions load when the pointer arrives and the cover
only switches to a decoded one, so it never flashes. Touch screens keep the
plain card.

## Filters

Every album with photos has the same five menus, in the same order: **Day**,
**Camera**, **Lens**, **Focal length** and **Film** (the Fujifilm film
simulation). A menu with a single value still opens and shows it, with its
count, as information (its checkbox is disabled: ticking it would change
nothing); one with no value says "Not recorded" (Film on an album without
Fujifilm photos).

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
loads `l` (2048). Its `srcset` includes `x`, so the 4096 px variant is fetched
**only when zooming** (originals are not served yet). Neighbouring slides are
preloaded (±1).

The open photo is in the URL (`/a/<album>#p=<key>`): opening the viewer adds a
history entry, so Back (or a phone's back gesture) closes it, and a reload or a
shared link opens the viewer on the same photo. Closing leaves the grid
scrolled to the last photo seen, with keyboard focus on it.

## Transitions

Cross-document View Transitions (`@view-transition { navigation: auto; }`):
an album card's title morphs into the album page title. Names are
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
2. **Typography carries the identity.** One typeface, Geist: a bold wordmark
   (28 px on phones, growing to 44 px), large titles (up to 92 px), weight 500, tight tracking
   (−0.05em); every other text small (13–15 px) and quiet. No all-caps
   labels.
3. **Nothing jumps.** Every image has width/height and a ThumbHash before it
   loads.
4. **Right pixels, right time.** `srcset`/`sizes` pick the smallest adequate
   derivative; originals only on explicit zoom or download.
5. **Color is accurate.** Derivatives are colour-managed: sRGB stays sRGB,
   wide-gamut sources keep a Display-P3 profile (see [cache.md](cache.md)).
6. **Keyboard and touch are first-class.** ←/→, Esc, `i` for info, `r` for
   the recipe, space for the slideshow, `f` for fullscreen, `?` for the list
   of keys, swipe, pinch.
   Every photo link is named (its date) for screen readers.

## Pages

- **Home** — opens on a row of whole photographs, never cropped: 16 random
  photos (portraits too) are split into rows of one height (about half the
  screen, less on phones) that fill the page width, each row the combination
  closest to that height. The frame keeps one height, so nothing below moves
  when the next row crossfades in every 8 s; dots pick a row. It pauses on
  hover, focus and in background tabs, and doesn't move with reduced motion.
  The album and date of the photo pointed at show under the row; a photo
  opens the viewer on it in its album (`/a/<album>#p=<key>`). Then a line of
  library stats ("2 albums, 200 photographs, 2023–2025") and the album
  covers, most recent first.
- **Album** — the title, a summary, then the album bar: filters on the left,
  grid density **S / M / L** on the right (remembered per browser). Once the
  title has scrolled away the bar stays at the top, frosted, with the
  wordmark (home), the album name and the current day ("Saturday 7 October")
  — on narrower screens on a line of its own. Albums spanning several days
  are split into **day sections** with a quiet heading each.
- **Album on phones** — the density switch has two sizes, **S** (the grid;
  a remembered M shows as S) and **L** (one photo per row, full width),
  on the filters' last line. The filters stay at the top of the page and
  scroll away; only a one-line frosted strip follows (`Pellicule · Japon ·
  Saturday 7 October`). Tablets and phones turned sideways (wider than
  720 px) get S / M / L and the desktop bar.
- **Viewer** — PhotoSwipe with the museum label (hidden while zoomed in);
  **slideshow** via the play button or the space bar (4 s per photo);
  **fullscreen** via its button or `f` (not on iPhone, where pages can't go
  fullscreen; closing the viewer leaves it); `i` hides the label; `?` or its
  button (shown once a mouse is used) lists the keys; on Fujifilm photos the
  film simulation ends the label's gear line, and the **Recipe** button or
  `r` shows the in-camera recipe beside the photo (on phones a bottom
  sheet that starts with the label's lines, with the photo fitted above
  it; see [exif.md](exif.md)).
- **Library** — scans and their history (see [architecture](architecture.md)).

### Planned

- **Album (essay)** — text and images in the sequence of `index.md`: full-width
  single images, rows of 2–3, captions in the margin on wide screens.
  A contact-sheet layout may be explored later. No film decoration
  (sprocket holes, film-strip graphics): the identity comes from the photos
  and the recipe, not from imitating film.
- **Share pages** — same templates, no navigation outside the shared subtree.

## EXIF presentation

A small museum-style label in the bottom-left corner of the viewer:

```
7 October 2023, 10:04
Fujifilm X-T3, XF35mmF1.4 R
35mm (53mm eq.), f/2, 1/210s, ISO 640, −1 EV
```

The film simulation ends the gear line on Fujifilm photos ("… · Classic
Chrome"); the **Recipe** button or `r` opens the full recipe beside the photo
(a bottom sheet on phones that repeats the label, which it covers), rendered
from the page, no extra request. See
[exif.md](exif.md).
