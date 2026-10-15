//! Walks the library, indexes what changed and generates derivatives.
//! Unchanged files (same size and mtime) are skipped without being read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};
use walkdir::WalkDir;

use crate::config::Config;
use crate::derive::{self, Derived, PIPELINE_VERSION};
use crate::library::{self, MediaKind};
use crate::meta::{self, Exif};

#[derive(Debug, Default)]
pub struct ScanSummary {
    pub albums: usize,
    pub images: usize,
    pub processed: usize,
    pub failed: usize,
    pub skipped_videos: usize,
    pub missing: usize,
}

struct Job {
    album_id: i64,
    file_name: String,
    path: PathBuf,
    size: u64,
    mtime_ns: i64,
}

struct Processed {
    key: String,
    exif: Exif,
    derived: Derived,
}

pub fn run(config: &Config, conn: &mut Connection) -> Result<ScanSummary> {
    let root = &config.library.root;
    ensure_mounted(root)?;

    let started = Instant::now();
    let generation: i64 =
        conn.query_row("SELECT COALESCE(MAX(seen_gen), 0) + 1 FROM album", [], |r| r.get(0))?;
    let mut summary = ScanSummary::default();

    let jobs = walk(root, conn, generation, &mut summary)?;
    tracing::info!(
        albums = summary.albums,
        images = summary.images,
        to_process = jobs.len(),
        "library walked"
    );

    process(config, conn, generation, jobs, &mut summary)?;

    let now = unix_now();
    summary.missing = conn.execute(
        "UPDATE media SET missing_since = ?1 WHERE seen_gen < ?2 AND missing_since IS NULL",
        params![now, generation],
    )?;
    conn.execute(
        "UPDATE album SET missing_since = ?1 WHERE seen_gen < ?2 AND missing_since IS NULL",
        params![now, generation],
    )?;

    tracing::info!(
        processed = summary.processed,
        failed = summary.failed,
        missing = summary.missing,
        skipped_videos = summary.skipped_videos,
        elapsed = ?started.elapsed(),
        "scan complete"
    );
    Ok(summary)
}

/// A NAS mount that is down looks like an empty directory. Treating that as
/// "everything was deleted" would wipe covers and share links, so refuse.
fn ensure_mounted(root: &Path) -> Result<()> {
    let mut entries = std::fs::read_dir(root)
        .with_context(|| format!("library root {} is not readable", root.display()))?;
    if entries.next().is_none() {
        bail!(
            "library root {} is empty (not mounted?); refusing to scan",
            root.display()
        );
    }
    Ok(())
}

fn walk(
    root: &Path,
    conn: &mut Connection,
    generation: i64,
    summary: &mut ScanSummary,
) -> Result<Vec<Job>> {
    let tx = conn.transaction()?;
    let mut album_ids: HashMap<String, i64> = HashMap::new();
    let mut jobs = Vec::new();

    let walker = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !library::is_ignored(entry));

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                tracing::warn!(%err, "skipping unreadable entry");
                continue;
            }
        };
        let Some(rel_path) = relative(root, entry.path()) else {
            tracing::warn!(path = %entry.path().display(), "skipping non UTF-8 path");
            continue;
        };

        if entry.file_type().is_dir() {
            let (parent_id, name) = match rel_path.rsplit_once('/') {
                Some((parent, name)) => (album_ids.get(parent).copied(), name.to_string()),
                None if rel_path.is_empty() => (None, String::new()),
                None => (album_ids.get("").copied(), rel_path.clone()),
            };
            let id: i64 = tx.query_row(
                "INSERT INTO album (rel_path, parent_id, name, seen_gen) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (rel_path) DO UPDATE SET
                    parent_id = excluded.parent_id, name = excluded.name,
                    seen_gen = excluded.seen_gen, missing_since = NULL
                 RETURNING id",
                params![rel_path, parent_id, name, generation],
                |r| r.get(0),
            )?;
            album_ids.insert(rel_path, id);
            summary.albums += 1;
            continue;
        }

        match library::media_kind(entry.path()) {
            Some(MediaKind::Image) => {}
            Some(MediaKind::Video) => {
                summary.skipped_videos += 1;
                continue;
            }
            None => continue,
        }
        summary.images += 1;

        let parent_rel = rel_path.rsplit_once('/').map_or("", |(parent, _)| parent);
        let Some(&album_id) = album_ids.get(parent_rel) else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let metadata = entry.metadata()?;
        let size = metadata.len();
        let mtime_ns = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);

        let unchanged = tx
            .query_row(
                "SELECT 1 FROM media
                 WHERE album_id = ?1 AND file_name = ?2 AND size = ?3 AND mtime_ns = ?4
                   AND derived_version = ?5 AND error IS NULL",
                params![album_id, file_name, size as i64, mtime_ns, PIPELINE_VERSION],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if unchanged {
            tx.execute(
                "UPDATE media SET seen_gen = ?1, missing_since = NULL
                 WHERE album_id = ?2 AND file_name = ?3",
                params![generation, album_id, file_name],
            )?;
        } else {
            jobs.push(Job {
                album_id,
                file_name,
                path: entry.path().to_path_buf(),
                size,
                mtime_ns,
            });
        }
    }

    tx.commit()?;
    Ok(jobs)
}

