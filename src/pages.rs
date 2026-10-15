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
    pub albums: Vec<AlbumCard>,
    pub tiles: Vec<Tile>,
}

#[derive(Template)]
#[template(path = "album.html")]
pub struct AlbumPage<'a> {
    pub layout: &'a Layout,
    pub title: String,
    pub crumbs: Vec<Crumb>,
    pub summary: String,
    pub albums: Vec<AlbumCard>,
    pub tiles: Vec<Tile>,
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
    pub view_url: String,
    pub zoom_srcset: String,
    pub zoom_width: u32,
    pub zoom_height: u32,
    pub label_date: String,
    pub label_gear: String,
    pub label_exposure: String,
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
    Ok(HomePage { layout, albums, tiles })
}

/// `None` when the album doesn't exist (or is missing / empty).
pub fn album<'a>(conn: &Connection, layout: &'a Layout, rel_path: &str) -> Result<Option<AlbumPage<'a>>> {
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
        crumbs.push(Crumb { href: album_href(&prefix), name: (*part).to_string() });
    }

    let (count, first, last) = album_stats(conn, &album.rel_path)?;
    if count == 0 {
        return Ok(None);
    }
    Ok(Some(AlbumPage {
        layout,
        title: album.name,
        crumbs,
        summary: summary(count, first.as_deref(), last.as_deref()),
        albums: child_cards(conn, album.id)?,
        tiles: tiles(conn, album.id)?,
    }))
}

fn album_by_path(conn: &Connection, rel_path: &str) -> Result<Option<AlbumRow>> {
    Ok(conn
        .query_row(
            "SELECT id, rel_path, name FROM album WHERE rel_path = ?1 AND missing_since IS NULL",
            params![rel_path],
            |r| Ok(AlbumRow { id: r.get(0)?, rel_path: r.get(1)?, name: r.get(2)? }),
        )
        .optional()?)
}

/// Visible media below an album path (recursively): the album itself or
/// anything whose path starts with `<rel_path>/`.
const IN_SUBTREE: &str = "(a.rel_path = ?1 OR substr(a.rel_path, 1, length(?1) + 1) = ?1 || '/')
     AND a.missing_since IS NULL AND m.missing_since IS NULL
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
            Ok(AlbumRow { id: r.get(0)?, rel_path: r.get(1)?, name: r.get(2)? })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut cards = Vec::new();
    for child in children {
        let (count, first, last) = album_stats(conn, &child.rel_path)?;
        if count == 0 {
            continue;
        }
        let cover = conn
            .query_row(
                &format!(
                    "SELECT {MEDIA_COLUMNS} FROM media m JOIN album a ON a.id = m.album_id
                     WHERE {IN_SUBTREE}
                     ORDER BY m.taken_at IS NULL, m.taken_at, m.file_name LIMIT 1"
                ),
                params![child.rel_path],
                tile_from_row,
            )
            .optional()?;
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
     m.focal_mm, m.focal_35mm, m.aperture, m.exposure_s, m.iso, m.exposure_bias, m.taken_at";

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
        view_url: url(l),
        zoom_srcset: srcset(&[t, m, l, x]),
        zoom_width,
        zoom_height,
        label_date: taken_at.as_deref().map(format_date_time).unwrap_or_default(),
        label_gear: gear(make.as_deref(), model.as_deref(), lens.as_deref()),
        label_exposure: exposure.join(", "),
    })
}

/// "Fujifilm X-T3, XF56mmF1.2 R". Makers write their name in capitals and
/// sometimes repeat it in the model ("Canon EOS R6").
fn gear(make: Option<&str>, model: Option<&str>, lens: Option<&str>) -> String {
    let make = make.map(|m| match m.to_ascii_uppercase().as_str() {
        "FUJIFILM" => "Fujifilm".to_string(),
        "CANON" => "Canon".to_string(),
        "NIKON CORPORATION" => "Nikon".to_string(),
        "SONY" => "Sony".to_string(),
        _ => m.to_string(),
    });
    let body = match (make, model) {
        (Some(make), Some(model)) if model.to_lowercase().starts_with(&make.to_lowercase()) => {
            model.to_string()
        }
        (Some(make), Some(model)) => format!("{make} {model}"),
        (None, Some(model)) => model.to_string(),
        (Some(make), None) => make,
        (None, None) => String::new(),
    };
    match lens {
        Some(lens) if !body.is_empty() => format!("{body}, {lens}"),
        Some(lens) => lens.to_string(),
        None => body,
    }
}

/// "760 photographs, March – April 2024"
fn summary(count: i64, first: Option<&str>, last: Option<&str>) -> String {
    let noun = if count == 1 { "photograph" } else { "photographs" };
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
    Some(if first.format("%Y-%m").to_string() == last.format("%Y-%m").to_string() {
        format!("{fm} {fy}")
    } else if fy.to_string() == ly.to_string() {
        format!("{fm} – {lm} {ly}")
    } else {
        format!("{fm} {fy} – {lm} {ly}")
    })
}

/// "12 March 2024, 14:32"
fn format_date_time(value: &str) -> String {
    parse(value)
        .map(|dt| dt.format("%-d %B %Y, %H:%M").to_string())
        .unwrap_or_default()
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
        assert_eq!(gear(Some("FUJIFILM"), Some("X-T3"), Some("XF56mmF1.2 R")), "Fujifilm X-T3, XF56mmF1.2 R");
        assert_eq!(gear(Some("Canon"), Some("Canon EOS R6"), None), "Canon EOS R6");
        assert_eq!(gear(None, None, None), "");
    }

    #[test]
    fn date_spans() {
        let d = |s: &str| Some(s.to_string());
        assert_eq!(date_span(d("2024-03-02T10:00:00").as_deref(), d("2024-03-20T10:00:00").as_deref()).unwrap(), "March 2024");
        assert_eq!(date_span(d("2024-03-02T10:00:00").as_deref(), d("2024-04-01T10:00:00").as_deref()).unwrap(), "March – April 2024");
        assert_eq!(date_span(d("2023-12-30T10:00:00").as_deref(), d("2024-01-02T10:00:00").as_deref()).unwrap(), "December 2023 – January 2024");
        assert_eq!(summary(1, None, None), "1 photograph");
    }
}
