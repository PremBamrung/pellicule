//! Fujifilm MakerNote: the in-camera "recipe" (film simulation, grain,
//! colour chrome, dynamic range, white balance, tone…). Tag numbers and value
//! meanings come from exiftool's FujiFilm tables (facts only, no code); see
//! docs/exif.md. The input is untrusted: every offset is bounds-checked and
//! nothing here panics.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// "Classic Chrome", "Acros + R"…; `None` when the note doesn't say.
    pub film: Option<String>,
    /// Display rows in order: ("Grain", "Weak · Small"), ("Shadow", "−2")…
    pub settings: Vec<(&'static str, String)>,
}

/// `None` if the bytes are not a Fujifilm MakerNote.
pub fn parse(note: &[u8]) -> Option<Recipe> {
    let tags = read_tags(note)?;
    let int = |tag: u16| tags.get(&tag).copied().flatten().map(|v| v.first);
    let i8_at = |tag: u16| tags.get(&tag).copied().flatten().map(|v| v.first_i8);

    let saturation = int(0x1003);
    let monochrome = saturation.and_then(monochrome_film);
    let film = monochrome
        .map(str::to_string)
        .or_else(|| int(0x1401).map(colour_film));

    let mut settings = Vec::new();
    let mut push = |label, value: Option<String>| {
        if let Some(value) = value {
            settings.push((label, value));
        }
    };

    push(
        "Grain",
        int(0x1047).and_then(off_weak_strong).map(|roughness| {
            match int(0x104C).and_then(grain_size) {
                Some(size) if roughness != "Off" => format!("{roughness} · {size}"),
                _ => roughness.to_string(),
            }
        }),
    );
    push(
        "Colour chrome",
        int(0x1048).and_then(off_weak_strong).map(str::to_string),
    );
    push(
        "Colour chrome blue",
        int(0x104E).and_then(off_weak_strong).map(str::to_string),
    );
    push(
        "Dynamic range",
        dynamic_range(int(0x1402), int(0x1403), int(0x140B)),
    );
    push(
        "White balance",
        white_balance(
            int(0x1002),
            int(0x1005),
            tags.get(&0x100A).copied().flatten(),
        ),
    );
    push("Highlight", int(0x1041).map(tone));
    push("Shadow", int(0x1040).map(tone));
    if monochrome.is_some() {
        let toning: Vec<String> = [("WC", i8_at(0x1049)), ("MG", i8_at(0x104B))]
            .into_iter()
            .filter_map(|(name, value)| value.map(|v| format!("{name} {}", signed(v as i64))))
            .collect();
        push("Toning", (!toning.is_empty()).then(|| toning.join(" · ")));
    } else {
        push("Colour", saturation.and_then(colour).map(signed));
    }
    push("Sharpness", int(0x1001).and_then(sharpness).map(signed));
    push(
        "Noise reduction",
        int(0x100E).and_then(noise_reduction).map(signed),
    );
    push(
        "Clarity",
        int(0x100F).map(|raw| signed((raw as f64 / 1000.0).round() as i64)),
    );
    push(
        "Developed",
        (int(0x1436) == Some(1)).then(|| "In camera, from RAW".to_string()),
    );

    Some(Recipe { film, settings })
}

#[derive(Debug, Clone, Copy)]
struct Value {
    /// First value, read with the declared type.
    first: i64,
    /// First byte as a signed byte (the BW toning tags, whatever their type).
    first_i8: i8,
    /// Second value, for two-value tags (white balance fine tune).
    second: Option<i64>,
}

