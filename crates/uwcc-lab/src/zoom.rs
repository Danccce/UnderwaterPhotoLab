//! Zoom-Ansicht: rechnet den sichtbaren Ausschnitt des ganzen Bildes im Hintergrund.
//!
//! Es läuft höchstens eine Berechnung; Anfragen währenddessen (Ziehen, Zoomen, Regler) führen
//! danach zu genau einer weiteren mit dem neuesten Stand. Slint verschiebt und skaliert das letzte
//! Ergebnis sofort mit.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::Arc,
    thread,
};

use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};
use uwcc_core::{Correction, Geometry, Settings};
use uwcc_filters::geometry;
use uwcc_photo::RgbaImage;

use crate::{histogram, preview, with_state, LabWindow};

thread_local! {
    static BUSY: Cell<bool> = const { Cell::new(false) };
    static AGAIN: Cell<bool> = const { Cell::new(false) };
    /// Geometrie, zu der die Maße der Zoom-Ansicht gesetzt sind.
    static SHOWN: RefCell<Option<Geometry>> = const { RefCell::new(None) };
}

struct Job {
    path: PathBuf,
    /// Ganzes Bild nach der Geometrie, sofern schon gerechnet; sonst rechnet der Job es aus `upright`.
    full: Option<Arc<RgbaImage>>,
    upright: Arc<RgbaImage>,
    geometry: Geometry,
    reference: Arc<uwcc_filters::Reference>,
    settings: Settings,
    auto: Correction,
    warning: bool,
    /// Ausschnitt in Bildpixeln.
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// Bildschirmpixel je Bildpixel; unter 1 wird der Ausschnitt verkleinert gerechnet.
    scale: f32,
}

impl Job {
    /// Original und Korrektur des Ausschnitts. Die Pipeline sieht das ganze Bild in der gerade
    /// gerechneten Auflösung, damit relative Radien wie beim Export wirken, und rechnet rundum
    /// einen Rand mit, damit Filter mit Nachbarschaft am Rand des Ausschnitts richtig wirken.
    /// Dazu das ganze Bild nach der Geometrie, falls es dafür erst gerechnet wurde.
    fn run(&self) -> (SharedPixelBuffer<Rgba8Pixel>, SharedPixelBuffer<Rgba8Pixel>, Option<Arc<RgbaImage>>) {
        let computed = self.full.is_none().then(|| preview::transformed(&self.upright, &self.geometry, &RefCell::new(None)));
        let full = self.full.as_ref().or(computed.as_ref()).expect("eins von beiden");
        let (fw, fh) = full.dimensions();
        let s = self.scale.min(1.0);
        let size = |v: u32| ((v as f32 * s).round() as u32).max(1);
        let ctx = uwcc_filters::Context::new(self.auto, self.reference.clone(), size(fw), size(fh)).with_scale(s);
        let margin = (uwcc_filters::margin_px(&self.settings, &ctx) as f32 / s).ceil() as u32;
        let (ex, ey) = (self.x.saturating_sub(margin), self.y.saturating_sub(margin));
        let ew = (self.x + self.width + margin).min(fw) - ex;
        let eh = (self.y + self.height + margin).min(fh) - ey;
        let (ow, oh) = (size(ew), size(eh));
        let mut extended = uwcc_photo::region(full, (ex, ey, ew, eh), (ow, oh));
        let original_ext = extended.clone();
        let ctx = uwcc_filters::Context { origin: ((ex as f32 * s).round() as u32, (ey as f32 * s).round() as u32), ..ctx };
        uwcc_filters::render(&mut extended, ow, oh, &self.settings, &ctx);

        // Den sichtbaren Teil herausschneiden.
        let ix = (((self.x - ex) as f32 * s).round() as u32).min(ow - 1);
        let iy = (((self.y - ey) as f32 * s).round() as u32).min(oh - 1);
        let (iw, ih) = (size(self.width).min(ow - ix), size(self.height).min(oh - iy));
        let cut = |img: &RgbaImage| uwcc_photo::region(img, (ix, iy, iw, ih), (iw, ih));
        let mut corrected = cut(&extended);
        if self.warning {
            histogram::mark_clipping(&mut corrected);
        }
        (
            SharedPixelBuffer::clone_from_slice(cut(&original_ext).as_raw(), iw, ih),
            SharedPixelBuffer::clone_from_slice(corrected.as_raw(), iw, ih),
            computed,
        )
    }
}

