//! Lädt das gewählte Bild im Hintergrund und rechnet die Vorschau.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};

use std::cell::RefCell;

use slint::{Rgba8Pixel, SharedPixelBuffer, Weak};
use uwcc_core::{Correction, Geometry, Settings};
use uwcc_filters::geometry;
use uwcc_photo::{Photo, RgbaImage};

use crate::LabWindow;

/// Längste Kante der Vorschau; der Export rechnet in voller Auflösung.
pub const PREVIEW_MAX: u32 = 1280;

/// Ein Bild nach einer Geometrie, gemerkt, bis sich die Geometrie ändert.
pub type Cached = RefCell<Option<(Geometry, Arc<RgbaImage>)>>;

pub struct Loaded {
    pub path: PathBuf,
    /// Ganzes Bild, aufrecht und unbearbeitet.
    pub full: Arc<RgbaImage>,
    /// Für Dehaze; dieselbe wie beim Export ([`Photo::reference`]).
    pub reference: Arc<uwcc_filters::Reference>,
    /// Unbearbeitete, aufrechte Vorschau.
    pub original: Arc<RgbaImage>,
    /// Auto-Analyse des ganzen Bildes in voller Auflösung.
    pub auto: Correction,
    pub settings: Settings,
    /// Vorschau nach der Geometrie.
    view: Cached,
    /// Ganzes Bild nach der Geometrie, für die Zoom-Ansicht.
    pub full_view: Cached,
}

/// `img` nach `g`, aus `cache` oder neu gerechnet.
pub fn transformed(img: &Arc<RgbaImage>, g: &Geometry, cache: &Cached) -> Arc<RgbaImage> {
    if geometry::is_identity(g) {
        return img.clone();
    }
    if let Some((cg, cached)) = cache.borrow().as_ref() {
        if cg == g {
            return cached.clone();
        }
    }
    let (px, w, h) = geometry::transform(img.as_raw(), img.width(), img.height(), g);
    let out = Arc::new(RgbaImage::from_raw(w, h, px).expect("Puffergröße passt"));
    *cache.borrow_mut() = Some((*g, out.clone()));
    out
}

impl Loaded {
    /// Geometrie der Anzeige; beim Zuschneiden ohne Zuschnitt (der Rahmen wird darübergelegt).
    pub fn view_geometry(&self, cropping: bool) -> Geometry {
        let mut g = self.settings.geometry;
        if cropping {
            g.crop = None;
        }
        g
    }

    /// Unbearbeitete Vorschau nach der Geometrie.
    pub fn view(&self, cropping: bool) -> Arc<RgbaImage> {
        transformed(&self.original, &self.view_geometry(cropping), &self.view)
    }

    /// Pixel der Vorschau je Pixel des Originals.
    pub fn preview_scale(&self) -> f32 {
        self.original.width() as f32 / self.full.width().max(1) as f32
    }

    /// Größe des Ergebnisses in voller Auflösung (nach Drehen und Zuschnitt).
    pub fn output_size(&self) -> (u32, u32) {
        geometry::output_size(self.full.width(), self.full.height(), &self.settings.geometry)
    }

    /// Größe des Rahmens in voller Auflösung (gedreht, ohne Zuschnitt).
    pub fn frame_size(&self) -> (u32, u32) {
        geometry::frame_size(self.full.width(), self.full.height(), &self.settings.geometry)
    }

    pub fn original_buffer(&self, cropping: bool) -> SharedPixelBuffer<Rgba8Pixel> {
        buffer(&self.view(cropping))
    }

    pub fn corrected_buffer(&self, cropping: bool) -> SharedPixelBuffer<Rgba8Pixel> {
        let view = self.view(cropping);
        let mut buf = buffer(&view);
        let (w, h) = view.dimensions();
        let ctx = uwcc_filters::Context::new(self.auto, self.reference.clone(), w, h).with_scale(self.preview_scale());
        uwcc_filters::render(buf.make_mut_bytes(), w, h, &self.settings, &ctx);
        buf
    }
}

fn buffer(img: &RgbaImage) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::clone_from_slice(img.as_raw(), img.width(), img.height())
}

/// Öffnet `path` in einem eigenen Thread und ruft `done` im Event-Loop auf, sofern `current`
/// dann noch `generation` ist (sonst wurde inzwischen ein anderes Bild gewählt).
pub fn load(
    weak: Weak<LabWindow>,
    path: PathBuf,
    current: Arc<AtomicU64>,
    generation: u64,
    done: fn(&LabWindow, Result<Loaded, String>),
) {
    thread::spawn(move || {
        let result = Photo::open(&path).map(|photo| {
            let auto = photo.analyze();
            let original = Arc::new(photo.preview(PREVIEW_MAX));
            let reference = Arc::new(photo.reference());
            Loaded {
                reference,
                full: Arc::new(photo.into_oriented()),
                auto,
                original,
                path,
                settings: Settings::default(),
                view: RefCell::new(None),
                full_view: RefCell::new(None),
            }
        });
        if current.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = weak.upgrade_in_event_loop(move |app| {
            if current.load(Ordering::SeqCst) == generation {
                done(&app, result);
            }
        });
    });
}
