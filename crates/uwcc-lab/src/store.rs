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
        app.invoke_notify(format!("Einstellungen nicht gespeichert: {e}").into());
    }
}

/// Orangefarbener Punkt (bearbeitet) und Haken (aktuell exportiert) für alle Bilder, dazu die
/// Plätze in der gefilterten Leiste (Filter „Unbearbeitet“ bzw. „Nicht exportiert“). Das aktuelle
/// Bild bleibt immer sichtbar, sonst verschwände es beim ersten Regler aus „Unbearbeitet“.
pub fn refresh_marks(app: &LabWindow) {
    let filter = app.get_strip_filter();
    let current = app.get_current();
    let marks: Vec<(bool, bool)> = with_state(|st| {
        st.files
            .iter()
            .map(|f| (settings_of(st, f) != Settings::default(), is_exported(st, f)))
            .collect()
    });
    let model = app.get_thumbs();
    let (mut shown, mut current_slot) = (0, -1);
    for (i, (edited, exported)) in marks.into_iter().enumerate() {
        let is_current = i as i32 == current;
        let visible = is_current
            || match filter {
                1 => !edited,
                2 => !exported,
                _ => true,
            };
        let slot = if visible {
            shown += 1;
            shown - 1
        } else {
            -1
        };
        if is_current {
            current_slot = slot;
        }
        if let Some(mut row) = model.row_data(i) {
            if row.edited != edited || row.exported != exported || row.slot != slot {
                row.edited = edited;
                row.exported = exported;
                row.slot = slot;
                model.set_row_data(i, row);
            }
        }
    }
    app.set_shown_count(shown);
    app.set_current_slot(current_slot);
}

/// Index des angezeigten Bildes `step` Plätze weiter (negativ: zurück) ab `from`; `None`, wenn es
/// so viele nicht mehr gibt (z. B. Pfeil nach unten in der letzten Zeile des Rasters).
pub fn next_shown(app: &LabWindow, from: i32, step: i32) -> Option<i32> {
    let model = app.get_thumbs();
    let (mut i, mut left) = (from, step.abs());
    while left > 0 {
        i += step.signum();
        if model.row_data(usize::try_from(i).ok()?)?.slot >= 0 {
            left -= 1;
        }
    }
    (i != from).then_some(i)
}

/// Indizes der angezeigten Bilder.
pub fn shown_indices(app: &LabWindow) -> Vec<usize> {
    app.get_thumbs().iter().enumerate().filter(|(_, t)| t.slot >= 0).map(|(i, _)| i).collect()
}
