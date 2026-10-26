//! Zwei-Finger-Geste auf dem Touchbildschirm: auseinanderziehen zoomt, gemeinsam schieben
//! verschiebt. Slint macht aus Berührungen nur einen Mauszeiger und kennt keine Gesten.

use std::collections::BTreeMap;

use slint::{
    winit_030::{
        winit::event::{Touch, TouchPhase},
        EventResult,
    },
    ComponentHandle,
};

use crate::LabWindow;

#[derive(Default)]
pub struct Touches {
    /// Finger-ID → Position in physischen Pixeln.
    points: BTreeMap<u64, (f64, f64)>,
    /// Ab dem zweiten Finger bis zum Abheben des letzten: Slint sieht die Berührungen nicht,
    /// sonst springt der Mauszeiger zwischen den Fingern hin und her.
    gesture: bool,
    /// Abstand und Mittelpunkt der ersten beiden Finger bei der letzten Bewegung.
    last: Option<(f64, f64, f64)>,
}

impl Touches {
    fn measure(&self) -> Option<(f64, f64, f64)> {
        let mut it = self.points.values();
        let (&(x1, y1), &(x2, y2)) = (it.next()?, it.next()?);
        Some((((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt().max(1.0), (x1 + x2) / 2.0, (y1 + y2) / 2.0))
    }

    pub fn handle(&mut self, app: &LabWindow, touch: &Touch) -> EventResult {
        let pos = (touch.location.x, touch.location.y);
        match touch.phase {
            TouchPhase::Started => {
                self.points.insert(touch.id, pos);
                if self.points.len() >= 2 {
                    self.gesture = true;
                    self.last = self.measure();
                }
            }
            TouchPhase::Moved => {
                self.points.insert(touch.id, pos);
                if let (Some((d0, x0, y0)), Some((d, x, y))) = (self.last, self.measure()) {
                    let s = app.window().scale_factor() as f64;
                    app.invoke_touch_gesture(
                        (d / d0) as f32,
                        (x / s) as f32,
                        (y / s) as f32,
                        ((x - x0) / s) as f32,
                        ((y - y0) / s) as f32,
                    );
                    self.last = Some((d, x, y));
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.points.remove(&touch.id);
                self.last = None;
                if self.points.len() >= 2 {
                    self.last = self.measure();
                }
                // Das Abheben des letzten Fingers geht an Slint, damit ein begonnenes Ziehen endet.
                if self.points.is_empty() && self.gesture {
                    self.gesture = false;
                    return EventResult::Propagate;
                }
            }
        }
        if self.gesture {
            EventResult::PreventDefault
        } else {
            EventResult::Propagate
        }
    }
}
