//! `uw-korrigiert\.uwcc.json`: Einstellungen und Export-Stand je Bild eines Ordners, damit ein
//! Ordner später weiterbearbeitet werden kann.
//!
//! ```json
//! {"version": 1, "files": {"P4100207.JPG": {
//!     "settings": {"auto": true, "strength": 1, "temperature": 0, "tint": 0, "vibrance": 0,
//!                  "redLimit": 4, "delta": {"hueShift": 0, "red": [0, 0], "green": [0, 0], "blue": [0, 0]}},
//!     "exported": {"settings": {…}, "target": "subfolder", "quality": 95}}}}
//! ```

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Map, Value};
use uwcc_core::{Delta, LevelsDelta, Settings};

use crate::{ExportTarget, SUBFOLDER};

pub const FILE_NAME: &str = ".uwcc.json";
const VERSION: u64 = 1;

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
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map(|v| from_json(&v))
        .unwrap_or_default()
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_settings() -> Settings {
        let mut s = Settings { strength: 0.4, temperature: 17.5, tint: -3.25, vibrance: 22.0, red_limit: 2.7, ..Settings::default() };
        s.delta.hue_shift = -12.0;
        s.delta.blue = LevelsDelta { low: 5.0, high: -8.0 };
        s
    }

    #[test]
    fn settings_roundtrip_exactly() {
        let s = sample_settings();
        assert_eq!(settings_from_json(&settings_to_json(&s)), s);
        assert_eq!(settings_from_json(&json!({})), Settings::default(), "leeres Objekt = Standard");
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

        fs::write(path(&dir), b"{ kaputt").unwrap();
        assert_eq!(load(&dir), Sidecar::default(), "unlesbar = leer");
    }
}
