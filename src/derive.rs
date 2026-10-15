//! Image derivatives (libvips). Every variant is generated at scan time and
//! stored on the cache disk, so browsing never wakes the library's HDD.
//! See docs/cache.md.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use libvips::ops;
use libvips::{VipsApp, VipsImage};

// libvips compatibility: the Rust bindings are generated for libvips 8.17+
// and their `*_with_opts` functions send every optional argument by name,
// including ones renamed since 8.16 (Debian trixie, our Docker image), which
// then rejects the whole call. So we only use calls without optional
// arguments, and pass save options in libvips' filename syntax
// (`file.webp[Q=80,...]`), which sends just the options we write.

/// Bump when sizes, quality or processing change: everything is regenerated.
pub const PIPELINE_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    pub name: &'static str,
    pub long_edge: u32,
    quality: i32,
}

/// Largest first: each variant is made from the previous one.
pub const VARIANTS: [Variant; 4] = [
    Variant {
        name: "x",
        long_edge: 4096,
        quality: 82,
    },
    Variant {
        name: "l",
        long_edge: 2048,
        quality: 80,
    },
    Variant {
        name: "m",
        long_edge: 960,
        quality: 78,
    },
    Variant {
        name: "t",
        long_edge: 480,
        quality: 75,
    },
];

pub fn variant(name: &str) -> Option<Variant> {
    VARIANTS.iter().copied().find(|v| v.name == name)
}

/// Size of a variant for an image with the given display dimensions
/// (fit inside the long edge, never upscale).
pub fn fit(width: u32, height: u32, variant: Variant) -> (u32, u32) {
    let long = width.max(height);
    if long <= variant.long_edge {
        return (width, height);
    }
    let scale = variant.long_edge as f64 / long as f64;
    let w = ((width as f64 * scale).round() as u32).max(1);
    let h = ((height as f64 * scale).round() as u32).max(1);
    (w, h)
}

/// Cheap content identity: BLAKE3 over the size and the first and last
/// 64 KiB. Renamed or moved files keep their key, hence their derivatives.
pub fn content_key(path: &Path, size: u64) -> Result<String> {
    const CHUNK: u64 = 64 * 1024;
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());

    let mut buf = vec![0; CHUNK as usize];
    let n = read_up_to(&mut file, &mut buf)?;
    hasher.update(&buf[..n]);
    if size > CHUNK * 2 {
        file.seek(SeekFrom::End(-(CHUNK as i64)))?;
        let n = read_up_to(&mut file, &mut buf)?;
        hasher.update(&buf[..n]);
    } else if size > CHUNK {
        file.seek(SeekFrom::Start(CHUNK))?;
        let n = read_up_to(&mut file, &mut buf)?;
        hasher.update(&buf[..n]);
    }
    // 128 bits is plenty for a personal library and keeps URLs short.
    Ok(hasher.finalize().to_hex()[..32].to_string())
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match file.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
}

/// `<cache>/d/ab/cd/<key>-<variant>.v<N>.webp`
pub fn file_name(key: &str, variant: Variant) -> String {
    format!("{key}-{}.v{PIPELINE_VERSION}.webp", variant.name)
}

pub fn path(cache: &Path, key: &str, variant: Variant) -> PathBuf {
    cache
        .join("d")
        .join(&key[0..2])
        .join(&key[2..4])
        .join(file_name(key, variant))
}

pub struct Derived {
    /// Display dimensions of the original (orientation applied).
    pub width: u32,
    pub height: u32,
    pub thumbhash: Vec<u8>,
    pub avg_rgb: u32,
}

/// Produces all variants for `source` unless they already exist for `key`
/// (e.g. after a rename) and `force` is false. Returns display dims and
/// placeholder data.
pub fn generate(source: &Path, cache: &Path, key: &str, force: bool) -> Result<Derived> {
    let (width, height) = display_dims(source)?;

    let smallest = VARIANTS[VARIANTS.len() - 1];
    let all_present = !force && VARIANTS.iter().all(|v| path(cache, key, *v).exists());
    let smallest_image = if all_present {
        VipsImage::new_from_file(&utf8(&path(cache, key, smallest))?)
            .map_err(|e| vips_error("loading cached thumbnail", e))?
    } else {
        write_variants(source, cache, key, width.max(height))?
    };

    let (thumbhash, avg_rgb) = placeholder(&smallest_image)?;
    Ok(Derived {
        width,
        height,
        thumbhash,
        avg_rgb,
    })
}

