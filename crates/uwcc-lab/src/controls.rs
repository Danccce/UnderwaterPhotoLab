//! Regler ↔ `Settings` des gewählten Bildes, über `Settings::effective` / `set_effective`.

use std::{cell::RefCell, time::Duration};

use slint::{ComponentHandle, Timer, TimerMode};
use uwcc_core::{Field, Settings};

use crate::{with_state, LabWindow};

thread_local! {
    // Fasst schnelle Regleränderungen zu einer Neuberechnung pro Frame zusammen.
    static RENDER: RefCell<Timer> = RefCell::new(Timer::default());
}

/// Ein Regler wurde bewegt; `value` ist der angezeigte (wirksame) Wert.
pub fn edit(app: &LabWindow, field: &str, value: f32) {
    if let Some(field) = Field::from_name(field) {
        update(app, Some(field), |s, auto| s.set_effective(field, value as f64, auto));
    }
}

pub fn set_auto(app: &LabWindow, on: bool) {
    update(app, None, |s, _| s.auto = on);
}

pub fn reset(app: &LabWindow) {
    update(app, None, |s, _| *s = Settings::default());
}

/// Doppelklick auf einen Regler: nur dieses Feld zurücksetzen.
pub fn reset_field(app: &LabWindow, field: &str) {
    if let Some(field) = Field::from_name(field) {
        update(app, None, |s, auto| s.reset_field(field, auto));
    }
}

/// Pipette: `(fx, fy)` als Anteil der Bildbreite und -höhe (0–1); setzt Temperatur und Tint so,
/// dass die Stelle neutral grau wird.
pub fn pick(app: &LabWindow, fx: f32, fy: f32) {
    let sample = with_state(|st| {
        let img = &st.current.as_ref()?.original;
        if !(0.0..1.0).contains(&fx) || !(0.0..1.0).contains(&fy) {
            return None;
        }
        Some(average(img, (fx * img.width() as f32) as u32, (fy * img.height() as f32) as u32))
    });
    let Some((r, g, b)) = sample else {
        app.set_status("Pipette: bitte auf eine Stelle im Bild klicken.".into());
        return;
    };
    update(app, None, |s, auto| {
        (s.temperature, s.tint) = s.white_balance_from_sample(auto, r, g, b);
    });
    let s = app.get_v_temperature();
    app.set_status(format!("Pipette: Temperatur {s:+.0}, Grün ↔ Magenta {:+.0}", app.get_v_tint()).into());
}

/// Mittelwert eines 5 × 5-Feldes um `(x, y)`, als 0–1.
fn average(img: &uwcc_photo::RgbaImage, x: u32, y: u32) -> (f32, f32, f32) {
    let (w, h) = img.dimensions();
    let (mut sum, mut n) = ([0f32; 3], 0f32);
    for yy in y.saturating_sub(2)..(y + 3).min(h) {
        for xx in x.saturating_sub(2)..(x + 3).min(w) {
            let p = img.get_pixel(xx, yy);
            for (s, v) in sum.iter_mut().zip(p.0) {
                *s += v as f32 / 255.0;
            }
            n += 1.0;
        }
    }
    (sum[0] / n, sum[1] / n, sum[2] / n)
}

/// Ändert die Einstellungen des aktuellen Bildes und trägt den vorigen Stand in dessen Verlauf
/// ein; `field` ist der bewegte Regler (siehe [`crate::history::History::record`]).
fn update(app: &LabWindow, field: Option<Field>, f: impl FnOnce(&mut Settings, &uwcc_core::Correction)) {
    let changed = with_state(|st| {
        let l = st.current.as_mut()?;
        let before = l.settings;
        f(&mut l.settings, &l.auto);
        if l.settings == before {
            return Some(false);
        }
        st.history.entry(l.path.clone()).or_default().record(before, field);
        st.settings.insert(l.path.clone(), l.settings);
        Some(true)
    });
    if changed == Some(true) {
        crate::selection::forget_undo(app);
        crate::store::save_soon(app);
    }
    if changed.is_some() {
        refresh(app);
    }
}

/// Schreibt die wirksamen Werte in die Regler, markiert das Bild in der Leiste und plant die
/// Neuberechnung der Vorschau.
pub fn refresh(app: &LabWindow) {
    let Some((s, values, undo, redo)) = with_state(|st| {
        let l = st.current.as_ref()?;
        let h = st.history.get(&l.path);
        Some((
            l.settings,
            Field::ALL.map(|f| l.settings.effective(f, &l.auto) as f32),
            h.is_some_and(|h| h.can_undo()),
            h.is_some_and(|h| h.can_redo()),
        ))
    }) else {
        return;
    };
    app.set_can_undo(undo);
    app.set_can_redo(redo);
    app.set_auto_on(s.auto);
    for (field, v) in Field::ALL.into_iter().zip(values) {
        match field {
            Field::Strength => app.set_v_strength(v),
            Field::Temperature => app.set_v_temperature(v),
            Field::Tint => app.set_v_tint(v),
            Field::Vibrance => app.set_v_vibrance(v),
            Field::RedLimit => app.set_v_red_limit(v),
            Field::HueShift => app.set_v_hue(v),
            // Die Levels je Kanal stellt nur die Auto-Analyse ein; im Fenster gibt es dafür keine Regler.
            Field::RedLow | Field::RedHigh | Field::GreenLow | Field::GreenHigh | Field::BlueLow | Field::BlueHigh => {}
        }
    }

    crate::store::refresh_marks(app);

    let weak = app.as_weak();
    RENDER.with(|t| {
        let t = t.borrow();
        if !t.running() {
            t.start(TimerMode::SingleShot, Duration::from_millis(16), move || {
                if let Some(app) = weak.upgrade() {
                    crate::histogram::show_corrected(&app);
                }
            });
        }
    });
}
