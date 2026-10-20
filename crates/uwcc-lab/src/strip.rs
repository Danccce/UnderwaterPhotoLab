//! Lädt die Vorschaubilder der Bildleiste im Hintergrund.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

use slint::{Image, Model, Rgba8Pixel, SharedPixelBuffer, Weak};

use crate::LabWindow;

/// Längste Kante der Vorschaubilder; EXIF-Vorschaubilder sind meist 160 px und bleiben so.
pub const THUMB_MAX: u32 = 240;
/// Begrenzt, weil Bilder ohne EXIF-Vorschaubild ganz dekodiert werden müssen (16 MP ≈ 64 MB).
const WORKERS: usize = 4;

/// Füllt `thumbs` Zeile für Zeile. Bricht ab, sobald `current` nicht mehr `generation` ist
/// (ein anderer Ordner wurde geöffnet).
pub fn load(weak: Weak<LabWindow>, files: Vec<PathBuf>, current: Arc<AtomicU64>, generation: u64) {
    let total = files.len();
    let files = Arc::new(files);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();

    for _ in 0..WORKERS.min(total) {
        let (weak, files, next, done, current) =
            (weak.clone(), files.clone(), next.clone(), done.clone(), current.clone());
        thread::spawn(move || loop {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let i = next.fetch_add(1, Ordering::SeqCst);
            if i >= total {
                return;
            }
            let buffer = uwcc_photo::thumbnail(&files[i], THUMB_MAX)
                .ok()
                .map(|t| SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(t.as_raw(), t.width(), t.height()));
            let finished = done.fetch_add(1, Ordering::SeqCst) + 1;
            let elapsed = start.elapsed();
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
                    model.set_row_data(i, row);
                }
                if finished == total {
                    app.set_status(format!("{total} Bilder, Vorschaubilder in {:.2} s", elapsed.as_secs_f64()).into());
                } else if finished % 25 == 0 {
                    app.set_status(format!("Vorschaubilder {finished} / {total} …").into());
                }
            });
        });
    }
}
