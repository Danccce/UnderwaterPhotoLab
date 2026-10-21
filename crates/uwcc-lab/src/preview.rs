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
use uwcc_core::{process_rgba8, Correction, Settings};
use uwcc_photo::{Photo, RgbaImage};

use crate::LabWindow;

/// Längste Kante der Vorschau; der Export rechnet in voller Auflösung.
pub const PREVIEW_MAX: u32 = 1280;

pub struct Loaded {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
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
        let mut px = self.original.clone();
        process_rgba8(&mut px, &self.settings, &self.auto);
        buffer(&px)
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
            let (width, height) = photo.rgba.dimensions();
            Loaded {
                width,
                height,
                auto: photo.analyze(),
                original: photo.preview(PREVIEW_MAX),
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
