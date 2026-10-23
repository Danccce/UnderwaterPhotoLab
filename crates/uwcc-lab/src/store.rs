//! Einstellungen und Export-Stand des Ordners: aus `.uwcc.json` laden, verzögert speichern und
//! die Markierungen der Leiste (bearbeitet, exportiert) aktuell halten.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    time::Duration,
};

use slint::{ComponentHandle, Model, Timer, TimerMode};
use uwcc_core::Settings;
use uwcc_photo::sidecar::{self, Entry, Exported, Sidecar};

use crate::{with_state, LabWindow, State};

thread_local! {
    // Fasst Änderungen zusammen; gespeichert wird kurz nach der letzten.
    static SAVE: RefCell<Timer> = RefCell::new(Timer::default());
}

pub fn settings_of(st: &State, path: &Path) -> Settings {
    st.settings.get(path).copied().unwrap_or_default()
}

/// Ist der letzte Export dieses Bildes noch aktuell (gleiche Einstellungen, Ziel, Qualität)?
pub fn is_exported(st: &State, path: &Path) -> bool {
    let wanted = Exported { settings: settings_of(st, path), target: crate::EXPORT_TARGET, quality: st.config.jpeg_quality };
    st.exported.get(path) == Some(&wanted)
}

/// Übernimmt `.uwcc.json` des Ordners; Export-Einträge ohne vorhandene Datei werden verworfen.
pub fn load(st: &mut State, dir: &Path, files: &[PathBuf]) {
    let sc = sidecar::load(dir);
    for f in files {
        let Some(e) = sc.entries.get(&uwcc_photo::file_name(f)) else { continue };
        if let Some(s) = e.settings {
            st.settings.insert(f.clone(), s);
        }
        if let Some(x) = e.exported {
            if uwcc_photo::output_path(f, x.target).exists() {
                st.exported.insert(f.clone(), x);
            }
        }
    }
}

pub fn save_soon(app: &LabWindow) {
    let weak = app.as_weak();
    SAVE.with(|t| {
        t.borrow().start(TimerMode::SingleShot, Duration::from_millis(800), move || {
            if let Some(app) = weak.upgrade() {
                save_now(&app);
            }
        });
    });
}

/// Schreibt `.uwcc.json` sofort. Legt `uw-korrigiert\` nicht an, solange es nichts zu speichern gibt.
pub fn save_now(app: &LabWindow) {
    SAVE.with(|t| t.borrow().stop());
    let result = with_state(|st| {
        let dir = st.dir.clone()?;
        let mut sc = Sidecar::default();
        for f in &st.files {
            let settings = st.settings.get(f).copied().filter(|s| *s != Settings::default());
            let exported = st.exported.get(f).copied();
            if settings.is_some() || exported.is_some() {
                sc.entries.insert(uwcc_photo::file_name(f), Entry { settings, exported });
            }
        }
        if sc.entries.is_empty() && !sidecar::path(&dir).exists() {
            return None;
        }
        Some(sidecar::save(&dir, &sc))
    });
    if let Some(Err(e)) = result {
        app.set_status(format!("Einstellungen nicht gespeichert: {e}").into());
    }
}

/// Orangefarbener Punkt (bearbeitet) und Haken (aktuell exportiert) für alle Bilder.
pub fn refresh_marks(app: &LabWindow) {
    let marks: Vec<(bool, bool)> = with_state(|st| {
        st.files
            .iter()
            .map(|f| (settings_of(st, f) != Settings::default(), is_exported(st, f)))
            .collect()
    });
    let model = app.get_thumbs();
    for (i, (edited, exported)) in marks.into_iter().enumerate() {
        if let Some(mut row) = model.row_data(i) {
            if row.edited != edited || row.exported != exported {
                row.edited = edited;
                row.exported = exported;
                model.set_row_data(i, row);
            }
        }
    }
}
