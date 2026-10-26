//! 100-%-Ansicht: rechnet den sichtbaren Ausschnitt des ganzen Bildes im Hintergrund.
//!
//! Es läuft höchstens eine Berechnung; Anfragen währenddessen (Ziehen, Regler) führen danach zu
//! genau einer weiteren mit dem neuesten Stand. Slint verschiebt das letzte Ergebnis sofort mit.

use std::{cell::Cell, path::PathBuf, sync::Arc, thread};

use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use uwcc_core::{Correction, Settings};
use uwcc_photo::RgbaImage;

use crate::{histogram, with_state, LabWindow};

thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
    static AGAIN: Cell<bool> = const { Cell::new(false) };
}

struct Job {
    path: PathBuf,
    full: Arc<RgbaImage>,
    settings: Settings,
    auto: Correction,
    warning: bool,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Job {
    /// Original und Korrektur des Ausschnitts. Die Pipeline sieht die Maße des ganzen Bildes,
    /// damit relative Radien wie beim Export wirken.
    fn run(&self) -> (SharedPixelBuffer<Rgba8Pixel>, SharedPixelBuffer<Rgba8Pixel>) {
        let (fw, fh) = self.full.dimensions();
        let row = self.width as usize * 4;
        let mut crop = Vec::with_capacity(row * self.height as usize);
        for y in self.y..self.y + self.height {
            let start = (y as usize * fw as usize + self.x as usize) * 4;
            crop.extend_from_slice(&self.full.as_raw()[start..start + row]);
        }
        let original = SharedPixelBuffer::clone_from_slice(&crop, self.width, self.height);
        let ctx = uwcc_filters::Context::new(self.auto, fw, fh);
        uwcc_filters::render(&mut crop, self.width, self.height, &self.settings, &ctx);
        if self.warning {
            histogram::mark_clipping(&mut crop);
        }
        (original, SharedPixelBuffer::clone_from_slice(&crop, self.width, self.height))
    }
}

/// Der sichtbare Ausschnitt um die Mitte `zoom-cx/-cy`, so groß wie das Vorschaufeld in
/// physischen Pixeln und innerhalb des Bildes.
fn job(app: &LabWindow) -> Option<Job> {
    let (path, full, settings, auto) =
        with_state(|st| st.current.as_ref().map(|l| (l.path.clone(), l.full.clone(), l.settings, l.auto)))?;
    let (fw, fh) = full.dimensions();
    let span = |view: f32, center: f32, size: u32| {
        let len = (view.ceil().max(1.0) as u32).min(size);
        let start = (center - len as f32 / 2.0).round().clamp(0.0, (size - len) as f32) as u32;
        (start, len)
    };
    let (x, width) = span(app.get_view_w(), app.get_zoom_cx(), fw);
    let (y, height) = span(app.get_view_h(), app.get_zoom_cy(), fh);
    Some(Job { path, full, settings, auto, warning: app.get_clip_warning(), x, y, width, height })
}

pub fn request(app: &LabWindow) {
    if !app.get_zoomed() {
        return;
    }
    if BUSY.get() {
        AGAIN.set(true);
        return;
    }
    let Some(job) = job(app) else { return };
    BUSY.set(true);
    let weak = app.as_weak();
    thread::spawn(move || {
        let (original, corrected) = job.run();
        let _ = weak.upgrade_in_event_loop(move |app| {
            BUSY.set(false);
            let current = with_state(|st| st.current.as_ref().is_some_and(|l| l.path == job.path));
            if current && app.get_zoomed() {
                app.set_zoom_original(Image::from_rgba8(original));
                app.set_zoom_corrected(Image::from_rgba8(corrected));
                app.set_zoom_x(job.x as f32);
                app.set_zoom_y(job.y as f32);
            }
            if AGAIN.replace(false) {
                request(&app);
            }
        });
    });
}

/// Neues Bild: alten Ausschnitt verwerfen und die Mitte an das neue Bild anpassen.
pub fn new_image(app: &LabWindow, full: Option<&RgbaImage>) {
    app.set_zoom_original(Image::default());
    app.set_zoom_corrected(Image::default());
    if let Some(full) = full {
        let (w, h) = (full.width() as f32, full.height() as f32);
        app.set_full_width(w);
        app.set_full_height(h);
        if app.get_center_on_load() {
            app.set_center_on_load(false);
            app.invoke_set_center(w / 2.0, h / 2.0);
        } else {
            app.invoke_set_center(app.get_zoom_cx(), app.get_zoom_cy());
        }
    }
}
