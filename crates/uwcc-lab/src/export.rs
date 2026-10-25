//! Export in voller Auflösung, parallel im Hintergrund, mit Fortschritt und Abbrechen.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
};

use slint::ComponentHandle;
use uwcc_core::Settings;
use uwcc_photo::sidecar::Exported;

use crate::{store, strip, with_state, LabWindow};

/// Ein 16-MP-Bild braucht beim Export rund 200 MB; daher höchstens 4 gleichzeitig.
fn workers() -> usize {
    thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4))
}

pub fn start(app: &LabWindow, paths: Vec<PathBuf>) {
    if paths.is_empty() || with_state(|st| st.export_cancel.is_some()) {
        return;
    }
    let (jobs, target, quality, cancel) = with_state(|st| {
        let jobs: Vec<(PathBuf, Settings)> = paths.iter().map(|p| (p.clone(), store::settings_of(st, p))).collect();
        let cancel = Arc::new(AtomicBool::new(false));
        st.export_cancel = Some(cancel.clone());
        st.export_errors.clear();
        (jobs, crate::EXPORT_TARGET, st.config.jpeg_quality, cancel)
    });
    let total = jobs.len();
    app.set_exporting(true);
    app.set_export_progress(format!("Exportiere 0 / {total} …").into());

    let jobs = Arc::new(jobs);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let n = workers().min(total);
    let alive = Arc::new(AtomicUsize::new(n));

    for _ in 0..n {
        let (weak, jobs, next, done, alive, cancel) =
            (app.as_weak(), jobs.clone(), next.clone(), done.clone(), alive.clone(), cancel.clone());
        thread::spawn(move || {
            while !cancel.load(Ordering::SeqCst) {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((path, settings)) = jobs.get(i).cloned() else { break };
                let result = uwcc_photo::export_file(&path, &settings, target, quality);
                let thumb = result.as_ref().ok().and_then(|out| strip::thumbnail_buffer(out));
                let finished = done.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = weak.upgrade_in_event_loop(move |app| {
                    let index = with_state(|st| {
                        let index = st.files.iter().position(|f| *f == path);
                        match result {
                            Ok(_) => {
                                st.exported.insert(path, Exported { settings, target, quality });
                            }
                            Err(e) => st.export_errors.push(e),
                        }
                        index
                    });
                    if let (Some(i), Some(t)) = (index, thumb) {
                        strip::set_corrected(&app, i, Some(t));
                    }
                    app.set_export_progress(format!("Exportiere {finished} / {total} …").into());
                    store::refresh_marks(&app);
                });
            }
            if alive.fetch_sub(1, Ordering::SeqCst) == 1 {
                let finished = done.load(Ordering::SeqCst);
                let _ = weak.upgrade_in_event_loop(move |app| finish(&app, finished, total));
            }
        });
    }
}

pub fn cancel(app: &LabWindow) {
    with_state(|st| {
        if let Some(c) = &st.export_cancel {
            c.store(true, Ordering::SeqCst);
        }
    });
    app.set_export_progress("Breche ab, laufende Bilder werden fertig …".into());
}

fn finish(app: &LabWindow, finished: usize, total: usize) {
    let (errors, target_dir) = with_state(|st| {
        st.export_cancel = None;
        let target_dir = st.dir.clone().unwrap_or_default().join(uwcc_photo::SUBFOLDER);
        (std::mem::take(&mut st.export_errors), target_dir)
    });
    app.set_exporting(false);
    app.set_export_progress("".into());
    let ok = finished - errors.len();
    let mut msg = if finished < total {
        format!("Export abgebrochen: {ok} von {total} Bildern gespeichert in {}.", target_dir.display())
    } else {
        format!("{ok} von {total} Bildern exportiert nach {}.", target_dir.display())
    };
    if let Some(first) = errors.first() {
        msg.push_str(&format!("\n{} Fehler, z. B.: {first}", errors.len()));
    }
    app.set_status(msg.into());
    store::refresh_marks(app);
    store::save_now(app);
    crate::programs::after_export(app, finished == total && errors.is_empty());
}
