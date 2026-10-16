//! SQLite index, stored in the cache directory. The library itself is never
//! written to; deleting the database only costs a rescan.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

const DB_FILE: &str = "pellicule.db";

/// Each entry upgrades the schema by one version (tracked in `user_version`).
const MIGRATIONS: &[&str] = &[
    r#"
    CREATE TABLE album (
        id            INTEGER PRIMARY KEY,
        rel_path      TEXT NOT NULL UNIQUE,      -- '' is the library root
        parent_id     INTEGER REFERENCES album(id),
        name          TEXT NOT NULL,
        seen_gen      INTEGER NOT NULL,
        missing_since INTEGER
    );
    CREATE INDEX album_parent ON album(parent_id);

    CREATE TABLE media (
        id              INTEGER PRIMARY KEY,
        album_id        INTEGER NOT NULL REFERENCES album(id),
        file_name       TEXT NOT NULL,
        kind            TEXT NOT NULL,            -- image | video
        size            INTEGER NOT NULL,
        mtime_ns        INTEGER NOT NULL,
        content_key     TEXT,                     -- derivative file stem
        width           INTEGER,                  -- display dims (orientation applied)
        height          INTEGER,
        thumbhash       BLOB,
        avg_rgb         INTEGER,
        make            TEXT,
        model           TEXT,
        lens            TEXT,
        focal_mm        REAL,
        focal_35mm      INTEGER,
        aperture        REAL,
        exposure_s      REAL,
        iso             INTEGER,
        exposure_bias   REAL,
        taken_at        TEXT,                     -- capture-local wall time, ISO 8601
        tz_offset_min   INTEGER,
        derived_version INTEGER,                  -- pipeline version of the cached variants
        error           TEXT,
        seen_gen        INTEGER NOT NULL,
        missing_since   INTEGER,
        UNIQUE (album_id, file_name)
    );
    CREATE INDEX media_album_taken ON media(album_id, taken_at);
"#,
    r#"
    CREATE TABLE scan_run (
        id          INTEGER PRIMARY KEY,
        trigger     TEXT NOT NULL,                -- startup | manual | cli
        rebuild     INTEGER NOT NULL,
        started_at  INTEGER NOT NULL,
        finished_at INTEGER,
        status      TEXT NOT NULL,                -- running | done | failed | cancelled | interrupted
        images      INTEGER,
        processed   INTEGER,
        failed      INTEGER,
        missing     INTEGER,
        error       TEXT
    );
"#,
    r#"
    -- Average processing time per photo and per worker (hash, EXIF,
    -- previews): comparable across machines and worker counts.
    ALTER TABLE scan_run ADD COLUMN photo_ms INTEGER;
"#,
];

pub fn open(cache_dir: &Path) -> Result<Connection> {
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("creating cache dir {}", cache_dir.display()))?;
    let path = cache_dir.join(DB_FILE);
    let mut conn =
        Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("applying migration {}", index + 1))?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    migrate(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once() {
        let mut conn = open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }
}
