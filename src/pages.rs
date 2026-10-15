//! Page data: SQLite queries turned into what the templates render.

use anyhow::Result;
use askama::Template;
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::derive::{self, VARIANTS, Variant};
use crate::meta;

/// Target row height of the justified grid, in CSS px. Must match `--row`
/// in app.css; used to compute `sizes` so the browser picks the right file.
const ROW_HEIGHT: f64 = 300.0;

pub struct Layout {
    pub css_url: String,
    pub js_url: String,
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomePage<'a> {
    pub layout: &'a Layout,
    pub hero: Option<Hero>,
    /// "2 albums, 200 photographs, 2023–2025"
    pub stats: String,
    pub albums: Vec<AlbumCard>,
    pub tiles: Vec<Tile>,
}

/// The large photo that opens the home page, picked at random.
pub struct Hero {
    pub tile: Tile,
    pub album_href: String,
    pub album_title: String,
}

#[derive(Template)]
#[template(path = "album.html")]
pub struct AlbumPage<'a> {
    pub layout: &'a Layout,
    pub title: String,
    pub crumbs: Vec<Crumb>,
    pub summary: String,
    pub cover: Option<Tile>,
    pub albums: Vec<AlbumCard>,
    /// One section per shooting day when the album spans several days,
    /// otherwise a single unlabelled section.
    pub sections: Vec<Section>,
    pub filters: Vec<FilterGroup>,
}

pub struct Section {
    pub label: String,
    pub tiles: Vec<Tile>,
}

/// One filter menu (day, camera, lens, focal) with the values present in
/// the album. Filtering itself happens in the browser (static/app.js).
pub struct FilterGroup {
    pub key: &'static str,
    pub label: &'static str,
    pub options: Vec<FilterOption>,
}

pub struct FilterOption {
    pub value: String,
    pub label: String,
    pub count: usize,
}

#[derive(Template)]
#[template(path = "library.html")]
pub struct LibraryPage<'a> {
    pub layout: &'a Layout,
    pub summary: String,
    pub runs: Vec<ScanRun>,
    pub failures: Vec<Failure>,
}

pub struct ScanRun {
    pub started: String,
    pub trigger: String,
    pub status: String,
    pub duration: String,
    pub processed: String,
    pub failed: String,
    pub missing: String,
    pub error: String,
}

pub struct Failure {
    pub path: String,
    pub error: String,
}

pub struct Crumb {
    pub href: String,
    pub name: String,
}

pub struct AlbumCard {
    pub href: String,
    pub title: String,
    pub summary: String,
    pub cover: Option<Tile>,
    latest: Option<String>,
}

pub struct Tile {
    pub aspect: String,
    pub background: String,
    pub sizes: String,
    pub thumb_url: String,
    pub thumb_srcset: String,
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub cover_srcset: String,
    /// For full-width images (hero, album header).
    pub wide_srcset: String,
    /// Base64 ThumbHash, decoded into a blurred placeholder by app.js.
    pub thumbhash: String,
    pub view_url: String,
    pub zoom_srcset: String,
    pub zoom_width: u32,
    pub zoom_height: u32,
    pub label_date: String,
    pub label_gear: String,
    pub label_exposure: String,
    /// Filter values; empty when unknown.
    pub day: String,
    pub camera: String,
    pub lens: String,
    pub focal: &'static str,
}

struct AlbumRow {
    id: i64,
    rel_path: String,
    name: String,
}

pub fn home<'a>(conn: &Connection, layout: &'a Layout) -> Result<HomePage<'a>> {
    let root = album_by_path(conn, "")?;
    let (albums, tiles) = match root {
        Some(root) => (child_cards(conn, root.id)?, tiles(conn, root.id)?),
        None => (Vec::new(), Vec::new()),
    };
    Ok(HomePage {
        layout,
        hero: hero(conn)?,
        stats: library_stats(conn)?,
        albums,
        tiles,
    })
}

/// A random landscape photo from the whole library, new on every visit.
fn hero(conn: &Connection) -> Result<Option<Hero>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {MEDIA_COLUMNS}, a.rel_path, a.name
                 FROM media m JOIN album a ON a.id = m.album_id
                 WHERE {VISIBLE} AND m.width > m.height
                 ORDER BY random() LIMIT 1"
            ),
            [],
            |r| {
                let rel_path: String = r.get(15)?;
                Ok(Hero {
                    tile: tile_from_row(r)?,
                    album_href: if rel_path.is_empty() {
                        "/".into()
                    } else {
                        album_href(&rel_path)
                    },
                    album_title: r.get(16)?,
                })
            },
        )
        .optional()?)
}

