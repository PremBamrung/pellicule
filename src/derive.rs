//! Image derivatives (libvips). Every variant is generated at scan time and
//! stored on the cache disk, so browsing never wakes the library's HDD.
//! See docs/cache.md.

use std::ffi::CStr;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use libvips::{VipsImage, bindings, ops};

// libvips compatibility: the Rust bindings are generated for libvips 8.17+
// and their `*_with_opts` functions send every optional argument by name,
// including ones renamed since 8.16 (Debian trixie, our Docker image), which
// then rejects the whole call. So we only use calls without optional
// arguments, and pass save options in libvips' filename syntax
// (`file.webp[Q=80,...]`), which sends just the options we write.

/// Bump when sizes, quality or processing change: everything is regenerated.
/// v3: the zoom size as JPEG, WebP at effort 2 (about 3× faster scans).
pub const PIPELINE_VERSION: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Webp,
    Jpeg,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Webp => "webp",
            Format::Jpeg => "jpg",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Format::Webp => "image/webp",
            Format::Jpeg => "image/jpeg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant {
    pub name: &'static str,
    pub long_edge: u32,
    pub format: Format,
    quality: i32,
}

/// Largest first: each variant is made from the previous one.
pub const VARIANTS: [Variant; 4] = [
    // Only fetched when zooming, on the private side: a JPEG encodes about
    // 9× faster than a WebP this size, for files ~25% larger.
    Variant {
        name: "x",
        long_edge: 4096,
        format: Format::Jpeg,
        quality: 85,
    },
    Variant {
        name: "l",
        long_edge: 2048,
        format: Format::Webp,
        quality: 80,
    },
    Variant {
        name: "m",
        long_edge: 960,
        format: Format::Webp,
        quality: 78,
    },
    Variant {
        name: "t",
        long_edge: 480,
        format: Format::Webp,
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

/// Content identity: BLAKE3 of the whole file. Renamed or moved files keep
/// their key, hence their derivatives. Only new or changed files are hashed,
/// and those are read in full for decoding anyway.
pub fn content_key(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    // 128 bits is plenty for a personal library and keeps URLs short.
    Ok(hasher.finalize().to_hex()[..32].to_string())
}

/// `<cache>/d/ab/cd/<key>-<variant>.v<N>.<webp|jpg>`
pub fn file_name(key: &str, variant: Variant) -> String {
    format!(
        "{key}-{}.v{PIPELINE_VERSION}.{}",
        variant.name,
        variant.format.extension()
    )
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

    // Shrink-on-load and autorotate in one pass, convert to sRGB, then keep
    // the result in memory so the smaller sizes don't decode again.
    let largest = VARIANTS[0];
    let image =
        ops::thumbnail(&utf8(source)?, target(largest)).map_err(|e| vips_error("decoding", e))?;
    let mut image =
        VipsImage::image_copy_memory(to_srgb(image)).map_err(|e| vips_error("buffering", e))?;
    save(&image, cache, key, largest)?;

    for variant in &VARIANTS[1..] {
        let resized = ops::thumbnail_image(&image, target(*variant))
            .map_err(|e| vips_error("resizing", e))?;
        image = VipsImage::image_copy_memory(resized).map_err(|e| vips_error("buffering", e))?;
        save(&image, cache, key, *variant)?;
    }
    Ok(image)
}

/// Derivatives are sRGB with no profile (it's stripped on save), so an image
/// with an embedded profile (Adobe RGB, Display-P3…) has its pixels converted.
/// `icc_transform` fails when there is no embedded profile: the image is then
/// already taken as sRGB and kept as is. (The bindings can't check for the
/// profile first.)
fn to_srgb(image: VipsImage) -> VipsImage {
    match ops::icc_transform(&image, "srgb") {
        Ok(converted) => converted,
        Err(_) => {
            take_vips_error();
            image
        }
    }
}

fn save(image: &VipsImage, cache: &Path, key: &str, variant: Variant) -> Result<()> {
    let target = path(cache, key, variant);
    let dir = target.parent().expect("derivative path has a parent");
    std::fs::create_dir_all(dir)?;
    // Write next to the target and rename, so a crash never leaves a
    // truncated file under the final (immutable, long-cached) name.
    // libvips picks the encoder from the extension.
    let extension = variant.format.extension();
    let tmp = dir.join(format!(".{}.tmp.{extension}", file_name(key, variant)));
    // Effort 2 is about twice as fast as 4 for 2.5% larger files;
    // smart_subsample made no difference on camera JPEGs.
    let options = match variant.format {
        Format::Webp => format!("[Q={},effort=2,keep=none]", variant.quality),
        // No alpha in JPEG: transparent areas go white, like the wall.
        Format::Jpeg => format!(
            "[Q={},optimize_coding=true,keep=none,background=255]",
            variant.quality
        ),
    };
    image
        .image_write_to_file(&format!("{}{options}", utf8(&tmp)?))
        .map_err(|e| vips_error(&format!("encoding {extension}"), e))?;
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
    // ThumbHash wants at most 100×100; `thumbnail_image` would upscale.
    let size = image.get_width().max(image.get_height()).min(100);
    let tiny = ops::thumbnail_image(image, size).map_err(|e| vips_error("placeholder", e))?;
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
    let detail = take_vips_error();
    if detail.is_empty() {
        anyhow!("libvips {what}: {err}")
    } else {
        anyhow!("libvips {what}: {detail}")
    }
}

/// Reads and clears libvips' error buffer. Not through `VipsApp`: dropping
/// one, even a temporary, calls `vips_shutdown`.
fn take_vips_error() -> String {
    // SAFETY: libvips always returns a valid NUL-terminated string; it's
    // copied before the buffer is cleared.
    unsafe {
        let detail = CStr::from_ptr(bindings::vips_error_buffer())
            .to_string_lossy()
            .trim()
            .replace('\n', "; ");
        bindings::vips_error_clear();
        detail
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

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pellicule-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn content_key_covers_the_whole_file() {
        let dir = temp_dir("key");
        let a = dir.join("a.bin");
        let mut data = vec![7u8; 300 * 1024];
        std::fs::write(&a, &data).unwrap();
        let key_a = content_key(&a).unwrap();

        let b = dir.join("renamed.bin");
        std::fs::rename(&a, &b).unwrap();
        assert_eq!(content_key(&b).unwrap(), key_a);

        // Same size, same head and tail: only the middle differs.
        data[150 * 1024] = 8;
        std::fs::write(&b, &data).unwrap();
        assert_ne!(content_key(&b).unwrap(), key_a);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Generates every variant of a fixture (tests/fixtures, made by
    /// scripts/make-fixtures.sh) and checks what all of them share.
    fn derive_fixture(name: &str) -> (Derived, Vec<(Variant, VipsImage)>) {
        // Once, and never dropped: `vips_init` isn't thread-safe and dropping
        // a `VipsApp` shuts libvips down for the whole process.
        static VIPS: std::sync::Once = std::sync::Once::new();
        VIPS.call_once(|| {
            std::mem::forget(libvips::VipsApp::new("pellicule-test", false).unwrap())
        });
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let cache = temp_dir(name);
        let key = content_key(&source).unwrap();
        let derived = generate(&source, &cache, &key, true).unwrap();

        let variants = VARIANTS
            .iter()
            .map(|v| {
                let file = path(&cache, &key, *v);
                let bytes = std::fs::read(&file).unwrap();
                // The right format, and no colour profile, EXIF or XMP survives.
                match v.format {
                    Format::Webp => assert_eq!(&bytes[8..12], b"WEBP", "{name} {}", v.name),
                    Format::Jpeg => assert_eq!(&bytes[..3], b"\xff\xd8\xff", "{name} {}", v.name),
                }
                let markers: [&[u8]; 6] = [
                    b"ICCP",
                    b"EXIF",
                    b"XMP ",
                    b"ICC_PROFILE",
                    b"Exif\0",
                    b"ns.adobe.com/xap",
                ];
                for marker in markers {
                    assert!(
                        !bytes.windows(marker.len()).any(|w| w == marker),
                        "{name} {}",
                        v.name
                    );
                }
                let image = VipsImage::new_from_buffer(&bytes, "").unwrap();
                let image = VipsImage::image_copy_memory(image).unwrap();
                let expected = fit(derived.width, derived.height, *v);
                let actual = (image.get_width() as u32, image.get_height() as u32);
                assert_eq!(actual, expected, "{name} {}", v.name);
                (*v, image)
            })
            .collect();
        std::fs::remove_dir_all(&cache).unwrap();
        (derived, variants)
    }

    fn pixel(image: &VipsImage, x: usize, y: usize) -> Vec<f64> {
        let (w, bands) = (image.get_width() as usize, image.get_bands() as usize);
        let start = (y * w + x) * bands;
        let bytes = image.image_write_to_memory();
        bytes[start..start + bands]
            .iter()
            .map(|&b| b as f64)
            .collect()
    }

    #[test]
    fn wide_gamut_is_converted_to_srgb() {
        let (derived, variants) = derive_fixture("p3.jpg");
        assert_eq!((derived.width, derived.height), (1200, 800));
        // Red in sRGB is (220, 30, 30); left in P3 it would be ~(213, 72, 62).
        for (v, image) in &variants {
            let px = pixel(image, 10, 10);
            assert!(
                px[0] > 205.0 && px[1] < 50.0 && px[2] < 50.0,
                "{}: {px:?}",
                v.name
            );
        }
    }

    #[test]
    fn orientation_is_applied() {
        let (derived, variants) = derive_fixture("rotated.jpg");
        // Stored 64×32 red | blue, orientation 6: displayed 32×64, red on top.
        assert_eq!((derived.width, derived.height), (32, 64));
        for (v, image) in &variants {
            let (top, bottom) = (pixel(image, 16, 8), pixel(image, 16, 56));
            assert!(top[0] > 150.0 && top[2] < 100.0, "{} top: {top:?}", v.name);
            assert!(
                bottom[2] > 150.0 && bottom[0] < 100.0,
                "{} bottom: {bottom:?}",
                v.name
            );
        }
    }

    #[test]
    fn grayscale_alpha_keeps_its_alpha() {
        let (_, variants) = derive_fixture("gray-alpha.png");
        for (v, image) in &variants {
            let px = pixel(image, 4, 4);
            match v.format {
                Format::Webp => {
                    assert!(image.image_hasalpha(), "{}", v.name);
                    assert!((px[0] - 128.0).abs() < 8.0, "{}: {px:?}", v.name);
                }
                // Grey 128 at alpha 200, flattened onto white: about 155.
                Format::Jpeg => {
                    assert!(!image.image_hasalpha(), "{}", v.name);
                    assert!((px[0] - 155.0).abs() < 8.0, "{}: {px:?}", v.name);
                }
            }
        }
    }

    #[test]
    fn small_images_are_not_upscaled() {
        // The dimension check in `derive_fixture` does the work.
        let (derived, _) = derive_fixture("tiny.jpg");
        assert_eq!((derived.width, derived.height), (40, 30));
    }
}