/// Der sichtbare Ausschnitt um die Mitte `zoom-cx/-cy` beim Maßstab `zoom-scale`, innerhalb des
/// Bildes; ein Pixel Rand, damit beim Verschieben keine Lücke aufblitzt.
fn job(app: &LabWindow) -> Option<Job> {
    let (path, upright, full, reference, settings, auto, (fw, fh)) = with_state(|st| {
        st.current.as_ref().map(|l| {
            let g = l.settings.geometry;
            let full = if geometry::is_identity(&g) {
                Some(l.full.clone())
            } else {
                l.full_view.borrow().as_ref().filter(|(cg, _)| *cg == g).map(|(_, img)| img.clone())
            };
            (l.path.clone(), l.full.clone(), full, l.reference.clone(), l.settings, l.auto, l.output_size())
        })
    })?;
    let geometry = settings.geometry;
    let scale = app.get_zoom_scale().max(0.01);
    let span = |view: f32, center: f32, size: u32| {
        let len = ((view / scale).ceil().max(1.0) as u32 + 2).min(size);
        let start = (center - len as f32 / 2.0).round().clamp(0.0, (size - len) as f32) as u32;
        (start, len)
    };
    let (x, width) = span(app.get_view_w(), app.get_zoom_cx(), fw);
    let (y, height) = span(app.get_view_h(), app.get_zoom_cy(), fh);
    Some(Job {
        path,
        full,
        upright,
        geometry,
        reference,
        settings,
        auto,
        warning: app.get_clip_warning(),
        x,
        y,
        width,
        height,
        scale,
    })
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
        let (original, corrected, computed) = job.run();
        let _ = weak.upgrade_in_event_loop(move |app| {
            BUSY.set(false);
            let current = with_state(|st| {
                let l = st.current.as_ref().filter(|l| l.path == job.path)?;
                if let Some(img) = computed {
                    if l.settings.geometry == job.geometry {
                        *l.full_view.borrow_mut() = Some((job.geometry, img));
                    }
                }
                Some(())
            })
            .is_some();
            if current && app.get_zoomed() {
                app.set_zoom_original(Image::from_rgba8(original));
                app.set_zoom_corrected(Image::from_rgba8(corrected));
                app.set_zoom_x(job.x as f32);
                app.set_zoom_y(job.y as f32);
                app.set_zoom_w(job.width as f32);
                app.set_zoom_h(job.height as f32);
            }
            if AGAIN.replace(false) {
                request(&app);
            }
        });
    });
}

/// Geometrie geändert (Drehen, Geraderichten, Zuschnitt): Maße wie bei einem neuen Bild setzen.
pub fn geometry_changed(app: &LabWindow, g: Geometry, size: (u32, u32)) {
    if SHOWN.with(|s| s.borrow().as_ref() != Some(&g)) {
        new_image(app, Some(size));
        SHOWN.with(|s| *s.borrow_mut() = Some(g));
    }
}

/// Neues Bild: alten Ausschnitt verwerfen und die Mitte an das neue Bild anpassen; `size` ist die
/// Größe des Ergebnisses in voller Auflösung (nach der Geometrie).
pub fn new_image(app: &LabWindow, size: Option<(u32, u32)>) {
    app.set_zoom_original(Image::default());
    app.set_zoom_corrected(Image::default());
    SHOWN.with(|s| *s.borrow_mut() = None);
    if let Some((w, h)) = size {
        let (w, h) = (w as f32, h as f32);
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
