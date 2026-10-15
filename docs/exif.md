# EXIF & Fujifilm metadata

Almost the whole library is Fujifilm JPEG (some Canon). Beyond the usual camera
and exposure fields, Pellicule decodes the **Fujifilm MakerNote** to show the
in-camera "recipe": film simulation, grain, colour chrome, dynamic range, white
balance shift… None of the galleries we looked at do this; it's Pellicule's
signature feature.

## Reading

| Part | How |
|---|---|
| Standard EXIF (JPEG, HEIC) + video dates (MOV/MP4) | [`nom-exif`](https://crates.io/crates/nom-exif) (MIT) |
| Fujifilm MakerNote | Our own parser (`src/meta/fuji.rs`), fed the raw bytes of tag `0x927C` |
| Canon | Standard EXIF only (Canon writes `LensModel` there). Picture Style (Canon MakerNote) maybe later |

Tag IDs and value meanings below come from exiftool's `FujiFilm.pm` tables —
**facts only**, no exiftool code is used.

### Stored fields

`make`, `model`, `lens`, `focal_mm`, `focal_35mm`, `aperture`, `exposure_s`,
`iso`, `exposure_bias`, `taken_at` + `tz_offset` (from `OffsetTimeOriginal`),
`rating`, `gps_lat`/`gps_lon` (private side only), plus `film_sim` (indexed) and
`fuji_recipe` (JSON). A `makernote_version` column lets us re-parse when the
parser improves.

**Capture date chain**: `SubSecDateTimeOriginal` / `DateTimeOriginal` +
`OffsetTimeOriginal` → QuickTime `CreationDate` (videos) → filename pattern →
file mtime. Dates are shown in the capture's local time, never converted to
the viewer's timezone.

## Fujifilm MakerNote format

```
bytes 0..8    "FUJIFILM"                  magic
bytes 8..12   u32 LE  ifd_offset          from the START of the MakerNote (usually 12)
at ifd:       u16 LE  entry_count
              entry_count × 12 bytes:     u16 tag | u16 type | u32 count | u32 value_or_offset
```

- **Always little-endian**, whatever the main EXIF byte order.
- **Offsets are relative to the start of the MakerNote**, so the raw blob is
  self-contained.
- TIFF types: 1 BYTE, 2 ASCII, 3 SHORT, 4 LONG, 7 UNDEFINED, 8 SSHORT, 9 SLONG,
  10 SRATIONAL. `BWAdjustment` / `BWMagentaGreen` are read as `i8` whatever
  their declared type.
- Untrusted input: bounds-check every offset, cap `entry_count` (≤ 512), ignore
  unknown tags, never panic.

## Film simulation

Derived from **two** tags: colour simulations are in `FilmMode` (0x1401), but
**Monochrome, Acros and Sepia live in `Saturation` (0x1003)** — when one of
those is set, `FilmMode` is absent.

| `Saturation` 0x1003 | Film simulation |
|---|---|
| 0x300 / 0x301 / 0x302 / 0x303 | Monochrome / + R / + Ye / + G |
| 0x310 | Sepia |
| 0x500 / 0x501 / 0x502 / 0x503 | Acros / + R / + Ye / + G |

| `FilmMode` 0x1401 | Film simulation |
|---|---|
| 0x000 | Provia / Standard |
| 0x120 | Astia / Soft |
| 0x200 | Velvia / Vivid |
| 0x500 / 0x501 | Pro Neg. Std / Pro Neg. Hi |
| 0x600 | Classic Chrome |
| 0x700 | Eterna / Cinema |
| 0x800 | Classic Neg. |
| 0x900 | Eterna Bleach Bypass |
| 0xA00 | Nostalgic Neg. |
| 0xB00 | Reala Ace |
| 0x100 / 0x110 / 0x130 / 0x300 / 0x400 | Older "Studio Portrait" / F-modes (old bodies) |

Unknown values are shown as `Unknown (0x…)` rather than guessed.

## Recipe tags

| Setting | Tag | Decoding |
|---|---|---|
| Grain roughness | 0x1047 | 0 Off · 32 Weak · 64 Strong |
| Grain size | 0x104C | 0 Off · 16 Small · 32 Large (newer bodies only) |
| Color Chrome Effect | 0x1048 | 0 Off · 32 Weak · 64 Strong |
| Color Chrome FX Blue | 0x104E | 0 Off · 32 Weak · 64 Strong |
| DR setting | 0x1402 | 0x000 Auto · 0x001 Manual · 0x100 Standard · 0x200 Wide1 · 0x201 Wide2 |
| DR value (manual) | 0x1403 | 100 / 200 / 400 → DR100 / DR200 / DR400 |
| DR value (auto) | 0x140B | percentage → "DR-Auto (200%)" |
| D-Range Priority | 0x1443 / 0x1444 / 0x1445 | Auto/Fixed; Weak · Strong (· Plus) → shown instead of DR |
| White balance | 0x1002 | 0x0 Auto · 0x1 Auto White Priority · 0x2 Auto Ambience · 0x100 Daylight · 0x200 Shade · 0x30x Fluorescent · 0x400 Incandescent · 0x600 Underwater · 0xF00–F04 Custom · 0xFF0 Kelvin |
| Colour temperature | 0x1005 | Kelvin, when WB = 0xFF0 |
| WB shift | 0x100A | (R, B) as i32; **newer bodies store ×20** (40, −80 → R+2 B−4) |
| Highlight tone | 0x1041 | display = −raw / 16 (−32 → +2; half steps fall out) |
| Shadow tone | 0x1040 | same as highlight |
| Color | 0x1003 | 0x000 0 · 0x080 +1 · 0x100 +2 · 0x0C0 +3 · 0x0E0 +4 · 0x180 −1 · 0x400 −2 · 0x4C0 −3 · 0x4E0 −4 |
| Sharpness | 0x1001 | 0x00 −4 · 0x01 −3 · 0x02 −2 · 0x82 −1 · 0x03 0 · 0x84 +1 · 0x04 +2 · 0x05 +3 · 0x06 +4 |
| High ISO NR | 0x100E | 0x000 0 · 0x180 +1 · 0x100 +2 · 0x1C0 +3 · 0x1E0 +4 · 0x280 −1 · 0x200 −2 · 0x2C0 −3 · 0x2E0 −4 |
| Clarity | 0x100F | raw / 1000 |
| Monochromatic colour WC / MG | 0x1049 / 0x104B | i8: + warm / − cool; + green / − magenta |
| Re-developed from RAW | 0x1436 | 1 → badge (recipe reflects the in-camera RAW conversion) |
| In-camera rating | 0x1431 | 0–5, a cover / sort hint |

Not decoded by exiftool either (so not shown): Smooth Skin Effect, newer Tone
Curve UI details. We show what is known and don't invent tags.

## Testing

Golden tests: run `exiftool -j -G1 -n -FujiFilm:all` once on 20–30 sample JPEGs
from **each body we own**, commit the JSON next to small fixture files, and diff
our parser against it in CI. exiftool is only a test oracle, never a runtime
dependency. The WB-shift ÷20 rule and the Adobe RGB `R03` detection are
confirmed this way.

## Display

**Caption line** (viewer, always visible, dimmed):

```
X-T5 · XF23mmF1.4 R LM WR · 23mm (35mm eq.) · f/2 · 1/500s · ISO 160 · +⅓ EV · Classic Chrome
```

- `f/2` not `f/2.0`; `1/500s` below a second, `2s` at or above.
- 35 mm equivalent only when it differs from the focal length.
- Exposure bias in thirds: `+⅓ EV`, `−1⅔ EV`, `±0 EV` hidden.

**Recipe card** (`i` key or button; side panel on desktop, bottom sheet on mobile;
typography only, small caps and tabular numbers):

```
CLASSIC CHROME
Grain        Weak · Small        Color Chrome   Strong
CC FX Blue   Weak                Dynamic Range  DR400
White Bal.   Auto  R+2 B−4       Highlight      −1
Shadow       +1                  Color          +2
Sharpness    −1                  High ISO NR    −4
Clarity      0
```

Recipe data is rendered inline in the page (≈300 bytes per photo), so opening
the card needs no request over Funnel.

**Film simulation as a facet** (later): a small chip on grid hover,
`/film/classic-chrome` pages across albums, and an album summary such as
"Mostly Classic Neg. · 3 Acros".

## Privacy

- GPS is stored if present (Fuji bodies only add it when paired with the phone
  app) and shown only on the private side.
- Public shares **never** include GPS; derivatives never carry any metadata.
- `show_exif` / `show_recipe` are per-share flags.
