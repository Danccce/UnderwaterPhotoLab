//! Rückgängig/Wiederholen je Bild (Strg+Z / Strg+Y), für die laufende Sitzung.

use std::time::{Duration, Instant};

use uwcc_core::{Field, Settings};

use crate::{controls, selection, store, with_state, LabWindow};

/// Höchstens so viele Schritte je Bild.
const LIMIT: usize = 100;
/// Änderungen desselben Reglers in diesem Abstand bilden einen Schritt (ein Zug am Regler).
const MERGE: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct History {
    undo: Vec<Settings>,
    redo: Vec<Settings>,
    last: Option<(Field, Instant)>,
}

impl History {
    /// Merkt `before` vor einer Änderung. `field` ist der bewegte Regler; ohne Regler (Auto,
    /// Zurücksetzen, Pipette, Übernahme) ist jede Änderung ein eigener Schritt.
    pub fn record(&mut self, before: Settings, field: Option<Field>) {
        let now = Instant::now();
        let merge = matches!((field, self.last), (Some(f), Some((l, t))) if f == l && now - t < MERGE);
        self.last = field.map(|f| (f, now));
        self.redo.clear();
        if !merge {
            self.undo.push(before);
            if self.undo.len() > LIMIT {
                self.undo.remove(0);
            }
        }
    }

    /// Voriger Stand, der sich von `current` unterscheidet; `current` kommt auf den Wiederholen-Stapel.
    fn undo(&mut self, current: Settings) -> Option<Settings> {
        self.last = None;
        while let Some(s) = self.undo.pop() {
            if s != current {
                self.redo.push(current);
                return Some(s);
            }
        }
        None
    }

    fn redo(&mut self, current: Settings) -> Option<Settings> {
        self.last = None;
        while let Some(s) = self.redo.pop() {
            if s != current {
                self.undo.push(current);
                return Some(s);
            }
        }
        None
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

pub fn undo(app: &LabWindow) {
    step(app, History::undo);
}

pub fn redo(app: &LabWindow) {
    step(app, History::redo);
}

fn step(app: &LabWindow, f: fn(&mut History, Settings) -> Option<Settings>) {
    let changed = with_state(|st| {
        let l = st.current.as_mut()?;
        let s = f(st.history.entry(l.path.clone()).or_default(), l.settings)?;
        l.settings = s;
        st.settings.insert(l.path.clone(), s);
        Some(())
    });
    if changed.is_some() {
        selection::forget_undo(app);
        store::save_soon(app);
    }
    controls::refresh(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_strength(v: f32) -> Settings {
        Settings { strength: v, ..Settings::default() }
    }

    #[test]
    fn one_drag_is_one_step_and_redo_follows_undo() {
        let mut h = History::default();
        h.record(with_strength(1.0), Some(Field::Strength));
        h.record(with_strength(0.9), Some(Field::Strength));
        h.record(with_strength(0.8), Some(Field::Strength));
        assert_eq!(h.undo(with_strength(0.7)), Some(with_strength(1.0)));
        assert_eq!(h.undo(with_strength(1.0)), None);
        assert_eq!(h.redo(with_strength(1.0)), Some(with_strength(0.7)));
    }

    #[test]
    fn other_field_or_button_starts_a_new_step_and_clears_redo() {
        let mut h = History::default();
        h.record(with_strength(1.0), Some(Field::Strength));
        h.record(with_strength(0.5), Some(Field::Vibrance));
        h.record(with_strength(0.4), None);
        assert_eq!(h.undo.len(), 3);
        h.undo(with_strength(0.3));
        assert!(h.can_redo());
        h.record(with_strength(0.4), None);
        assert!(!h.can_redo());
    }

    #[test]
    fn steps_without_change_are_skipped() {
        let mut h = History::default();
        h.record(with_strength(1.0), None);
        h.record(with_strength(0.5), None);
        assert_eq!(h.undo(with_strength(0.5)), Some(with_strength(1.0)));
    }
}
