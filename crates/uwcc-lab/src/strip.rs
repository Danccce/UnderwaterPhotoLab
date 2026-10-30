//! Lädt die Vorschaubilder der Bildleiste im Hintergrund: vom Original und, falls exportiert,
//! von der korrigierten Datei.

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
/// Begrenzt, weil Bilder ohne EXIF-Vorschaubild ganz dekodiert werden müssen (16 MP ≈ 64 MB).
const WORKERS: usize = 4;

pub fn thumbnail_buffer(path: &Path) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let t = uwcc_photo::thumbnail(path, THUMB_MAX).ok()?;
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

/// Füllt `thumbs` Zeile für Zeile. `files` sind Original und ggf. exportierte Datei. Bricht ab,
/// sobald `current` nicht mehr `generation` ist (ein anderer Ordner wurde geöffnet).
pub fn load(weak: Weak<LabWindow>, files: Vec<(PathBuf, Option<PathBuf>)>, current: Arc<AtomicU64>, generation: u64) {
    let total = files.len();
    let files = Arc::new(files);
    let next = Arc::new(AtomicUsize::new(0));

    for _ in 0..WORKERS.min(total) {
        let (weak, files, next, current) = (weak.clone(), files.clone(), next.clone(), current.clone());
        thread::spawn(move || loop {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let i = next.fetch_add(1, Ordering::SeqCst);
            if i >= total {
                return;
            }
            let (original, exported) = &files[i];
            let buffer = thumbnail_buffer(original);
            let corrected = exported.as_deref().and_then(thumbnail_buffer);
            let current = current.clone();

            let _ = weak.upgrade_in_event_loop(move |app| {
                if current.load(Ordering::SeqCst) != generation {
                    return;
                }
                let model = app.get_thumbs();
                if let Some(mut row) = model.row_data(i) {
                    row.loaded = true;
                    row.failed = buffer.is_none();
                    if let Some(b) = buffer {
                        row.image = Image::from_rgba8(b);
                    }
                    row.has_corrected = corrected.is_some();
                    if let Some(c) = corrected {
                        row.corrected = Image::from_rgba8(c);
                    }
                    model.set_row_data(i, row);
                }
            });
        });
    }
}
