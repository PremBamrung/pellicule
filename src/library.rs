//! Library conventions: what is media, what is ignored, what is an album.
//! See docs/conventions.md.

use std::path::Path;

use anyhow::Result;
use walkdir::WalkDir;

/// A directory containing one of these files is hidden along with its children.
const IGNORE_MARKERS: &[&str] = &[".pellicule-ignore", ".nomedia"];

const JUNK_NAMES: &[&str] = &["@eaDir", "#recycle", "Thumbs.db", "desktop.ini"];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "heic", "heif", "tif", "tiff",
];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}

pub fn media_kind(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Image)
    } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

/// Dotfiles, `_drafts`-style names and NAS junk are never part of the gallery.
pub fn is_ignored_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('_') || JUNK_NAMES.contains(&name)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanSummary {
    pub albums: usize,
    pub images: usize,
    pub videos: usize,
}

/// Walks the library and counts what it would index.
/// Placeholder for M1: no database, no derivatives yet.
pub fn scan(root: &Path) -> Result<ScanSummary> {
    let mut summary = ScanSummary::default();

    let walker = WalkDir::new(root).follow_links(false).into_iter();
    let entries = walker.filter_entry(|entry| {
        if entry.depth() == 0 {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        if is_ignored_name(&name) {
            return false;
        }
        !(entry.file_type().is_dir()
            && IGNORE_MARKERS
                .iter()
                .any(|marker| entry.path().join(marker).exists()))
    });

    for entry in entries {
        let entry = entry?;
        if entry.file_type().is_dir() {
            let has_media = std::fs::read_dir(entry.path())?.flatten().any(|child| {
                !is_ignored_name(&child.file_name().to_string_lossy())
                    && media_kind(&child.path()).is_some()
            });
            if has_media {
                summary.albums += 1;
            }
            continue;
        }
        match media_kind(entry.path()) {
            Some(MediaKind::Image) => summary.images += 1,
            Some(MediaKind::Video) => summary.videos += 1,
            None => {}
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_media_case_insensitively() {
        assert_eq!(
            media_kind(Path::new("DSCF1234.JPG")),
            Some(MediaKind::Image)
        );
        assert_eq!(media_kind(Path::new("clip.MOV")), Some(MediaKind::Video));
        assert_eq!(media_kind(Path::new("DSCF1234.RAF")), None);
        assert_eq!(media_kind(Path::new("index.md")), None);
    }

    #[test]
    fn ignores_hidden_drafts_and_junk() {
        assert!(is_ignored_name(".DS_Store"));
        assert!(is_ignored_name("_drafts"));
        assert!(is_ignored_name("@eaDir"));
        assert!(!is_ignored_name("2025-02 Hokkaido"));
    }
}
