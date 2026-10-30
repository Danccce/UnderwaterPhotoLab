//! Mehrfachauswahl in der Leiste und das Übernehmen von Einstellungen auf die Auswahl.

use std::path::PathBuf;

use slint::Model;
use uwcc_core::{Groups, Settings};

use crate::{controls, select, store, with_state, LabWindow, State};

#[derive(Clone, Copy)]
pub enum Transfer {
    /// Jedes Bild bekommt genau dieselbe Korrektur.
    Fixed,
    /// Jedes Bild behält seine Auto-Analyse, Abweichung und übrige Regler kommen dazu.
    Relative,
}

/// Klick in der Leiste: ohne Taste nur dieses Bild, Strg schaltet ein Bild dazu oder weg,
/// Umschalt wählt den Bereich bis zum aktuellen Bild. Das aktuelle Bild bleibt immer dabei.
pub fn press(app: &LabWindow, index: i32, ctrl: bool, shift: bool) {
    let Ok(i) = usize::try_from(index) else { return };
    let current = usize::try_from(app.get_current()).ok();
    let open = with_state(|st| match (current, ctrl, shift) {
        (Some(c), _, true) => {
            if !ctrl {
                st.selection.clear();
            }
            st.selection.extend(c.min(i)..=c.max(i));
            false
        }
        (Some(c), true, false) => {
            if i != c && !st.selection.remove(&i) {
                st.selection.insert(i);
            }
            st.selection.insert(c);
            false
        }
        _ => {
            st.selection.clear();
            st.selection.insert(i);
            true
        }
    });
    if open {
        select(app, index);
    }
    sync(app);
}

/// Nur noch `index` ausgewählt (Pfeiltasten, neuer Ordner).
pub fn single(app: &LabWindow, index: i32) {
    with_state(|st| {
        st.selection.clear();
        if let Ok(i) = usize::try_from(index) {
            st.selection.insert(i);
        }
    });
    sync(app);
}

/// Alle angezeigten Bilder (mit Filter nur diese).
pub fn select_all(app: &LabWindow) {
    let shown = store::shown_indices(app);
    with_state(|st| st.selection = shown.into_iter().collect());
    sync(app);
}

pub fn clear(app: &LabWindow) {
    single(app, app.get_current());
}

/// Die beim Übertragen gewählten Gruppen (Schalter unter „Mehrere Bilder“).
pub fn groups(app: &LabWindow) -> Groups {
    Groups {
        color: app.get_transfer_color(),
        tone: app.get_transfer_tone(),
        detail: app.get_transfer_detail(),
        geometry: app.get_transfer_geometry(),
    }
}

/// Überträgt die gewählten Gruppen des aktuellen Bildes auf die übrigen ausgewählten Bilder.
pub fn transfer(app: &LabWindow, mode: Transfer) {
    let groups = groups(app);
    let Some((source, targets)) = with_state(|st| {
        let l = st.current.as_ref()?;
        let source = match mode {
            Transfer::Fixed => l.settings.fixed(&l.auto),
            Transfer::Relative => l.settings,
        };
        let targets: Vec<PathBuf> =
            st.selection.iter().filter_map(|&i| st.files.get(i)).filter(|p| **p != l.path).cloned().collect();
        Some((source, targets))
    }) else {
        return;
    };
    let count = apply(app, &targets, |s| s.with_groups(&source, groups));
    let how = match mode {
        Transfer::Fixed => "gleiche Werte",
        Transfer::Relative => "Auto + Abweichung",
    };
    app.invoke_notify(format!("Einstellungen auf {count} Bilder übernommen ({how}).").into());
}

/// Setzt für `targets` die Einstellungen `f(bisherige)`. Der vorige Stand kommt in den Verlauf
/// jedes Bildes und in „Letzte Übernahme rückgängig“; gehört das aktuelle Bild dazu, folgt die
/// Anzeige. Gibt die Anzahl zurück.
pub fn apply(app: &LabWindow, targets: &[PathBuf], f: impl Fn(&Settings) -> Settings) -> usize {
    let current_changed = with_state(|st| {
        let undo: Vec<_> = targets
            .iter()
            .map(|p| {
                let before = st.settings.get(p).copied();
                st.settings.insert(p.clone(), f(&before.unwrap_or_default()));
                (p.clone(), before)
            })
            .collect();
        for (path, previous) in &undo {
            st.history.entry(path.clone()).or_default().record(previous.unwrap_or_default(), None);
        }
        st.undo = Some(undo);
        follow_current(st)
    });
    app.set_can_undo_transfer(!targets.is_empty());
    store::refresh_marks(app);
    store::save_soon(app);
    if current_changed {
        controls::refresh(app);
    }
    targets.len()
}

/// Gleicht die Einstellungen des geladenen Bildes an `st.settings` an; `true`, wenn sie sich ändern.
fn follow_current(st: &mut State) -> bool {
    let Some(l) = st.current.as_mut() else { return false };
    let s = st.settings.get(&l.path).copied().unwrap_or_default();
    let changed = l.settings != s;
    l.settings = s;
    changed
}

/// Macht die letzte Übernahme rückgängig.
pub fn undo(app: &LabWindow) {
    let restored = with_state(|st| {
        let undo = st.undo.take()?;
        for (path, previous) in &undo {
            match previous {
                Some(s) => st.settings.insert(path.clone(), *s),
                None => st.settings.remove(path),
            };
        }
        Some((undo.len(), follow_current(st)))
    });
    app.set_can_undo_transfer(false);
    if let Some((n, current_changed)) = restored {
        app.invoke_notify(format!("Übernahme rückgängig gemacht ({n} Bilder).").into());
        store::refresh_marks(app);
        store::save_soon(app);
        if current_changed {
            controls::refresh(app);
        }
    }
}

/// Nach einer Änderung am aktuellen Bild würde „Rückgängig“ sie überschreiben; daher verwerfen.
pub fn forget_undo(app: &LabWindow) {
    with_state(|st| st.undo = None);
    app.set_can_undo_transfer(false);
}

/// Auswahl-Markierung der Leiste und Anzahl an den Zustand angleichen.
fn sync(app: &LabWindow) {
    let (flags, count) = with_state(|st| {
        let multi = st.selection.len() > 1;
        ((0..st.files.len()).map(|i| multi && st.selection.contains(&i)).collect::<Vec<_>>(), st.selection.len())
    });
    let model = app.get_thumbs();
    for (i, selected) in flags.into_iter().enumerate() {
        if let Some(mut row) = model.row_data(i) {
            if row.selected != selected {
                row.selected = selected;
                model.set_row_data(i, row);
            }
        }
    }
    app.set_selected_count(count as i32);
}