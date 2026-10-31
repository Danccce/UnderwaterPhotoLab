//! Programmeinstellungen in `%APPDATA%\UnderwaterPhotoLab\config.json`. Unbekannte Schlüssel
//! bleiben beim Speichern erhalten.

use std::{fs, path::PathBuf};

use serde_json::{json, Map, Value};
use uwcc_core::Groups;
use uwcc_photo::{sidecar, DEFAULT_JPEG_QUALITY};

pub const MIN_JPEG_QUALITY: u8 = 50;

/// Mindestbreite der Zellen der Rasteranzeige in px; die scharfen Vorschaubilder reichen bis 320.
pub fn clamp_grid_size(px: i64) -> i32 {
    px.clamp(110, 320) as i32
}

/// Ein selbst gewählter Programmpfad: ersetzt die Suche für ein bekanntes Programm (`id` wie
/// `rapidraw`) oder ist ein eigenes Programm (`id` beginnt mit `custom-`). In `args` steht
/// `{files}` für die exportierten Dateien und `{folder}` für den Ordner `uw-korrigiert`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgramPath {
    pub id: String,
    pub name: String,
    pub exe: PathBuf,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub jpeg_quality: u8,
    /// Leiste zeigt exportierte Bilder korrigiert.
    pub strip_corrected: bool,
    /// Programm für „In … öffnen“.
    pub open_with: String,
    pub programs: Vec<ProgramPath>,
    /// Zuletzt geöffneter Ordner; wird beim Start ohne Argument wieder geöffnet.
    pub last_folder: Option<PathBuf>,
    /// Zuletzt gewählte Reglergruppe: 0 Farbe, 1 Ton, 2 Details.
    pub panel_tab: i32,
    /// Mindestbreite der Rasterzellen in px (Regler „Größe“).
    pub grid_size: i32,
    /// Gruppen, die „Mehrere Bilder“ überträgt.
    pub transfer_groups: Groups,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            jpeg_quality: DEFAULT_JPEG_QUALITY,
            strip_corrected: false,
            open_with: crate::programs::DEFAULT_PROGRAM.to_owned(),
            programs: Vec::new(),
            last_folder: None,
            panel_tab: 0,
            grid_size: 190,
            transfer_groups: Groups::default(),
        }
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

fn program_from_json(v: &Value) -> Option<ProgramPath> {
    let args = match v.get("args").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        None => vec!["{files}".to_owned()],
    };
    Some(ProgramPath {
        id: v.get("id")?.as_str()?.to_owned(),
        name: v.get("name")?.as_str()?.to_owned(),
        exe: PathBuf::from(v.get("exe")?.as_str()?),
        args,
    })
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
        open_with: o.get("openWith").and_then(Value::as_str).map_or(d.open_with, str::to_owned),
        programs: o
            .get("programs")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(program_from_json).collect())
            .unwrap_or_default(),
        last_folder: o.get("lastFolder").and_then(Value::as_str).map(PathBuf::from),
        panel_tab: o.get("panelTab").and_then(Value::as_i64).map_or(d.panel_tab, |t| t.clamp(0, 2) as i32),
        grid_size: o.get("gridSize").and_then(Value::as_i64).map_or(d.grid_size, clamp_grid_size),
        transfer_groups: o.get("transferGroups").map_or(d.transfer_groups, |g| {
            let flag = |k: &str, default: bool| g.get(k).and_then(Value::as_bool).unwrap_or(default);
            let t = d.transfer_groups;
            Groups {
                color: flag("color", t.color),
                tone: flag("tone", t.tone),
                detail: flag("detail", t.detail),
                geometry: flag("geometry", t.geometry),
            }
        }),
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    let path = path().ok_or("APPDATA ist nicht gesetzt")?;
    let mut o = read_object();
    o.remove("exportTarget");
    o.insert("jpegQuality".into(), config.jpeg_quality.into());
    o.insert("stripCorrected".into(), config.strip_corrected.into());
    o.insert("openWith".into(), config.open_with.clone().into());
    let programs: Vec<Value> = config
        .programs
        .iter()
        .map(|p| json!({"id": p.id, "name": p.name, "exe": p.exe.display().to_string(), "args": p.args}))
        .collect();
    o.insert("programs".into(), programs.into());
    o.insert("panelTab".into(), config.panel_tab.into());
    o.insert("gridSize".into(), config.grid_size.into());
    let g = config.transfer_groups;
    o.insert("transferGroups".into(), json!({"color": g.color, "tone": g.tone, "detail": g.detail, "geometry": g.geometry}));
    match &config.last_folder {
        Some(dir) => o.insert("lastFolder".into(), dir.display().to_string().into()),
        None => o.remove("lastFolder"),
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(o)).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}