fn library_stats(conn: &Connection) -> Result<String> {
    let (albums, photos, first, last): (i64, i64, Option<String>, Option<String>) = conn
        .query_row(
            &format!(
                "SELECT count(DISTINCT m.album_id), count(*), min(m.taken_at), max(m.taken_at)
                 FROM media m JOIN album a ON a.id = m.album_id WHERE {VISIBLE}"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    if photos == 0 {
        return Ok(String::new());
    }
    let mut stats = format!(
        "{albums} {}, {photos} {}",
        if albums == 1 { "album" } else { "albums" },
        if photos == 1 {
            "photograph"
        } else {
            "photographs"
        },
    );
    let year = |v: Option<String>| v.and_then(|v| v.get(..4).map(str::to_owned));
    match (year(first), year(last)) {
        (Some(a), Some(b)) if a != b => stats.push_str(&format!(", {a}–{b}")),
        (Some(a), _) => stats.push_str(&format!(", {a}")),
        _ => {}
    }
    Ok(stats)
}

/// `None` when the album doesn't exist (or is missing / empty).
pub fn album<'a>(
    conn: &Connection,
    layout: &'a Layout,
    rel_path: &str,
) -> Result<Option<AlbumPage<'a>>> {
    let rel_path = rel_path.trim_matches('/');
    if rel_path.is_empty() {
        return Ok(None);
    }
    let Some(album) = album_by_path(conn, rel_path)? else {
        return Ok(None);
    };

    let mut crumbs = Vec::new();
    let mut prefix = String::new();
    let parts: Vec<&str> = rel_path.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        crumbs.push(Crumb {
            href: album_href(&prefix),
            name: (*part).to_string(),
        });
    }

    let (count, first, last) = album_stats(conn, &album.rel_path)?;
    if count == 0 {
        return Ok(None);
    }
    let tiles = tiles(conn, album.id)?;
    Ok(Some(AlbumPage {
        layout,
        title: album.name,
        crumbs,
        summary: summary(count, first.as_deref(), last.as_deref()),
        cover: cover(conn, &album.rel_path)?,
        albums: child_cards(conn, album.id)?,
        filters: filters(&tiles),
        sections: by_day(tiles),
    }))
}

/// Splits a chronological list of tiles into one section per day, when the
/// album spans more than one day.
fn by_day(tiles: Vec<Tile>) -> Vec<Section> {
    let days: std::collections::BTreeSet<&str> = tiles.iter().map(|t| t.day.as_str()).collect();
    if days.len() < 2 {
        return vec![Section {
            label: String::new(),
            tiles,
        }];
    }
    let several_years = days
        .iter()
        .filter(|d| !d.is_empty())
        .map(|d| &d[..4])
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1;

    let mut sections: Vec<Section> = Vec::new();
    let mut current_day: Option<String> = None;
    for tile in tiles {
        if current_day.as_deref() != Some(tile.day.as_str()) {
            current_day = Some(tile.day.clone());
            sections.push(Section {
                label: day_label(&tile.day, several_years),
                tiles: Vec::new(),
            });
        }
        sections.last_mut().expect("pushed above").tiles.push(tile);
    }
    sections
}

/// "Saturday 7 October" (with the year when the album spans several).
fn day_label(day: &str, with_year: bool) -> String {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|d| {
            d.format(if with_year {
                "%A %-d %B %Y"
            } else {
                "%A %-d %B"
            })
            .to_string()
        })
        .unwrap_or_else(|_| "Undated".to_string())
}

/// The album's cover: its first landscape photo (portraits crop badly in
/// wide frames), or its first photo.
fn cover(conn: &Connection, rel_path: &str) -> Result<Option<Tile>> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {MEDIA_COLUMNS} FROM media m JOIN album a ON a.id = m.album_id
                 WHERE {IN_SUBTREE}
                 ORDER BY m.width < m.height, m.taken_at IS NULL, m.taken_at, m.file_name
                 LIMIT 1"
            ),
            params![rel_path],
            tile_from_row,
        )
        .optional()?)
}

