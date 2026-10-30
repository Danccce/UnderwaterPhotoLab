//! Presets: mitgelieferte und eigene (in `%APPDATA%\UnderwaterPhotoLab\presets.json`).
//! Ein Preset sind Farbe, Ton und Details – bei eingeschaltetem Auto als Abweichung von der
//! Analyse des jeweiligen Bildes; Drehen und Zuschnitt nie.

use std::{fs, path::PathBuf};

use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use uwcc_core::{Detail, Groups, Settings, Tone};
use uwcc_photo::sidecar;

use crate::{selection, with_state, LabWindow, PresetItem};

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub settings: Settings,
}

/// Mitgelieferte Ausgangspunkte; danach nach Geschmack nachstellen.
pub fn builtin() -> Vec<Preset> {
    let preset = |name: &str, settings: Settings| Preset { name: name.to_owned(), settings };
    vec![
        preset(
            "Blauwasser",
            Settings {
                temperature: 12.0,
                vibrance: 15.0,
                tone: Tone { contrast: 10.0, dehaze: 25.0, ..Tone::default() },
                detail: Detail { clarity: 15.0, ..Detail::default() },
                ..Settings::default()
            },
        ),
        preset(
            "Grünwasser (See)",
            Settings {
                temperature: 5.0,
                tint: 25.0,
                vibrance: 10.0,
                tone: Tone { contrast: 10.0, dehaze: 35.0, ..Tone::default() },
                detail: Detail { clarity: 10.0, ..Detail::default() }.with_noise(20.0),
                ..Settings::default()
            },
        ),
        preset(
            "Mit Blitz",
            Settings {
                strength: 0.5,
                vibrance: 10.0,
                tone: Tone { shadows: 10.0, ..Tone::default() },
                detail: Detail { clarity: 10.0, ..Detail::default() }.with_noise(20.0),
                ..Settings::default()
            },
        ),
    ]
}

fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("UnderwaterPhotoLab").join("presets.json"))
}

/// Eigene Presets; fehlt die Datei oder ist sie unlesbar, keine.
pub fn load_user() -> Vec<Preset> {
    let Some(v) = path().and_then(|p| fs::read(p).ok()).and_then(|b| sidecar::parse_json(&b)) else {
        return Vec::new();
    };
    v.get("presets")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    Some(Preset {
                        name: p.get("name")?.as_str()?.to_owned(),
                        settings: sidecar::settings_from_json(p.get("settings")?),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_user(presets: &[Preset]) -> Result<(), String> {
    let path = path().ok_or("APPDATA ist nicht gesetzt")?;
    let list: Vec<Value> =
        presets.iter().map(|p| json!({"name": p.name, "settings": sidecar::settings_to_json(&p.settings)})).collect();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&json!({"version": 1, "presets": list})).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Mitgelieferte zuerst, dann die eigenen.
fn all(user: &[Preset]) -> Vec<(Preset, bool)> {
    builtin().into_iter().map(|p| (p, true)).chain(user.iter().cloned().map(|p| (p, false))).collect()
}

/// Liste fürs Menü aktualisieren.
pub fn show(app: &LabWindow) {
    let items: Vec<PresetItem> = with_state(|st| all(&st.presets))
        .into_iter()
        .map(|(p, builtin)| PresetItem { name: p.name.into(), builtin })
        .collect();
    app.set_presets(ModelRc::new(VecModel::from(items)));
}

/// Preset `index` auf die Auswahl (mehrere Bilder) bzw. das aktuelle Bild anwenden; Drehen und
/// Zuschnitt der Bilder bleiben.
pub fn apply(app: &LabWindow, index: usize) {
    let Some((preset, targets)) = with_state(|st| {
        let (preset, _) = all(&st.presets).into_iter().nth(index)?;
        let targets: Vec<PathBuf> = if st.selection.len() > 1 {
            st.selection.iter().filter_map(|&i| st.files.get(i)).cloned().collect()
        } else {
            vec![st.current.as_ref()?.path.clone()]
        };
        Some((preset, targets))
    }) else {
        return;
    };
    let groups = Groups { color: true, tone: true, detail: true, geometry: false };
    let n = selection::apply(app, &targets, |s| s.with_groups(&preset.settings, groups));
    let what = if n == 1 { "das Bild".to_owned() } else { format!("{n} Bilder") };
    app.invoke_notify(format!("Preset „{}“ auf {what} angewendet.", preset.name).into());
}

/// Die Einstellungen des aktuellen Bildes (ohne Drehen und Zuschnitt) als eigenes Preset; ein
/// gleichnamiges wird ersetzt.
pub fn save_current(app: &LabWindow, name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let result = with_state(|st| {
        let l = st.current.as_ref()?;
        let settings = Settings { geometry: Default::default(), ..l.settings };
        match st.presets.iter_mut().find(|p| p.name == name) {
            Some(p) => p.settings = settings,
            None => st.presets.push(Preset { name: name.to_owned(), settings }),
        }
        Some(save_user(&st.presets))
    });
    match result {
        Some(Ok(())) => app.invoke_notify(format!("Preset „{name}“ gespeichert.").into()),
        Some(Err(e)) => app.invoke_notify(format!("Preset nicht gespeichert: {e}").into()),
        None => return,
    }
    show(app);
}

/// Eigenes Preset löschen (mitgelieferte nicht).
pub fn delete(app: &LabWindow, index: usize) {
    let builtin = builtin().len();
    let result = with_state(|st| {
        let i = index.checked_sub(builtin).filter(|&i| i < st.presets.len())?;
        let removed = st.presets.remove(i);
        Some((removed.name, save_user(&st.presets)))
    });
    if let Some((name, saved)) = result {
        app.invoke_notify(match saved {
            Ok(()) => format!("Preset „{name}“ gelöscht.").into(),
            Err(e) => format!("Preset nicht gelöscht: {e}").into(),
        });
        show(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_change_something_but_never_the_geometry() {
        for p in builtin() {
            assert_ne!(p.settings, Settings::default(), "{}", p.name);
            assert_eq!(p.settings.geometry, Default::default(), "{}", p.name);
            assert!(p.settings.auto, "{}: relativ zur Auto-Analyse", p.name);
        }
    }

    #[test]
    fn presets_survive_the_json_round_trip() {
        for p in builtin() {
            let back = sidecar::settings_from_json(&sidecar::settings_to_json(&p.settings));
            assert_eq!(back, p.settings, "{}", p.name);
        }
    }
}
