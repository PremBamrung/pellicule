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
| Fujifilm MakerNote | Our own parser (`src/meta/fuji.rs`), fed the raw bytes of tag `0x927C`, only when `Make` is FUJIFILM |
| Canon | Standard EXIF only (Canon writes `LensModel` there). Picture Style (Canon MakerNote) maybe later |

Tag IDs and value meanings below come from exiftool's `FujiFilm.pm` tables —
**facts only**, no exiftool code is used.

### Stored fields

`make`, `model`, `lens`, `focal_mm`, `focal_35mm`, `aperture`, `exposure_s`,
`iso`, `exposure_bias`, `taken_at` + `tz_offset` (from `OffsetTimeOriginal`),
plus `film_sim` (indexed) and `fuji_recipe`. *Planned:* `rating`,
`gps_lat`/`gps_lon` (private side only).

`fuji_recipe` holds the recipe as display rows, one `label<TAB>value` line
each (`Shadow` and `−2`), which the viewer shows as they are; the film simulation
is its own column so it can be filtered on. `meta_version` records which
version of the reader (`META_VERSION` in `src/meta.rs`) filled a row: after
a bump, the next scan re-reads the metadata of every photo without touching
the derivatives (506 photos in a third of a second on the Mac).

Photos from other makes (the Canon EOS 600D) simply have no film simulation
and no recipe: nothing is guessed and nothing empty is shown.

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
| D-Range Priority | 0x1443 / 0x1444 / 0x1445 | Auto/Fixed; Weak · Strong (· Plus). **Not decoded yet**: none of our bodies has it, so it can't be verified |
| White balance | 0x1002 | 0x0 Auto · 0x1 Auto White Priority · 0x2 Auto Ambience · 0x100 Daylight · 0x200 Shade · 0x300–0x304 Fluorescent (daylight, day white, white, warm white, living room) · 0x400 Incandescent · 0x500 Flash · 0x600 Underwater · 0xF00–F04 Custom · 0xFF0 Kelvin |
| Colour temperature | 0x1005 | Kelvin, when WB = 0xFF0 |
| WB shift | 0x100A | (R, B) as i32; **newer bodies store ×20** (40, −80 → R+2 B−4; the X-T3 and X-E3 do). Values that aren't multiples of 20 are taken as steps. exiftool prints them unscaled |
| Highlight tone | 0x1041 | display = −raw / 16 (−32 → +2; half steps fall out) |
| Shadow tone | 0x1040 | same as highlight |
| Color | 0x1003 | 0x000 0 · 0x080 +1 · 0x100 +2 · 0x0C0 +3 · 0x0E0 +4 · 0x180 −1 · 0x400 −2 · 0x4C0 −3 · 0x4E0 −4 |
| Sharpness | 0x1001 | 0x00 −4 · 0x01 −3 · 0x02 −2 · 0x82 −1 · 0x03 0 · 0x84 +1 · 0x04 +2 · 0x05 +3 · 0x06 +4 |
| High ISO NR | 0x100E | 0x000 0 · 0x180 +1 · 0x100 +2 · 0x1C0 +3 · 0x1E0 +4 · 0x280 −1 · 0x200 −2 · 0x2C0 −3 · 0x2E0 −4 |
| Clarity | 0x100F | raw / 1000 |
| Monochromatic colour WC / MG | 0x1049 / 0x104B | i8: + warm / − cool; + green / − magenta |
| Re-developed from RAW | 0x1436 | 1 → "Developed: In camera, from RAW" (the recipe reflects the in-camera RAW conversion) |
| In-camera rating | 0x1431 | 0–5, a cover / sort hint *(not read yet)* |

Not decoded by exiftool either (so not shown): Smooth Skin Effect, newer Tone
Curve UI details. We show what is known and don't invent tags.

## Testing

Golden tests on one photo per distinct recipe in the library (four X-T3,
two X-E3: Classic Chrome, Acros + R, Astia; Daylight, Custom and Kelvin
white balance; manual and auto DR). `scripts/make-fuji-fixtures.sh` (run in
the Docker `test` image, see [development](development.md)) saves for each
one only the **MakerNote bytes** (about 1 KB, no image) with the camera's
internal serial number zeroed, and exiftool's raw reading (`-n`) as JSON in
`tests/fixtures/fuji/`.

The tests check that the parser reads the same raw values as exiftool for
every recipe tag, then the displayed wording against exiftool's printed
values; plus every truncation and corrupted header byte of a real note
(never a panic) and a non-Fujifilm note. exiftool is only a test oracle,
never a runtime dependency. The WB-shift ÷20 rule is confirmed this way
(every X-T3 / X-E3 value is a multiple of 20).

## Display

**Museum label** (viewer; `i` hides it): the film simulation ends the gear
line.

```
6 October 2023, 12:22
Fujifilm X-T3, XF16-80mmF4 R OIS WR · Acros + R
17.1mm (26mm eq.), f/9, 1/350s, ISO 640, −⅔ EV
```

- `f/2` not `f/2.0`; `1/500s` below a second, `2s` at or above.
- 35 mm equivalent only when it differs from the focal length.
- Exposure bias in thirds: `+⅓ EV`, `−1⅔ EV`, `±0 EV` hidden.

**Recipe** (the "Recipe" button or `r`; a panel beside the photo on
desktop, a sheet at the bottom on phones; typography only, tabular numbers;
it stays open from photo to photo until closed, Esc closes it first). Only
the settings the camera recorded are listed, in this order:

```
Classic Chrome
Grain            Off
Colour chrome    Strong
Dynamic range    DR400
White balance    Daylight · R+3 B−5
Highlight        −1
Shadow           −2
Colour           +3          (Toning WC / MG instead, for monochrome)
Sharpness        +1
Noise reduction  −1
```

Grain size, Colour chrome blue and Clarity appear on bodies that record
them (not the X-T3 / X-E3). No film decoration: the photos and the words do
the work.

Recipe data is rendered inline in the page (≈300 bytes per photo), so opening
the card needs no request over Funnel.

**Film simulation as a facet**: a **Film** menu on every album, after Focal
length (see [frontend](frontend.md)). Later: `/film/classic-chrome` pages
across albums, and an album summary such as "Mostly Classic Neg. · 3 Acros".

## Privacy

- GPS is stored if present (Fuji bodies only add it when paired with the phone
  app) and shown only on the private side.
- Public shares **never** include GPS; derivatives never carry any metadata.
- `show_exif` / `show_recipe` are per-share flags.
