//! UnderwaterPhotoLab: Farbkorrektur für Unterwasserfotos.
//!
//! Algorithmus der Auto-Korrektur: „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0).
#![windows_subsystem = "windows"]

mod config;
mod controls;
mod export;
mod preview;
mod selection;
mod store;
mod strip;

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use uwcc_photo::{sidecar::Exported, ExportTarget};

use slint::{
    winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor},
    ComponentHandle, Image, Model, ModelRc, VecModel,
};

slint::include_modules!();

#[derive(Default)]
struct State {
    dir: Option<PathBuf>,
    files: Vec<PathBuf>,
    /// Wird bei jedem geöffneten Ordner erhöht; Vorschaubilder eines älteren Ordners werden verworfen.
    folder_generation: Arc<AtomicU64>,
    /// Wird bei jedem gewählten Bild erhöht; ein später fertiges älteres Bild wird verworfen.
    image_generation: Arc<AtomicU64>,
    current: Option<preview::Loaded>,
    /// Einstellungen je Bild; gespeichert in `uw-korrigiert\.uwcc.json` des Ordners.
    settings: HashMap<PathBuf, uwcc_core::Settings>,
    /// Womit ein Bild zuletzt exportiert wurde.
    exported: HashMap<PathBuf, Exported>,
    config: config::Config,
    /// Gesetzt, solange ein Export läuft.
    export_cancel: Option<Arc<AtomicBool>>,
    export_errors: Vec<String>,
    /// Ausgewählte Bilder (Indizes in `files`), das aktuelle Bild ist immer dabei.
    selection: BTreeSet<usize>,
    /// Vorherige Einstellungen der Bilder der letzten Übernahme (`None` = vorher keine).
    undo: Option<Vec<(PathBuf, Option<uwcc_core::Settings>)>>,
}

