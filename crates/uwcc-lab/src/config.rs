//! Programmeinstellungen in `%APPDATA%\UnderwaterPhotoLab\config.json`. Unbekannte Schlüssel
//! bleiben beim Speichern erhalten.

use std::{fs, path::PathBuf};

use serde_json::{Map, Value};
use uwcc_photo::{sidecar, DEFAULT_JPEG_QUALITY};

pub const MIN_JPEG_QUALITY: u8 = 50;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub jpeg_quality: u8,
    /// Leiste zeigt exportierte Bilder korrigiert.
    pub strip_corrected: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { jpeg_quality: DEFAULT_JPEG_QUALITY, strip_corrected: false }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("UnderwaterPhotoLab").join("config.json"))
}

fn read_object() -> Map<String, Value> {
    path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| sidecar::parse_json(&b))
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

pub fn load() -> Config {
    let o = read_object();
    let d = Config::default();
    Config {
        jpeg_quality: o
            .get("jpegQuality")
            .and_then(Value::as_u64)
            .and_then(|q| u8::try_from(q).ok())
            .map_or(d.jpeg_quality, |q| q.clamp(MIN_JPEG_QUALITY, 100)),
        strip_corrected: o.get("stripCorrected").and_then(Value::as_bool).unwrap_or(d.strip_corrected),
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    let path = path().ok_or("APPDATA ist nicht gesetzt")?;
    let mut o = read_object();
    o.remove("exportTarget");
    o.insert("jpegQuality".into(), config.jpeg_quality.into());
    o.insert("stripCorrected".into(), config.strip_corrected.into());
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(o)).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