fn album_by_path(conn: &Connection, rel_path: &str) -> Result<Option<AlbumRow>> {
    Ok(conn
        .query_row(
            "SELECT id, rel_path, name FROM album WHERE rel_path = ?1 AND missing_since IS NULL",
            params![rel_path],
            |r| {
                Ok(AlbumRow {
                    id: r.get(0)?,
                    rel_path: r.get(1)?,
                    name: r.get(2)?,
                })
            },
        )
        .optional()?)
}

/// Visible media below an album path (recursively): the album itself or
/// anything whose path starts with `<rel_path>/`.
const IN_SUBTREE: &str = "(a.rel_path = ?1 OR substr(a.rel_path, 1, length(?1) + 1) = ?1 || '/')
     AND a.missing_since IS NULL AND m.missing_since IS NULL
     AND m.error IS NULL AND m.content_key IS NOT NULL";

/// Media that can be shown anywhere in the library.
const VISIBLE: &str = "a.missing_since IS NULL AND m.missing_since IS NULL
     AND m.error IS NULL AND m.content_key IS NOT NULL";

fn album_stats(conn: &Connection, rel_path: &str) -> Result<(i64, Option<String>, Option<String>)> {
    Ok(conn.query_row(
        &format!(
            "SELECT count(*), min(m.taken_at), max(m.taken_at)
             FROM media m JOIN album a ON a.id = m.album_id WHERE {IN_SUBTREE}"
        ),
        params![rel_path],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}

fn child_cards(conn: &Connection, parent_id: i64) -> Result<Vec<AlbumCard>> {
    let mut stmt = conn.prepare(
        "SELECT id, rel_path, name FROM album
         WHERE parent_id = ?1 AND missing_since IS NULL ORDER BY name",
    )?;
    let children: Vec<AlbumRow> = stmt
        .query_map(params![parent_id], |r| {
            Ok(AlbumRow {
                id: r.get(0)?,
                rel_path: r.get(1)?,
                name: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut cards = Vec::new();
    for child in children {
        let (count, first, last) = album_stats(conn, &child.rel_path)?;
        if count == 0 {
            continue;
        }
        let cover = cover(conn, &child.rel_path)?;
        cards.push(AlbumCard {
            href: album_href(&child.rel_path),
            title: child.name,
            summary: summary(count, first.as_deref(), last.as_deref()),
            cover,
            latest: last,
        });
    }
    // Most recent first, like a portfolio. ISO timestamps sort as text.
    cards.sort_by(|a, b| b.latest.cmp(&a.latest));
    Ok(cards)
}

const MEDIA_COLUMNS: &str = "m.content_key, m.width, m.height, m.avg_rgb, m.make, m.model, m.lens,
     m.focal_mm, m.focal_35mm, m.aperture, m.exposure_s, m.iso, m.exposure_bias, m.taken_at,
     m.thumbhash";

fn tiles(conn: &Connection, album_id: i64) -> Result<Vec<Tile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLUMNS} FROM media m JOIN album a ON a.id = m.album_id
         WHERE m.album_id = ?1 AND m.missing_since IS NULL AND m.error IS NULL
           AND m.content_key IS NOT NULL
         ORDER BY m.taken_at IS NULL, m.taken_at, m.file_name"
    ))?;
    let tiles = stmt
        .query_map(params![album_id], tile_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(tiles)
}

fn tile_from_row(row: &Row) -> rusqlite::Result<Tile> {
    let key: String = row.get(0)?;
    let width: u32 = row.get(1)?;
    let height: u32 = row.get(2)?;
    let avg_rgb: u32 = row.get::<_, Option<u32>>(3)?.unwrap_or(0x30_30_30);
    let make: Option<String> = row.get(4)?;
    let model: Option<String> = row.get(5)?;
    let lens: Option<String> = row.get(6)?;
    let focal_mm: Option<f64> = row.get(7)?;
    let focal_35mm: Option<i64> = row.get(8)?;
    let aperture: Option<f64> = row.get(9)?;
    let exposure_s: Option<f64> = row.get(10)?;
    let iso: Option<i64> = row.get(11)?;
    let bias: Option<f64> = row.get(12)?;
    let taken_at: Option<String> = row.get(13)?;
    let thumbhash: Option<Vec<u8>> = row.get(14)?;

    let url = |v: Variant| format!("/d/{}", derive::file_name(&key, v));
    let dims = |v: Variant| derive::fit(width, height, v);
    let [x, l, m, t] = VARIANTS;
    let srcset = |variants: &[Variant]| {
        variants
            .iter()
            .map(|v| format!("{} {}w", url(*v), dims(*v).0))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let aspect = width as f64 / height.max(1) as f64;
    let (thumb_width, thumb_height) = dims(t);
    let (zoom_width, zoom_height) = dims(x);

    let mut exposure = Vec::new();
    if let Some(focal) = focal_mm {
        exposure.push(meta::format_focal(focal, focal_35mm));
    }
    if let Some(f) = aperture {
        exposure.push(meta::format_aperture(f));
    }
    if let Some(s) = exposure_s {
        exposure.push(meta::format_shutter(s));
    }
    if let Some(iso) = iso {
        exposure.push(format!("ISO {iso}"));
    }
    if let Some(bias) = bias.and_then(meta::format_bias) {
        exposure.push(bias);
    }

    Ok(Tile {
        aspect: format!("{aspect:.4}"),
        background: format!("#{avg_rgb:06x}"),
        sizes: format!("{}px", (aspect * ROW_HEIGHT).round()),
        thumb_url: url(t),
        thumb_srcset: srcset(&[t, m]),
        thumb_width,
        thumb_height,
        cover_srcset: srcset(&[m, l]),
        wide_srcset: srcset(&[m, l, x]),
        thumbhash: thumbhash.as_deref().map(base64).unwrap_or_default(),
        view_url: url(l),
        zoom_srcset: srcset(&[t, m, l, x]),
        zoom_width,
        zoom_height,
        label_date: taken_at
            .as_deref()
            .map(format_date_time)
            .unwrap_or_default(),
        label_gear: gear(make.as_deref(), model.as_deref(), lens.as_deref()),
        label_exposure: exposure.join(", "),
        day: taken_at
            .as_deref()
            .and_then(parse)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        camera: body(make.as_deref(), model.as_deref()),
        lens: lens.unwrap_or_default(),
        focal: focal_mm
            .map(|mm| focal_range(focal_35mm.map_or(mm, |eq| eq as f64)).0)
            .unwrap_or(""),
    })
}

/// Focal ranges in 35mm-equivalent: (value, label). Zooms record values
/// like 30.8mm, so ranges read better than exact lengths.
const FOCAL_RANGES: [(&str, &str, f64); 4] = [
    ("wide", "Wide, under 35mm", 35.0),
    ("normal", "Normal, 35–70mm", 70.0),
    ("short-tele", "Short tele, 70–135mm", 135.0),
    ("tele", "Tele, 135mm and over", f64::INFINITY),
];

fn focal_range(equivalent_mm: f64) -> (&'static str, &'static str) {
    FOCAL_RANGES
        .iter()
        .find(|(_, _, below)| equivalent_mm < *below)
        .map(|(value, label, _)| (*value, *label))
        .expect("last range is unbounded")
}

/// Menus only appear when they offer a real choice (two values or more).
fn filters(tiles: &[Tile]) -> Vec<FilterGroup> {
    fn counted<'a>(values: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for value in values.filter(|v| !v.is_empty()) {
            *counts.entry(value).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(v, n)| (v.to_string(), n))
            .collect()
    }

    let days = counted(tiles.iter().map(|t| t.day.as_str()));
    let several_years = days
        .first()
        .zip(days.last())
        .is_some_and(|(a, b)| a.0[..4] != b.0[..4]);
    let day_label = |value: &str| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|d| {
                d.format(if several_years {
                    "%a %-d %b %Y"
                } else {
                    "%a %-d %b"
                })
                .to_string()
            })
            .unwrap_or_else(|_| value.to_string())
    };

    let focal_counts = counted(tiles.iter().map(|t| t.focal));
    let focal = FOCAL_RANGES
        .iter()
        .filter_map(|(value, label, _)| {
            let count = focal_counts.iter().find(|(v, _)| v == value)?.1;
            Some(FilterOption {
                value: value.to_string(),
                label: label.to_string(),
                count,
            })
        })
        .collect();

    let plain = |values: Vec<(String, usize)>| {
        values
            .into_iter()
            .map(|(value, count)| FilterOption {
                label: value.clone(),
                value,
                count,
            })
            .collect()
    };

    [
        FilterGroup {
            key: "day",
            label: "Day",
            options: days
                .into_iter()
                .map(|(value, count)| FilterOption {
                    label: day_label(&value),
                    value,
                    count,
                })
                .collect(),
        },
        FilterGroup {
            key: "camera",
            label: "Camera",
            options: plain(counted(tiles.iter().map(|t| t.camera.as_str()))),
        },
        FilterGroup {
            key: "lens",
            label: "Lens",
            options: plain(counted(tiles.iter().map(|t| t.lens.as_str()))),
        },
        FilterGroup {
            key: "focal",
            label: "Focal length",
            options: focal,
        },
    ]
    .into_iter()
    .filter(|group| group.options.len() >= 2)
    .collect()
}

