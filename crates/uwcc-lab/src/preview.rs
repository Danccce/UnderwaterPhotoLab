//! Lädt das gewählte Bild im Hintergrund und rechnet die Vorschau.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};

use slint::{Rgba8Pixel, SharedPixelBuffer, Weak};
use uwcc_core::{Correction, Settings};
use uwcc_photo::{Photo, RgbaImage};

use crate::LabWindow;

/// Längste Kante der Vorschau; der Export rechnet in voller Auflösung.
pub const PREVIEW_MAX: u32 = 1280;

pub struct Loaded {
    pub path: PathBuf,
    /// Ganzes Bild, gedreht und unbearbeitet, für die 100-%-Ansicht.
    pub full: Arc<RgbaImage>,
    /// Für Dehaze; dieselbe wie beim Export ([`Photo::reference`]).
    pub reference: Arc<uwcc_filters::Reference>,
    /// Unbearbeitete, gedrehte Vorschau.
    pub original: RgbaImage,
    /// Auto-Analyse des ganzen Bildes in voller Auflösung.
    pub auto: Correction,
    pub settings: Settings,
}

impl Loaded {
    pub fn original_buffer(&self) -> SharedPixelBuffer<Rgba8Pixel> {
        buffer(&self.original)
    }

    pub fn corrected_buffer(&self) -> SharedPixelBuffer<Rgba8Pixel> {
        let mut buf = buffer(&self.original);
        let (w, h) = self.original.dimensions();
        let ctx = uwcc_filters::Context::new(self.auto, self.reference.clone(), w, h);
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
            let original = photo.preview(PREVIEW_MAX);
            let reference = Arc::new(photo.reference());
            Loaded {
                reference,
                full: Arc::new(photo.into_oriented()),
                auto,
                original,
                path,
                settings: Settings::default(),
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
