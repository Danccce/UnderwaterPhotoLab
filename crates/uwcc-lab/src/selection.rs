//! Mehrfachauswahl in der Leiste und das Übernehmen von Einstellungen auf die Auswahl.

use std::path::PathBuf;

use slint::Model;

use crate::{select, store, with_state, LabWindow};

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

pub fn select_all(app: &LabWindow) {
    with_state(|st| st.selection = (0..st.files.len()).collect());
    sync(app);
}

pub fn clear(app: &LabWindow) {
    single(app, app.get_current());
}

/// Überträgt die Einstellungen des aktuellen Bildes auf die übrigen ausgewählten Bilder.
pub fn transfer(app: &LabWindow, mode: Transfer) {
    let count = with_state(|st| {
        let l = st.current.as_ref()?;
        let source = match mode {
            Transfer::Fixed => l.settings.fixed(&l.auto),
            Transfer::Relative => l.settings,
        };
        let targets: Vec<PathBuf> = st
            .selection
            .iter()
            .filter_map(|&i| st.files.get(i))
            .filter(|p| **p != l.path)
            .cloned()
            .collect();
        let undo = targets.iter().map(|p| (p.clone(), st.settings.insert(p.clone(), source))).collect();
        st.undo = Some(undo);
        Some(targets.len())
    });
    let Some(count) = count else { return };
    app.set_can_undo_transfer(count > 0);
    let how = match mode {
        Transfer::Fixed => "gleiche Werte",
        Transfer::Relative => "Auto + Abweichung",
    };
    app.set_status(format!("Einstellungen auf {count} Bilder übernommen ({how}).").into());
    store::refresh_marks(app);
    store::save_soon(app);
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
        Some(undo.len())
    });
    app.set_can_undo_transfer(false);
    if let Some(n) = restored {
        app.set_status(format!("Übernahme rückgängig gemacht ({n} Bilder).").into());
        store::refresh_marks(app);
        store::save_soon(app);
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