/// Tag → value. An entry whose data is out of bounds maps to `None`.
fn read_tags(note: &[u8]) -> Option<HashMap<u16, Option<Value>>> {
    if note.get(..8)? != b"FUJIFILM" {
        return None;
    }
    // Always little-endian; offsets count from the start of the note.
    let u16_at = |at: usize| {
        note.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let u32_at = |at: usize| {
        note.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let ifd = u32_at(8)? as usize;
    let count = (u16_at(ifd)? as usize).min(512);

    let mut tags = HashMap::new();
    for i in 0..count {
        let entry = ifd + 2 + 12 * i;
        let (Some(tag), Some(kind), Some(n)) =
            (u16_at(entry), u16_at(entry + 2), u32_at(entry + 4))
        else {
            break;
        };
        let size = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 => 4,
            _ => continue, // rationals and others: no recipe tag uses them
        };
        let Some(len) = (n as usize).checked_mul(size).filter(|&len| len > 0) else {
            continue;
        };
        let at = if len <= 4 {
            entry + 8
        } else {
            match u32_at(entry + 8) {
                Some(offset) => offset as usize,
                None => continue,
            }
        };
        let Some(data) = at.checked_add(len).and_then(|end| note.get(at..end)) else {
            tags.insert(tag, None);
            continue;
        };
        let read = |index: usize| -> Option<i64> {
            let b = data.get(index * size..(index + 1) * size)?;
            Some(match kind {
                1 | 2 | 7 => b[0] as i64,
                6 => b[0] as i8 as i64,
                3 => u16::from_le_bytes([b[0], b[1]]) as i64,
                8 => i16::from_le_bytes([b[0], b[1]]) as i64,
                4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
                _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64,
            })
        };
        tags.insert(
            tag,
            read(0).map(|first| Value {
                first,
                first_i8: data[0] as i8,
                second: read(1),
            }),
        );
    }
    Some(tags)
}

/// Monochrome, Acros and Sepia live in `Saturation` (0x1003), not `FilmMode`.
fn monochrome_film(saturation: i64) -> Option<&'static str> {
    Some(match saturation {
        0x300 => "Monochrome",
        0x301 => "Monochrome + R",
        0x302 => "Monochrome + Ye",
        0x303 => "Monochrome + G",
        0x310 => "Sepia",
        0x500 => "Acros",
        0x501 => "Acros + R",
        0x502 => "Acros + Ye",
        0x503 => "Acros + G",
        _ => return None,
    })
}

/// `FilmMode` (0x1401), named as on the cameras' menus.
fn colour_film(mode: i64) -> String {
    match mode {
        0x000 => "Provia",
        0x100 => "Studio Portrait",
        0x110 => "Studio Portrait Enhanced Saturation",
        0x120 => "Astia",
        0x130 => "Studio Portrait Increased Sharpness",
        0x200 | 0x400 => "Velvia",
        0x300 => "Studio Portrait Ex",
        0x500 => "Pro Neg. Std",
        0x501 => "Pro Neg. Hi",
        0x600 => "Classic Chrome",
        0x700 => "Eterna",
        0x800 => "Classic Neg.",
        0x900 => "Eterna Bleach Bypass",
        0xA00 => "Nostalgic Neg.",
        0xB00 => "Reala Ace",
        other => return format!("Unknown (0x{other:X})"),
    }
    .to_string()
}

fn off_weak_strong(raw: i64) -> Option<&'static str> {
    Some(match raw {
        0 => "Off",
        32 => "Weak",
        64 => "Strong",
        _ => return None,
    })
}

fn grain_size(raw: i64) -> Option<&'static str> {
    Some(match raw {
        16 => "Small",
        32 => "Large",
        _ => return None,
    })
}

fn dynamic_range(setting: Option<i64>, manual: Option<i64>, auto: Option<i64>) -> Option<String> {
    Some(match setting? {
        0x000 => match auto {
            Some(percent) => format!("Auto (DR{percent})"),
            None => "Auto".to_string(),
        },
        0x001 => format!("DR{}", manual?),
        0x100 => "Standard".to_string(),
        0x200 => "Wide 1".to_string(),
        0x201 => "Wide 2".to_string(),
        _ => return None,
    })
}

fn white_balance(
    mode: Option<i64>,
    kelvin: Option<i64>,
    fine_tune: Option<Value>,
) -> Option<String> {
    let mode = mode?;
    let name = match mode {
        0x000 => "Auto".to_string(),
        0x001 => "Auto (white priority)".to_string(),
        0x002 => "Auto (ambience priority)".to_string(),
        0x100 => "Daylight".to_string(),
        0x200 => "Shade".to_string(),
        0x300 => "Daylight fluorescent".to_string(),
        0x301 => "Day white fluorescent".to_string(),
        0x302 => "White fluorescent".to_string(),
        0x303 => "Warm white fluorescent".to_string(),
        0x304 => "Living room warm white fluorescent".to_string(),
        0x400 => "Incandescent".to_string(),
        0x500 => "Flash".to_string(),
        0x600 => "Underwater".to_string(),
        0xF00 => "Custom".to_string(),
        0xF01..=0xF04 => format!("Custom {}", mode - 0xF00 + 1),
        0xFF0 => match kelvin {
            Some(k) => format!("{k}K"),
            None => "Kelvin".to_string(),
        },
        _ => return None,
    };
    let shift = fine_tune.and_then(|v| Some((v.first as i32, v.second? as i32)));
    Some(match shift.map(wb_shift) {
        Some((0, 0)) | None => name,
        Some((red, blue)) => format!("{name} · R{} B{}", signed(red), signed(blue)),
    })
}