/// `long_edge` is the original's long edge: variants never upscale.
fn write_variants(source: &Path, cache: &Path, key: &str, long_edge: u32) -> Result<VipsImage> {
    // `thumbnail` fits the image in a `size` × `size` box: the long edge of
    // the result is `size`.
    let target = |variant: Variant| variant.long_edge.min(long_edge) as i32;

    // Shrink-on-load and autorotate in one pass, then keep the result in
    // memory so the smaller sizes don't decode again.
    let largest = VARIANTS[0];
    let mut image =
        ops::thumbnail(&utf8(source)?, target(largest)).map_err(|e| vips_error("decoding", e))?;
    image = VipsImage::image_copy_memory(image).map_err(|e| vips_error("buffering", e))?;
    save(&image, cache, key, largest)?;

    for variant in &VARIANTS[1..] {
        let resized = ops::thumbnail_image(&image, target(*variant))
            .map_err(|e| vips_error("resizing", e))?;
        image = VipsImage::image_copy_memory(resized).map_err(|e| vips_error("buffering", e))?;
        save(&image, cache, key, *variant)?;
    }
    Ok(image)
}

fn save(image: &VipsImage, cache: &Path, key: &str, variant: Variant) -> Result<()> {
    let target = path(cache, key, variant);
    let dir = target.parent().expect("derivative path has a parent");
    std::fs::create_dir_all(dir)?;
    // Write next to the target and rename, so a crash never leaves a
    // truncated file under the final (immutable, long-cached) name.
    let tmp = dir.join(format!(".{}.tmp.webp", file_name(key, variant)));
    let smart_subsample = if variant.quality >= 80 {
        ",smart_subsample=true"
    } else {
        ""
    };
    let options = format!(
        "[Q={},effort=4,keep=none{smart_subsample}]",
        variant.quality
    );
    image
        .image_write_to_file(&format!("{}{options}", utf8(&tmp)?))
        .map_err(|e| vips_error("encoding WebP", e))?;
    std::fs::rename(&tmp, &target)?;
    Ok(())
}

fn display_dims(source: &Path) -> Result<(u32, u32)> {
    let header =
        VipsImage::new_from_file(&utf8(source)?).map_err(|e| vips_error("reading header", e))?;
    let (w, h) = (header.get_width() as u32, header.get_height() as u32);
    // EXIF orientations 5–8 rotate by 90°.
    Ok(if header.get_orientation() >= 5 {
        (h, w)
    } else {
        (w, h)
    })
}

fn placeholder(image: &VipsImage) -> Result<(Vec<u8>, u32)> {
    // ThumbHash wants at most 100×100.
    let tiny = ops::thumbnail_image(image, 100).map_err(|e| vips_error("placeholder", e))?;
    let (w, h) = (tiny.get_width() as usize, tiny.get_height() as usize);
    let bands = tiny.get_bands() as usize;
    let pixels = tiny.image_write_to_memory();
    if pixels.len() != w * h * bands || !(1..=4).contains(&bands) {
        bail!("unexpected placeholder layout: {w}x{h}x{bands}");
    }

    let rgba: Vec<u8> = pixels
        .chunks_exact(bands)
        .flat_map(|px| match bands {
            1 => [px[0], px[0], px[0], 255],
            2 => [px[0], px[0], px[0], px[1]],
            3 => [px[0], px[1], px[2], 255],
            _ => [px[0], px[1], px[2], px[3]],
        })
        .collect();
    let hash = thumbhash::rgba_to_thumb_hash(w, h, &rgba);
    let (r, g, b, _) =
        thumbhash::thumb_hash_to_average_rgba(&hash).map_err(|()| anyhow!("thumbhash average"))?;
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
    Ok((hash, channel(r) << 16 | channel(g) << 8 | channel(b)))
}

fn utf8(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .with_context(|| format!("non UTF-8 path: {}", path.display()))
}

/// The bindings' errors only say which operation failed; libvips' own
/// message (e.g. "unknown option") is in its error buffer.
fn vips_error(what: &str, err: libvips::error::Error) -> anyhow::Error {
    let app = VipsApp;
    let detail = app
        .error_buffer()
        .unwrap_or_default()
        .trim()
        .replace('\n', "; ");
    app.error_clear();
    if detail.is_empty() {
        anyhow!("libvips {what}: {err}")
    } else {
        anyhow!("libvips {what}: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_never_upscales() {
        let l = variant("l").unwrap();
        assert_eq!(fit(6240, 4160, l), (2048, 1365));
        assert_eq!(fit(4160, 6240, l), (1365, 2048));
        assert_eq!(fit(1000, 800, l), (1000, 800));
    }

    #[test]
    fn content_key_depends_on_head_and_tail() {
        let dir = std::env::temp_dir().join(format!("pellicule-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let mut data = vec![7u8; 300 * 1024];
        std::fs::write(&a, &data).unwrap();
        let key_a = content_key(&a, data.len() as u64).unwrap();

        let b = dir.join("renamed.bin");
        std::fs::rename(&a, &b).unwrap();
        assert_eq!(content_key(&b, data.len() as u64).unwrap(), key_a);

        *data.last_mut().unwrap() = 8;
        std::fs::write(&b, &data).unwrap();
        assert_ne!(content_key(&b, data.len() as u64).unwrap(), key_a);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
