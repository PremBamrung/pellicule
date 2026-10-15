//! Standard EXIF extraction and photographer-friendly formatting.
//! See docs/exif.md. The Fujifilm MakerNote recipe comes later (M4).

use std::path::Path;

use nom_exif::{ExifDateTime, ExifTag};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Exif {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub focal_35mm: Option<i64>,
    pub aperture: Option<f64>,
    pub exposure_s: Option<f64>,
    pub iso: Option<i64>,
    pub exposure_bias: Option<f64>,
    /// Capture-local wall time, `YYYY-MM-DDTHH:MM:SS`.
    pub taken_at: Option<String>,
    pub tz_offset_min: Option<i32>,
}

/// Missing or unreadable EXIF is not an error: the photo is still shown.
pub fn read_exif(path: &Path) -> Exif {
    let Ok(exif) = nom_exif::read_exif(path) else {
        return Exif::default();
    };
    let text = |tag| {
        exif.get(tag)
            .and_then(|v| v.as_str())
            .map(|s| s.trim().trim_end_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let float = |tag| exif.get(tag).and_then(|v| v.try_as_float());
    let int = |tag| exif.get(tag).and_then(|v| v.try_as_integer());

    let (taken_at, tz_offset_min) = match exif
        .get(ExifTag::DateTimeOriginal)
        .and_then(|v| v.as_datetime())
    {
        Some(ExifDateTime::Aware(dt)) => (
            Some(dt.naive_local().format("%Y-%m-%dT%H:%M:%S").to_string()),
            Some(dt.offset().local_minus_utc() / 60),
        ),
        Some(ExifDateTime::Naive(dt)) => (Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()), None),
        None => (None, None),
    };

    Exif {
        make: text(ExifTag::Make),
        model: text(ExifTag::Model),
        lens: text(ExifTag::LensModel),
        focal_mm: float(ExifTag::FocalLength).filter(|f| *f > 0.0),
        focal_35mm: int(ExifTag::FocalLengthIn35mmFilm).filter(|f| *f > 0),
        aperture: float(ExifTag::FNumber).filter(|f| *f > 0.0),
        exposure_s: float(ExifTag::ExposureTime).filter(|f| *f > 0.0),
        iso: int(ExifTag::ISOSpeedRatings).filter(|i| *i > 0),
        exposure_bias: float(ExifTag::ExposureBiasValue),
        taken_at,
        tz_offset_min,
    }
}

/// `1/500s` below a second, `2s` / `1.3s` at or above.
pub fn format_shutter(seconds: f64) -> String {
    if seconds >= 1.0 {
        format!("{}s", trim_decimal(seconds, 1))
    } else {
        let denominator = 1.0 / seconds;
        if (denominator - denominator.round()).abs() < 0.05 * denominator {
            format!("1/{}s", denominator.round() as i64)
        } else {
            format!("{}s", trim_decimal(seconds, 2))
        }
    }
}

/// `f/2`, `f/2.8`.
pub fn format_aperture(f_number: f64) -> String {
    format!("f/{}", trim_decimal(f_number, 1))
}

/// `23mm`, or `23mm (35mm eq.)` when the equivalent differs.
pub fn format_focal(focal_mm: f64, focal_35mm: Option<i64>) -> String {
    let focal = trim_decimal(focal_mm, 1);
    match focal_35mm {
        Some(eq) if (eq as f64 - focal_mm).abs() >= 1.0 => format!("{focal}mm ({eq}mm eq.)"),
        _ => format!("{focal}mm"),
    }
}

/// Exposure compensation in thirds: `+⅓ EV`, `−1⅔ EV`. Zero is not shown.
pub fn format_bias(ev: f64) -> Option<String> {
    let thirds = (ev * 3.0).round() as i64;
    if thirds == 0 {
        return None;
    }
    let sign = if thirds > 0 { "+" } else { "−" };
    let whole = thirds.abs() / 3;
    let fraction = match thirds.abs() % 3 {
        1 => "⅓",
        2 => "⅔",
        _ => "",
    };
    let whole = if whole == 0 && !fraction.is_empty() {
        String::new()
    } else {
        whole.to_string()
    };
    Some(format!("{sign}{whole}{fraction} EV"))
}

fn trim_decimal(value: f64, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutter() {
        assert_eq!(format_shutter(1.0 / 500.0), "1/500s");
        assert_eq!(format_shutter(10.0 / 64000.0), "1/6400s");
        assert_eq!(format_shutter(0.3), "0.3s");
        assert_eq!(format_shutter(2.0), "2s");
        assert_eq!(format_shutter(1.3), "1.3s");
    }

    #[test]
    fn aperture_and_focal() {
        assert_eq!(format_aperture(2.0), "f/2");
        assert_eq!(format_aperture(2.8), "f/2.8");
        assert_eq!(format_focal(56.0, Some(84)), "56mm (84mm eq.)");
        assert_eq!(format_focal(35.0, Some(35)), "35mm");
        assert_eq!(format_focal(18.5, None), "18.5mm");
    }

    #[test]
    fn bias_in_thirds() {
        assert_eq!(format_bias(0.0), None);
        assert_eq!(format_bias(1.0 / 3.0).as_deref(), Some("+⅓ EV"));
        assert_eq!(format_bias(-5.0 / 3.0).as_deref(), Some("−1⅔ EV"));
        assert_eq!(format_bias(1.0).as_deref(), Some("+1 EV"));
    }
}