/// "Fujifilm X-T3, XF56mmF1.2 R".
fn gear(make: Option<&str>, model: Option<&str>, lens: Option<&str>) -> String {
    let body = body(make, model);
    match lens {
        Some(lens) if !body.is_empty() => format!("{body}, {lens}"),
        Some(lens) => lens.to_string(),
        None => body,
    }
}

/// "Fujifilm X-T3". Makers write their name in capitals and sometimes
/// repeat it in the model ("Canon EOS R6").
fn body(make: Option<&str>, model: Option<&str>) -> String {
    let make = make.map(|m| match m.to_ascii_uppercase().as_str() {
        "FUJIFILM" => "Fujifilm".to_string(),
        "CANON" => "Canon".to_string(),
        "NIKON CORPORATION" => "Nikon".to_string(),
        "SONY" => "Sony".to_string(),
        _ => m.to_string(),
    });
    match (make, model) {
        (Some(make), Some(model)) if model.to_lowercase().starts_with(&make.to_lowercase()) => {
            model.to_string()
        }
        (Some(make), Some(model)) => format!("{make} {model}"),
        (None, Some(model)) => model.to_string(),
        (Some(make), None) => make,
        (None, None) => String::new(),
    }
}

pub fn library<'a>(
    conn: &Connection,
    layout: &'a Layout,
    cache: &std::path::Path,
) -> Result<LibraryPage<'a>> {
    let (albums, photos): (i64, i64) = conn.query_row(
        "SELECT count(DISTINCT m.album_id), count(*) FROM media m JOIN album a ON a.id = m.album_id
         WHERE m.missing_since IS NULL AND a.missing_since IS NULL
           AND m.error IS NULL AND m.content_key IS NOT NULL",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let last_scan: Option<i64> = conn.query_row(
        "SELECT max(finished_at) FROM scan_run WHERE status = 'done'",
        [],
        |r| r.get(0),
    )?;

    let mut summary = format!(
        "{albums} {}, {photos} {}, {} of previews on disk.",
        if albums == 1 { "album" } else { "albums" },
        if photos == 1 {
            "photograph"
        } else {
            "photographs"
        },
        format_bytes(dir_size(&cache.join("d"))),
    );
    if let Some(ts) = last_scan {
        summary.push_str(&format!(" Last complete scan {}.", format_timestamp(ts)));
    }

    let mut stmt = conn.prepare(
        "SELECT started_at, finished_at, trigger, rebuild, status, processed, failed, missing, error
         FROM scan_run ORDER BY id DESC LIMIT 20",
    )?;
    let runs = stmt
        .query_map([], |r| {
            let started: i64 = r.get(0)?;
            let finished: Option<i64> = r.get(1)?;
            let trigger: String = r.get(2)?;
            let rebuild: bool = r.get(3)?;
            let number = |v: Option<i64>| v.map(|n| n.to_string()).unwrap_or_default();
            Ok(ScanRun {
                started: format_timestamp(started),
                trigger: match (trigger.as_str(), rebuild) {
                    ("startup", _) => "On start".into(),
                    ("cli", true) => "Command line, rebuild".into(),
                    ("cli", false) => "Command line".into(),
                    (_, true) => "Rebuild".into(),
                    _ => "Rescan".into(),
                },
                status: r.get::<_, String>(4)?,
                duration: finished
                    .map(|f| format_duration(f - started))
                    .unwrap_or_default(),
                processed: number(r.get(5)?),
                failed: number(r.get(6)?),
                missing: number(r.get(7)?),
                error: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut stmt = conn.prepare(
        "SELECT a.rel_path, m.file_name, m.error FROM media m JOIN album a ON a.id = m.album_id
         WHERE m.error IS NOT NULL AND m.missing_since IS NULL ORDER BY a.rel_path, m.file_name LIMIT 100",
    )?;
    let failures = stmt
        .query_map([], |r| {
            let album: String = r.get(0)?;
            let file: String = r.get(1)?;
            Ok(Failure {
                path: if album.is_empty() {
                    file
                } else {
                    format!("{album}/{file}")
                },
                error: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(LibraryPage {
        layout,
        summary,
        runs,
        failures,
    })
}

fn dir_size(dir: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_duration(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

/// Local time of the server (set `TZ` in Docker).
fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%-d %b %Y, %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// "760 photographs, March – April 2024"
fn summary(count: i64, first: Option<&str>, last: Option<&str>) -> String {
    let noun = if count == 1 {
        "photograph"
    } else {
        "photographs"
    };
    match date_span(first, last) {
        Some(span) => format!("{count} {noun}, {span}"),
        None => format!("{count} {noun}"),
    }
}

fn parse(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok()
}

fn date_span(first: Option<&str>, last: Option<&str>) -> Option<String> {
    let first = parse(first?)?;
    let last = last.and_then(parse).unwrap_or(first);
    let (fy, ly) = (first.format("%Y"), last.format("%Y"));
    let (fm, lm) = (first.format("%B"), last.format("%B"));
    Some(
        if first.format("%Y-%m").to_string() == last.format("%Y-%m").to_string() {
            format!("{fm} {fy}")
        } else if fy.to_string() == ly.to_string() {
            format!("{fm} – {lm} {ly}")
        } else {
            format!("{fm} {fy} – {lm} {ly}")
        },
    )
}

/// "12 March 2024, 14:32"
fn format_date_time(value: &str) -> String {
    parse(value)
        .map(|dt| dt.format("%-d %B %Y, %H:%M").to_string())
        .unwrap_or_default()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `/a/` + percent-encoded path segments.
pub fn album_href(rel_path: &str) -> String {
    let mut href = String::from("/a/");
    for (i, segment) in rel_path.split('/').enumerate() {
        if i > 0 {
            href.push('/');
        }
        for byte in segment.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                href.push(byte as char);
            } else {
                href.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    href
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hrefs_are_encoded_per_segment() {
        assert_eq!(album_href("Japon/day 2"), "/a/Japon/day%202");
        assert_eq!(album_href("Mariage Thomas"), "/a/Mariage%20Thomas");
        assert_eq!(album_href("Été"), "/a/%C3%89t%C3%A9");
    }

    #[test]
    fn gear_reads_like_a_label() {
        assert_eq!(
            gear(Some("FUJIFILM"), Some("X-T3"), Some("XF56mmF1.2 R")),
            "Fujifilm X-T3, XF56mmF1.2 R"
        );
        assert_eq!(
            gear(Some("Canon"), Some("Canon EOS R6"), None),
            "Canon EOS R6"
        );
        assert_eq!(gear(None, None, None), "");
    }

    #[test]
    fn focal_ranges_use_35mm_equivalent() {
        assert_eq!(focal_range(24.0).0, "wide");
        assert_eq!(focal_range(35.0).0, "normal");
        assert_eq!(focal_range(84.0).0, "short-tele");
        assert_eq!(focal_range(135.0).0, "tele");
    }

    #[test]
    fn human_sizes_and_durations() {
        assert_eq!(format_bytes(512), "512 bytes");
        assert_eq!(format_bytes(1_340_000_000), "1.3 GB");
        assert_eq!(format_duration(42), "42s");
        assert_eq!(format_duration(125), "2m 05s");
        assert_eq!(format_duration(7260), "2h 01m");
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn date_spans() {
        let d = |s: &str| Some(s.to_string());
        assert_eq!(
            date_span(
                d("2024-03-02T10:00:00").as_deref(),
                d("2024-03-20T10:00:00").as_deref()
            )
            .unwrap(),
            "March 2024"
        );
        assert_eq!(
            date_span(
                d("2024-03-02T10:00:00").as_deref(),
                d("2024-04-01T10:00:00").as_deref()
            )
            .unwrap(),
            "March – April 2024"
        );
        assert_eq!(
            date_span(
                d("2023-12-30T10:00:00").as_deref(),
                d("2024-01-02T10:00:00").as_deref()
            )
            .unwrap(),
            "December 2023 – January 2024"
        );
        assert_eq!(summary(1, None, None), "1 photograph");
    }
}