thread_local! {
    // Nur im Event-Loop-Thread benutzt; Hintergrund-Threads melden sich über `upgrade_in_event_loop`.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn main() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new().backend_name("winit".into()).select()?;

    let app = LabWindow::new()?;
    app.window().set_maximized(true);
    app.set_status("Kein Ordner geöffnet.".into());
    let cfg = config::load();
    with_state(|st| st.config = cfg);
    app.set_export_target(match cfg.export_target {
        ExportTarget::Subfolder => 0,
        ExportTarget::SameFolder => 1,
    });
    app.set_jpeg_quality(cfg.jpeg_quality as f32);

    let weak = app.as_weak();
    app.window().on_winit_window_event(move |_, event| {
        let Some(app) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            WindowEvent::HoveredFile(_) => app.set_drag_over(true),
            WindowEvent::HoveredFileCancelled => app.set_drag_over(false),
            WindowEvent::DroppedFile(path) => {
                app.set_drag_over(false);
                open(&app, path);
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak = app.as_weak();
    app.on_open_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        let mut dialog = rfd::FileDialog::new().set_title("Ordner mit Unterwasserfotos öffnen");
        if let Some(dir) = with_state(|st| st.dir.clone()) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(dir) = dialog.pick_folder() {
            open(&app, &dir);
        }
    });

    let weak = app.as_weak();
    app.on_thumb_pressed(move |i, ctrl, shift| {
        if let Some(app) = weak.upgrade() {
            selection::press(&app, i, ctrl, shift);
        }
    });

    let weak = app.as_weak();
    app.on_navigate(move |step| {
        if let Some(app) = weak.upgrade() {
            let last = with_state(|st| st.files.len() as i32 - 1);
            let next = (app.get_current() + step).clamp(0, last.max(0));
            if next != app.get_current() {
                select(&app, next);
                selection::single(&app, next);
            }
        }
    });

    let weak = app.as_weak();
    app.on_select_all(move || {
        if let Some(app) = weak.upgrade() {
            selection::select_all(&app);
        }
    });
    let weak = app.as_weak();
    app.on_clear_selection(move || {
        if let Some(app) = weak.upgrade() {
            selection::clear(&app);
        }
    });
    let weak = app.as_weak();
    app.on_transfer_fixed(move || {
        if let Some(app) = weak.upgrade() {
            selection::transfer(&app, selection::Transfer::Fixed);
        }
    });
    let weak = app.as_weak();
    app.on_transfer_relative(move || {
        if let Some(app) = weak.upgrade() {
            selection::transfer(&app, selection::Transfer::Relative);
        }
    });
    let weak = app.as_weak();
    app.on_undo_transfer(move || {
        if let Some(app) = weak.upgrade() {
            selection::undo(&app);
        }
    });

    let weak = app.as_weak();
    app.on_edited(move |field, value| {
        if let Some(app) = weak.upgrade() {
            controls::edit(&app, &field, value);
        }
    });
    let weak = app.as_weak();
    app.on_auto_toggled(move |on| {
        if let Some(app) = weak.upgrade() {
            controls::set_auto(&app, on);
        }
    });
    let weak = app.as_weak();
    app.on_reset(move || {
        if let Some(app) = weak.upgrade() {
            controls::reset(&app);
        }
    });
    let weak = app.as_weak();
    app.on_reset_field(move |field| {
        if let Some(app) = weak.upgrade() {
            controls::reset_field(&app, &field);
        }
    });
    let weak = app.as_weak();
    app.on_pick(move |x, y, w, h| {
        if let Some(app) = weak.upgrade() {
            controls::pick(&app, x, y, w, h);
        }
    });

    let weak = app.as_weak();
    app.on_export_current(move || {
        let Some(app) = weak.upgrade() else { return };
        let path = with_state(|st| usize::try_from(app.get_current()).ok().and_then(|i| st.files.get(i).cloned()));
        export::start(&app, path.into_iter().collect());
    });
    let weak = app.as_weak();
    app.on_export_selection(move || {
        let Some(app) = weak.upgrade() else { return };
        let paths = with_state(|st| st.selection.iter().filter_map(|&i| st.files.get(i).cloned()).collect());
        export::start(&app, paths);
    });
    let weak = app.as_weak();
    app.on_cancel_export(move || {
        if let Some(app) = weak.upgrade() {
            export::cancel(&app);
        }
    });
    let weak = app.as_weak();
    app.on_export_target_changed(move |index| {
        let Some(app) = weak.upgrade() else { return };
        let target = if index == 1 { ExportTarget::SameFolder } else { ExportTarget::Subfolder };
        update_config(&app, |c| c.export_target = target);
    });
    let weak = app.as_weak();
    app.on_jpeg_quality_changed(move |q| {
        let Some(app) = weak.upgrade() else { return };
        let q = (q.round() as u8).clamp(config::MIN_JPEG_QUALITY, 100);
        app.set_jpeg_quality(q as f32);
        update_config(&app, |c| c.jpeg_quality = q);
    });

    // Ordner oder Bild als Argument: „Öffnen mit“ oder auf die .exe gezogen.
    if let Some(path) = std::env::args_os().nth(1) {
        open(&app, Path::new(&path));
    }

    let result = app.run();
    store::save_now(&app);
    result
}

fn update_config(app: &LabWindow, f: impl FnOnce(&mut config::Config)) {
    let cfg = with_state(|st| {
        f(&mut st.config);
        st.config
    });
    if let Err(e) = config::save(&cfg) {
        app.set_status(format!("Einstellungen nicht gespeichert: {e}").into());
    }
    store::refresh_marks(app);
}

/// Öffnet einen Ordner; bei einem Bild dessen Ordner mit diesem Bild ausgewählt.
fn open(app: &LabWindow, path: &Path) {
    let (dir, wanted) = if path.is_dir() {
        (path.to_owned(), None)
    } else {
        match path.parent() {
            Some(dir) => (dir.to_owned(), Some(path.to_owned())),
            None => return,
        }
    };
    if with_state(|st| st.export_cancel.is_some()) {
        app.set_status("Bitte warten, bis der Export fertig ist.".into());
        return;
    }
    let files = match uwcc_photo::scan_folder(&dir) {
        Ok(files) => files,
        Err(e) => {
            app.set_status(e.into());
            return;
        }
    };
    store::save_now(app);

    let rows: Vec<Thumb> = files
        .iter()
        .map(|f| Thumb {
            image: Image::default(),
            name: uwcc_photo::file_name(f).into(),
            loaded: false,
            failed: false,
            edited: false,
            exported: false,
            selected: false,
        })
        .collect();
    app.set_thumbs(ModelRc::new(VecModel::from(rows)));
    app.set_folder(dir.display().to_string().into());
    app.set_has_preview(false);
    app.set_image_info("".into());
    app.set_current(-1);
    app.set_status(if files.is_empty() {
        "Keine Bilder (JPG, PNG, TIFF) in diesem Ordner.".into()
    } else {
        format!("{} Bilder, lade Vorschaubilder …", files.len()).into()
    });

    let selected = wanted.and_then(|w| files.iter().position(|f| *f == w)).unwrap_or(0);
    let (counter, generation) = with_state(|st| {
        st.settings.clear();
        st.exported.clear();
        store::load(st, &dir, &files);
        st.dir = Some(dir);
        st.files = files.clone();
        st.current = None;
        st.undo = None;
        (st.folder_generation.clone(), st.folder_generation.fetch_add(1, Ordering::SeqCst) + 1)
    });
    app.set_can_undo_transfer(false);
    store::refresh_marks(app);
    strip::load(app.as_weak(), files, counter, generation);
    select(app, selected as i32);
    selection::single(app, selected as i32);
}

/// Wählt ein Bild: zeigt sofort das Vorschaubild aus der Leiste und lädt das Bild im Hintergrund.
fn select(app: &LabWindow, index: i32) {
    let job = with_state(|st| {
        let path = st.files.get(usize::try_from(index).ok()?)?.clone();
        st.current = None;
        let generation = st.image_generation.fetch_add(1, Ordering::SeqCst) + 1;
        Some((path, st.image_generation.clone(), generation))
    });
    let Some((path, counter, generation)) = job else {
        app.set_current(-1);
        return;
    };
    app.set_current(index);

    let thumb = app.get_thumbs().row_data(index as usize).filter(|t| t.loaded && !t.failed);
    app.set_has_preview(thumb.is_some());
    if let Some(t) = thumb {
        app.set_preview_original(t.image.clone());
        app.set_preview_corrected(t.image);
    }
    app.set_image_info(uwcc_photo::file_name(&path).into());
    app.set_loading(true);
    preview::load(app.as_weak(), path, counter, generation, loaded);
}

fn loaded(app: &LabWindow, result: Result<preview::Loaded, String>) {
    app.set_loading(false);
    match result {
        Ok(mut l) => {
            if let Some(s) = with_state(|st| st.settings.get(&l.path).copied()) {
                l.settings = s;
            }
            app.set_preview_original(Image::from_rgba8(l.original_buffer()));
            app.set_preview_corrected(Image::from_rgba8(l.corrected_buffer()));
            app.set_has_preview(true);
            let a = &l.auto;
            app.set_image_info(
                format!(
                    "{}\n{} × {} px\n\nAuto-Analyse:\nRot wiederherstellen {}°\nRot {}–{}, Grün {}–{}, Blau {}–{}",
                    uwcc_photo::file_name(&l.path),
                    l.width,
                    l.height,
                    a.hue_shift,
                    a.red.low,
                    a.red.high,
                    a.green.low,
                    a.green.high,
                    a.blue.low,
                    a.blue.high,
                )
                .into(),
            );
            with_state(|st| st.current = Some(l));
            controls::refresh(app);
        }
        Err(e) => app.set_image_info(e.into()),
    }
}