fn process(
    config: &Config,
    conn: &mut Connection,
    generation: i64,
    jobs: Vec<Job>,
    summary: &mut ScanSummary,
) -> Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }
    let total = jobs.len();
    let cache = config.library.cache.clone();
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get().saturating_sub(1))
        .max(1);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|i| format!("derive-{i}"))
        .build()?;

    let (sender, receiver) = mpsc::channel::<(Job, Result<Processed>)>();

    std::thread::scope(|scope| -> Result<()> {
        scope.spawn(move || {
            pool.install(|| {
                jobs.into_par_iter().for_each_with(sender, |sender, job| {
                    let result = process_one(&job, &cache);
                    let _ = sender.send((job, result));
                });
            });
        });

        let mut last_log = Instant::now();
        let mut tx = conn.transaction()?;
        let mut pending = 0;
        for (job, result) in receiver {
            match result {
                Ok(processed) => {
                    upsert_media(&tx, generation, &job, Some(&processed), None)?;
                    summary.processed += 1;
                }
                Err(err) => {
                    tracing::warn!(path = %job.path.display(), "{err:#}");
                    upsert_media(&tx, generation, &job, None, Some(&format!("{err:#}")))?;
                    summary.failed += 1;
                }
            }
            pending += 1;
            if pending >= 25 {
                tx.commit()?;
                tx = conn.transaction()?;
                pending = 0;
            }
            if last_log.elapsed() > Duration::from_secs(10) {
                let done = summary.processed + summary.failed;
                tracing::info!("processed {done}/{total}");
                last_log = Instant::now();
            }
        }
        tx.commit()?;
        Ok(())
    })
}

fn process_one(job: &Job, cache: &Path) -> Result<Processed> {
    let key = derive::content_key(&job.path, job.size)?;
    let exif = meta::read_exif(&job.path);
    let derived = derive::generate(&job.path, cache, &key)?;
    Ok(Processed { key, exif, derived })
}

fn upsert_media(
    conn: &Connection,
    generation: i64,
    job: &Job,
    processed: Option<&Processed>,
    error: Option<&str>,
) -> Result<()> {
    let exif = processed.map(|p| &p.exif);
    let derived = processed.map(|p| &p.derived);
    conn.execute(
        "INSERT INTO media (
            album_id, file_name, kind, size, mtime_ns, content_key, width, height,
            thumbhash, avg_rgb, make, model, lens, focal_mm, focal_35mm, aperture,
            exposure_s, iso, exposure_bias, taken_at, tz_offset_min,
            derived_version, error, seen_gen, missing_since)
         VALUES (?1, ?2, 'image', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, NULL)
         ON CONFLICT (album_id, file_name) DO UPDATE SET
            size = excluded.size, mtime_ns = excluded.mtime_ns,
            content_key = excluded.content_key, width = excluded.width,
            height = excluded.height, thumbhash = excluded.thumbhash,
            avg_rgb = excluded.avg_rgb, make = excluded.make, model = excluded.model,
            lens = excluded.lens, focal_mm = excluded.focal_mm,
            focal_35mm = excluded.focal_35mm, aperture = excluded.aperture,
            exposure_s = excluded.exposure_s, iso = excluded.iso,
            exposure_bias = excluded.exposure_bias, taken_at = excluded.taken_at,
            tz_offset_min = excluded.tz_offset_min,
            derived_version = excluded.derived_version, error = excluded.error,
            seen_gen = excluded.seen_gen, missing_since = NULL",
        params![
            job.album_id,
            job.file_name,
            job.size as i64,
            job.mtime_ns,
            processed.map(|p| &p.key),
            derived.map(|d| d.width),
            derived.map(|d| d.height),
            derived.map(|d| &d.thumbhash),
            derived.map(|d| d.avg_rgb),
            exif.and_then(|e| e.make.as_deref()),
            exif.and_then(|e| e.model.as_deref()),
            exif.and_then(|e| e.lens.as_deref()),
            exif.and_then(|e| e.focal_mm),
            exif.and_then(|e| e.focal_35mm),
            exif.and_then(|e| e.aperture),
            exif.and_then(|e| e.exposure_s),
            exif.and_then(|e| e.iso),
            exif.and_then(|e| e.exposure_bias),
            exif.and_then(|e| e.taken_at.as_deref()),
            exif.and_then(|e| e.tz_offset_min),
            processed.map(|_| PIPELINE_VERSION),
            error,
            generation,
        ],
    )?;
    Ok(())
}

/// Library-relative path with `/` separators; `""` for the root itself.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/"))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_use_slashes() {
        let root = Path::new("/photos");
        assert_eq!(relative(root, Path::new("/photos")).as_deref(), Some(""));
        assert_eq!(
            relative(root, Path::new("/photos/Japon/day 2")).as_deref(),
            Some("Japon/day 2")
        );
    }

    #[test]
    fn refuses_empty_root() {
        let dir = std::env::temp_dir().join(format!("pellicule-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ensure_mounted(&dir).is_err());
        std::fs::remove_dir(&dir).unwrap();
    }
}