/// Fine tune in the camera's steps. Current bodies store steps × 20 (an X-T3
/// set to R+3 B−5 records 60, −100); values that aren't multiples of 20 are
/// taken as steps already.
fn wb_shift((red, blue): (i32, i32)) -> (i64, i64) {
    if red % 20 == 0 && blue % 20 == 0 {
        ((red / 20) as i64, (blue / 20) as i64)
    } else {
        (red as i64, blue as i64)
    }
}

/// Highlight and shadow tone: the camera's value is −raw / 16 (half steps on
/// newer bodies).
fn tone(raw: i64) -> String {
    let halves = -raw / 8;
    if halves % 2 == 0 {
        signed(halves / 2)
    } else {
        let sign = if halves > 0 { "+" } else { "−" };
        format!("{sign}{}.5", halves.abs() / 2)
    }
}

/// `Saturation` (0x1003) for colour films.
fn colour(raw: i64) -> Option<i64> {
    Some(match raw {
        0x000 => 0,
        0x080 => 1,
        0x100 => 2,
        0x0C0 => 3,
        0x0E0 => 4,
        0x180 => -1,
        0x400 => -2,
        0x4C0 => -3,
        0x4E0 => -4,
        _ => return None,
    })
}

fn sharpness(raw: i64) -> Option<i64> {
    Some(match raw {
        0x00 => -4,
        0x01 => -3,
        0x02 => -2,
        0x82 => -1,
        0x03 => 0,
        0x84 => 1,
        0x04 => 2,
        0x05 => 3,
        0x06 => 4,
        _ => return None,
    })
}

/// High ISO noise reduction (0x100E).
fn noise_reduction(raw: i64) -> Option<i64> {
    Some(match raw {
        0x000 => 0,
        0x180 => 1,
        0x100 => 2,
        0x1C0 => 3,
        0x1E0 => 4,
        0x280 => -1,
        0x200 => -2,
        0x2C0 => -3,
        0x2E0 => -4,
        _ => return None,
    })
}

