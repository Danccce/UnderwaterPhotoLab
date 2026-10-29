//! `uw-korrigiert\.uwcc.json`: Einstellungen und Export-Stand je Bild eines Ordners, damit ein
//! Ordner später weiterbearbeitet werden kann.
//!
//! ```json
//! {"version": 2, "files": {"P4100207.JPG": {
//!     "settings": {"auto": true, "strength": 1, "temperature": 0, "tint": 0, "vibrance": 0,
//!                  "redLimit": 4, "delta": {"hueShift": 0, "red": [0, 0], "green": [0, 0], "blue": [0, 0]},
//!                  "tone": {"exposure": 0, "contrast": 0, "highlights": 0, "shadows": 0, "dehaze": 0},
//!                  "detail": {"clarity": 0, "sharpen": 0, "sharpenRadius": 1, "sharpenMask": 0,
//!                             "noiseColor": 0, "noiseLuma": 0},
//!                  "geometry": {"quarterTurns": 0, "flipHorizontal": false, "straighten": 0,
//!                               "crop": {"x": 0, "y": 0, "width": 1, "height": 1}}},
//!     "exported": {"settings": {…}, "target": "subfolder", "quality": 95}}}}
//! ```
//!
//! Version 1 hatte keine Gruppen `tone`, `detail`, `geometry`; fehlende Werte sind neutral.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};
use uwcc_core::{Crop, Delta, Detail, Geometry, LevelsDelta, Settings, Tone, DEFAULT_SHARPEN_RADIUS};

use crate::{ExportTarget, SUBFOLDER};

pub const FILE_NAME: &str = ".uwcc.json";
const VERSION: u64 = 2;

/// Womit ein Bild zuletzt exportiert wurde; stimmt das mit dem jetzigen Stand überein, ist der
/// Export aktuell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exported {
    pub settings: Settings,
    pub target: ExportTarget,
    pub quality: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub settings: Option<Settings>,
    pub exported: Option<Exported>,
}

/// Einträge nach Dateiname (ohne Ordner).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sidecar {
    pub entries: BTreeMap<String, Entry>,
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(SUBFOLDER).join(FILE_NAME)
}

/// Liest die Datei; fehlt sie oder ist sie unlesbar, gibt es einen leeren Stand.
pub fn load(dir: &Path) -> Sidecar {
    fs::read(path(dir))
        .ok()
        .and_then(|bytes| parse_json(&bytes))
        .map(|v| from_json(&v))
        .unwrap_or_default()
}

/// JSON aus Dateiinhalt; ein UTF-8-BOM (Editor, PowerShell) wird übersprungen.
pub fn parse_json(bytes: &[u8]) -> Option<Value> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    serde_json::from_slice(bytes).ok()
}

/// Schreibt die Datei (über eine Temp-Datei); legt `uw-korrigiert\` bei Bedarf an.
pub fn save(dir: &Path, sidecar: &Sidecar) -> Result<(), String> {
    let out = path(dir);
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", out.display());
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| err(&e))?;
    }
    let text = serde_json::to_string_pretty(&to_json(sidecar)).map_err(|e| err(&e))?;
    let tmp = out.with_file_name(format!("{FILE_NAME}.tmp"));
    fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &out)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        err(&e)
    })
}

fn to_json(sidecar: &Sidecar) -> Value {
    let files: Map<String, Value> = sidecar
        .entries
        .iter()
        .filter(|(_, e)| e.settings.is_some() || e.exported.is_some())
        .map(|(name, e)| {
            let mut entry = Map::new();
            if let Some(s) = &e.settings {
                entry.insert("settings".into(), settings_to_json(s));
            }
            if let Some(x) = &e.exported {
                entry.insert(
                    "exported".into(),
                    json!({"settings": settings_to_json(&x.settings), "target": target_name(x.target), "quality": x.quality}),
                );
            }
            (name.clone(), Value::Object(entry))
        })
        .collect();
    json!({"version": VERSION, "files": files})
}

fn from_json(v: &Value) -> Sidecar {
    let entries = v
        .get("files")
        .and_then(Value::as_object)
        .map(|files| {
            files
                .iter()
                .map(|(name, e)| {
                    let exported = e.get("exported").and_then(|x| {
                        Some(Exported {
                            settings: settings_from_json(x.get("settings")?),
                            target: target_from_name(x.get("target")?.as_str()?)?,
                            quality: u8::try_from(x.get("quality")?.as_u64()?).ok()?,
                        })
                    });
                    let settings = e.get("settings").map(settings_from_json);
                    (name.clone(), Entry { settings, exported })
                })
                .collect()
        })
        .unwrap_or_default();
    Sidecar { entries }
}

