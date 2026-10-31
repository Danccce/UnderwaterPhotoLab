//! Lädt die Vorschaubilder der Bildleiste im Hintergrund: vom Original und, falls exportiert,
//! von der korrigierten Datei. Erst schnell die EXIF-Vorschaubilder, danach schärfere für große
//! Rasterzellen.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    thread,
};

use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, Weak};

use crate::LabWindow;

/// Längste Kante der Vorschaubilder; EXIF-Vorschaubilder sind meist 160 px und bleiben so.
pub const THUMB_MAX: u32 = 240;
/// Längste Kante der nachgeladenen scharfen Vorschaubilder (größte Rasterzelle).
const SHARP_MAX: u32 = 320;
/// Begrenzt, weil Bilder ohne EXIF-Vorschaubild ganz dekodiert werden müssen (16 MP ≈ 64 MB).
const WORKERS: usize = 4;

pub fn thumbnail_buffer(path: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    buffer(uwcc_photo::thumbnail(path, THUMB_MAX).ok()?)
}

pub fn sharp_buffer(path: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    buffer(uwcc_photo::sharp_thumbnail(path, SHARP_MAX).ok()?)
}

fn buffer(t: uwcc_photo::RgbaImage) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    Some(SharedPixelBuffer::clone_from_slice(t.as_raw(), t.width(), t.height()))
}

/// Setzt das korrigierte Vorschaubild einer Zeile (nach dem Laden oder einem Export).
pub fn set_corrected(app: &LabWindow, index: usize, buffer: Option<SharedPixelBuffer<Rgba8Pixel>>) {
    let model = app.get_thumbs();
    if let Some(mut row) = model.row_data(index) {
        row.has_corrected = buffer.is_some();
        row.corrected = buffer.map(Image::from_rgba8).unwrap_or_default();
        model.set_row_data(index, row);
    }
}

/// Füllt `thumbs` Zeile für Zeile, erst schnell, dann scharf. `files` sind Original und ggf.
/// exportierte Datei. Bricht ab, sobald `current` nicht mehr `generation` ist (ein anderer Ordner
/// wurde geöffnet).
pub fn load(weak: Weak<LabWindow>, files: Vec<(PathBuf, Option<PathBuf>)>, current: Arc<AtomicU64>, generation: u64) {
    let total = files.len();
    let files = Arc::new(files);
    // Ein Zähler für beide Durchgänge: 0 … total−1 schnell, total … 2·total−1 scharf.
    let next = Arc::new(AtomicUsize::new(0));

    for _ in 0..WORKERS.min(total) {
        let (weak, files, next, current) = (weak.clone(), files.clone(), next.clone(), current.clone());
        thread::spawn(move || loop {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let n = next.fetch_add(1, Ordering::SeqCst);
            if n >= 2 * total {
                return;
            }
            let (i, sharp) = (n % total, n >= total);
            let (original, exported) = &files[i];
            let load = if sharp { sharp_buffer } else { thumbnail_buffer };
            let buffer = load(original);
            let corrected = exported.as_deref().and_then(load);
            let current = current.clone();

            let _ = weak.upgrade_in_event_loop(move |app| {
                if current.load(Ordering::SeqCst) != generation {
                    return;
                }
                let model = app.get_thumbs();
                let Some(mut row) = model.row_data(i) else {
                    return;
                };
                if sharp {
                    // Nur ersetzen, was geladen werden konnte; ein fehlgeschlagener Versuch lässt das
                    // schnelle Vorschaubild stehen.
                    if let Some(b) = buffer {
                        row.image = Image::from_rgba8(b);
                        row.loaded = true;
                        row.failed = false;
                    }
                    if let Some(c) = corrected {
                        row.corrected = Image::from_rgba8(c);
                        row.has_corrected = true;
                    }
                } else {
                    // Das scharfe Vorschaubild kann (selten) schneller da sein.
                    if row.loaded {
                        return;
                    }
                    row.loaded = true;
                    row.failed = buffer.is_none();
                    if let Some(b) = buffer {
                        row.image = Image::from_rgba8(b);
                    }
                    row.has_corrected = corrected.is_some();
                    if let Some(c) = corrected {
                        row.corrected = Image::from_rgba8(c);
                    }
                }
                model.set_row_data(i, row);
            });
        });
    }
}