/// `+2`, `−1` (minus sign), `0`.
fn signed(value: i64) -> String {
    match value {
        0 => "0".to_string(),
        v if v > 0 => format!("+{v}"),
        v => format!("−{}", -v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Recipe {
        let path = format!(
            "{}/tests/fixtures/fuji/{name}.makernote",
            env!("CARGO_MANIFEST_DIR")
        );
        parse(&std::fs::read(path).unwrap()).unwrap()
    }

    /// exiftool's raw reading (`-n`) of the same note.
    fn exiftool(name: &str) -> serde_json::Map<String, serde_json::Value> {
        let path = format!(
            "{}/tests/fixtures/fuji/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        json[0].as_object().unwrap().clone()
    }

    fn setting<'a>(recipe: &'a Recipe, label: &str) -> Option<&'a str> {
        recipe
            .settings
            .iter()
            .find(|(l, _)| *l == label)
            .map(|(_, v)| v.as_str())
    }

    const FIXTURES: [&str; 6] = [
        "xt3-classic-chrome-daylight",
        "xt3-classic-chrome-custom",
        "xt3-acros-auto",
        "xt3-acros-kelvin",
        "xe3-acros-kelvin",
        "xe3-astia",
    ];

    #[test]
    fn reads_the_same_raw_values_as_exiftool() {
        let tags = [
            (0x1001, "Sharpness"),
            (0x1002, "WhiteBalance"),
            (0x1003, "Saturation"),
            (0x1005, "ColorTemperature"),
            (0x100E, "NoiseReduction"),
            (0x1040, "ShadowTone"),
            (0x1041, "HighlightTone"),
            (0x1047, "GrainEffectRoughness"),
            (0x1048, "ColorChromeEffect"),
            (0x1401, "FilmMode"),
            (0x1402, "DynamicRangeSetting"),
            (0x1403, "DevelopmentDynamicRange"),
            (0x140B, "AutoDynamicRange"),
            (0x1436, "ImageGeneration"),
        ];
        for name in FIXTURES {
            let note = std::fs::read(format!(
                "{}/tests/fixtures/fuji/{name}.makernote",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let ours = read_tags(&note).unwrap();
            let theirs = exiftool(name);
            for (tag, key) in tags {
                let expected = theirs
                    .get(&format!("FujiFilm:{key}"))
                    .and_then(|v| v.as_i64());
                let got = ours.get(&tag).copied().flatten().map(|v| v.first);
                assert_eq!(got, expected, "{name}: {key}");
            }
            let fine_tune = theirs
                .get("FujiFilm:WhiteBalanceFineTune")
                .and_then(|v| v.as_str())
                .map(|s| {
                    s.split(' ')
                        .map(|n| n.parse::<i64>().unwrap())
                        .collect::<Vec<_>>()
                });
            let got = ours
                .get(&0x100A)
                .copied()
                .flatten()
                .map(|v| vec![v.first, v.second.unwrap()]);
            assert_eq!(got, fine_tune, "{name}: WhiteBalanceFineTune");
        }
    }

    // Expected values: exiftool's printed reading of each photo, in the
    // cameras' wording (exiftool prints "Red +60, Blue -100" unscaled).
    #[test]
    fn x_t3_classic_chrome() {
        let r = fixture("xt3-classic-chrome-daylight");
        assert_eq!(r.film.as_deref(), Some("Classic Chrome"));
        assert_eq!(
            r.settings,
            [
                ("Grain", "Off"),
                ("Colour chrome", "Strong"),
                ("Dynamic range", "DR400"),
                ("White balance", "Daylight · R+3 B−5"),
                ("Highlight", "−1"),
                ("Shadow", "−2"),
                ("Colour", "+3"),
                ("Sharpness", "+1"),
                ("Noise reduction", "−1"),
            ]
            .map(|(l, v)| (l, v.to_string()))
        );
        let r = fixture("xt3-classic-chrome-custom");
        assert_eq!(setting(&r, "White balance"), Some("Custom · R−6 B+5"));
        assert_eq!(setting(&r, "Dynamic range"), Some("DR100"));
    }

    #[test]
    fn x_t3_acros() {
        let r = fixture("xt3-acros-auto");
        assert_eq!(r.film.as_deref(), Some("Acros + R"));
        assert_eq!(setting(&r, "Dynamic range"), Some("Auto (DR100)"));
        assert_eq!(setting(&r, "White balance"), Some("Auto"));
        assert_eq!(setting(&r, "Highlight"), Some("+1"));
        assert_eq!(setting(&r, "Shadow"), Some("+3"));
        assert_eq!(setting(&r, "Toning"), Some("WC 0"));
        assert_eq!(setting(&r, "Colour"), None, "monochrome has no colour");
        assert_eq!(setting(&r, "Sharpness"), Some("−1"));
        assert_eq!(setting(&r, "Noise reduction"), Some("−4"));
        let r = fixture("xt3-acros-kelvin");
        assert_eq!(setting(&r, "White balance"), Some("10000K · R+1 B−4"));
    }

    #[test]
    fn x_e3() {
        let r = fixture("xe3-astia");
        assert_eq!(r.film.as_deref(), Some("Astia"));
        assert_eq!(setting(&r, "Grain"), Some("Strong"));
        assert_eq!(setting(&r, "Colour chrome"), None, "the X-E3 has none");
        assert_eq!(setting(&r, "Dynamic range"), Some("DR200"));
        assert_eq!(
            setting(&r, "White balance"),
            Some("Daylight fluorescent · R−2 B+4")
        );
        assert_eq!(setting(&r, "Highlight"), Some("−2"));
        assert_eq!(setting(&r, "Shadow"), Some("0"));
        assert_eq!(setting(&r, "Colour"), Some("+2"));
        assert_eq!(setting(&r, "Sharpness"), Some("−4"));
        let r = fixture("xe3-acros-kelvin");
        assert_eq!(r.film.as_deref(), Some("Acros + R"));
        assert_eq!(setting(&r, "White balance"), Some("10000K · R+7 B+7"));
        assert_eq!(setting(&r, "Shadow"), Some("+4"));
    }

    #[test]
    fn tones_in_half_steps() {
        assert_eq!(tone(-32), "+2");
        assert_eq!(tone(16), "−1");
        assert_eq!(tone(-24), "+1.5");
        assert_eq!(tone(8), "−0.5");
        assert_eq!(tone(0), "0");
    }

    #[test]
    fn untrusted_input_never_panics() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"CANON   whatever"), None);
        let good = std::fs::read(format!(
            "{}/tests/fixtures/fuji/xt3-classic-chrome-daylight.makernote",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        // Every truncation, and garbage in every byte of the header and IFD.
        for len in 0..good.len() {
            let _ = parse(&good[..len]);
        }
        for at in 8..200.min(good.len()) {
            let mut bad = good.clone();
            bad[at] = 0xFF;
            let _ = parse(&bad);
        }
    }
}