pub fn target_name(t: ExportTarget) -> &'static str {
    match t {
        ExportTarget::Subfolder => "subfolder",
        ExportTarget::SameFolder => "same-folder",
    }
}

pub fn target_from_name(name: &str) -> Option<ExportTarget> {
    match name {
        "subfolder" => Some(ExportTarget::Subfolder),
        "same-folder" => Some(ExportTarget::SameFolder),
        _ => None,
    }
}

pub fn settings_to_json(s: &Settings) -> Value {
    let pair = |l: LevelsDelta| json!([l.low, l.high]);
    json!({
        "auto": s.auto,
        "strength": s.strength,
        "temperature": s.temperature,
        "tint": s.tint,
        "vibrance": s.vibrance,
        "redLimit": s.red_limit,
        "delta": {
            "hueShift": s.delta.hue_shift,
            "red": pair(s.delta.red),
            "green": pair(s.delta.green),
            "blue": pair(s.delta.blue),
        },
        "tone": {
            "exposure": s.tone.exposure,
            "contrast": s.tone.contrast,
            "highlights": s.tone.highlights,
            "shadows": s.tone.shadows,
            "dehaze": s.tone.dehaze,
        },
        "detail": {
            "clarity": s.detail.clarity,
            "sharpen": s.detail.sharpen,
            "sharpenRadius": s.detail.sharpen_radius,
            "sharpenMask": s.detail.sharpen_mask,
            "noiseColor": s.detail.noise_color,
            "noiseLuma": s.detail.noise_luma,
        },
        "geometry": {
            "quarterTurns": s.geometry.quarter_turns,
            "flipHorizontal": s.geometry.flip_horizontal,
            "straighten": s.geometry.straighten,
            "crop": s.geometry.crop.map(|c| json!({"x": c.x, "y": c.y, "width": c.width, "height": c.height})),
        },
    })
}

/// Fehlende oder falsche Felder bekommen ihren Standardwert.
pub fn settings_from_json(v: &Value) -> Settings {
    let d = Settings::default();
    let num = |key: &str, default: f64| v.get(key).and_then(Value::as_f64).unwrap_or(default);
    let delta = v.get("delta");
    let pair = |key: &str| {
        delta
            .and_then(|d| d.get(key))
            .and_then(|p| Some(LevelsDelta { low: p.get(0)?.as_f64()?, high: p.get(1)?.as_f64()? }))
            .unwrap_or_default()
    };
    Settings {
        auto: v.get("auto").and_then(Value::as_bool).unwrap_or(d.auto),
        strength: num("strength", d.strength as f64) as f32,
        temperature: num("temperature", d.temperature as f64) as f32,
        tint: num("tint", d.tint as f64) as f32,
        vibrance: num("vibrance", d.vibrance as f64) as f32,
        red_limit: num("redLimit", d.red_limit),
        delta: Delta {
            hue_shift: delta.and_then(|d| d.get("hueShift")).and_then(Value::as_f64).unwrap_or(0.0),
            red: pair("red"),
            green: pair("green"),
            blue: pair("blue"),
        },
        tone: tone_from_json(v.get("tone")),
        detail: detail_from_json(v.get("detail")),
        geometry: geometry_from_json(v.get("geometry")),
    }
}

/// Zahl aus einer Gruppe als `f32`, sonst 0 (neutral).
fn group_f32(group: Option<&Value>, key: &str) -> f32 {
    group.and_then(|g| g.get(key)).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn tone_from_json(g: Option<&Value>) -> Tone {
    Tone {
        exposure: group_f32(g, "exposure"),
        contrast: group_f32(g, "contrast"),
        highlights: group_f32(g, "highlights"),
        shadows: group_f32(g, "shadows"),
        dehaze: group_f32(g, "dehaze"),
    }
}

fn detail_from_json(g: Option<&Value>) -> Detail {
    Detail {
        clarity: group_f32(g, "clarity"),
        sharpen: group_f32(g, "sharpen"),
        sharpen_radius: g
            .and_then(|g| g.get("sharpenRadius"))
            .and_then(Value::as_f64)
            .map_or(DEFAULT_SHARPEN_RADIUS, |r| r as f32),
        sharpen_mask: group_f32(g, "sharpenMask"),
        noise_color: group_f32(g, "noiseColor"),
        noise_luma: group_f32(g, "noiseLuma"),
    }
}

fn geometry_from_json(g: Option<&Value>) -> Geometry {
    let crop = g.and_then(|g| g.get("crop")).and_then(|c| {
        let f = |k: &str| c.get(k).and_then(Value::as_f64).map(|v| v as f32);
        Some(Crop { x: f("x")?, y: f("y")?, width: f("width")?, height: f("height")? })
    });
    Geometry {
        quarter_turns: g.and_then(|g| g.get("quarterTurns")).and_then(Value::as_u64).map_or(0, |q| (q % 4) as u8),
        flip_horizontal: g.and_then(|g| g.get("flipHorizontal")).and_then(Value::as_bool).unwrap_or(false),
        straighten: group_f32(g, "straighten"),
        crop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_settings() -> Settings {
        let mut s = Settings { strength: 0.4, temperature: 17.5, tint: -3.25, vibrance: 22.0, red_limit: 2.7, ..Settings::default() };
        s.delta.hue_shift = -12.0;
        s.delta.blue = LevelsDelta { low: 5.0, high: -8.0 };
        s.tone = Tone { exposure: 0.3, contrast: 12.0, highlights: -40.0, shadows: 25.5, dehaze: 60.0 };
        s.detail = Detail {
            clarity: 20.0,
            sharpen: 35.0,
            sharpen_radius: 1.4,
            sharpen_mask: 30.0,
            noise_color: 50.0,
            noise_luma: 10.0,
        };
        s.geometry = Geometry {
            quarter_turns: 3,
            flip_horizontal: true,
            straighten: -2.75,
            crop: Some(Crop { x: 0.1, y: 0.05, width: 0.8, height: 0.6 }),
        };
        s
    }

    #[test]
    fn settings_roundtrip_exactly() {
        let s = sample_settings();
        assert_eq!(settings_from_json(&settings_to_json(&s)), s);
        assert_eq!(settings_from_json(&json!({})), Settings::default(), "leeres Objekt = Standard");
    }

    #[test]
    fn reads_version_1_settings() {
        let v1 = json!({"auto": true, "strength": 0.5, "temperature": 10, "tint": 0, "vibrance": 0, "redLimit": 4,
                        "delta": {"hueShift": 3, "red": [0, 0], "green": [0, 0], "blue": [0, 0]}});
        let s = settings_from_json(&v1);
        assert_eq!((s.strength, s.temperature, s.delta.hue_shift), (0.5, 10.0, 3.0));
        assert_eq!((s.tone, s.detail, s.geometry), (Tone::default(), Detail::default(), Geometry::default()));
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join("uwcc-photo-tests").join("sidecar");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir), Sidecar::default(), "ohne Datei leer");

        let mut sc = Sidecar::default();
        sc.entries.insert("P1.JPG".into(), Entry { settings: Some(sample_settings()), exported: None });
        sc.entries.insert(
            "P2.JPG".into(),
            Entry {
                settings: None,
                exported: Some(Exported { settings: Settings::default(), target: ExportTarget::SameFolder, quality: 90 }),
            },
        );
        sc.entries.insert("leer.JPG".into(), Entry::default());
        save(&dir, &sc).unwrap();

        let back = load(&dir);
        assert_eq!(back.entries.len(), 2, "leere Einträge werden nicht gespeichert");
        assert_eq!(back.entries["P1.JPG"], sc.entries["P1.JPG"]);
        assert_eq!(back.entries["P2.JPG"], sc.entries["P2.JPG"]);
        assert!(!path(&dir).with_file_name(format!("{FILE_NAME}.tmp")).exists());

        let mut with_bom = b"\xEF\xBB\xBF".to_vec();
        with_bom.extend(fs::read(path(&dir)).unwrap());
        fs::write(path(&dir), with_bom).unwrap();
        assert_eq!(load(&dir).entries.len(), 2, "UTF-8-BOM wird übersprungen");

        fs::write(path(&dir), b"{ kaputt").unwrap();
        assert_eq!(load(&dir), Sidecar::default(), "unlesbar = leer");
    }
}
