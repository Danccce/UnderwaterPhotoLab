//! UnderwaterPhotoLab: Farbkorrektur für Unterwasserfotos.
//!
//! Algorithmus der Auto-Korrektur: „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0).
#![windows_subsystem = "windows"]

mod config;
mod controls;
mod export;
mod histogram;
mod history;
mod info;
mod preview;
mod presets;
mod programs;
mod selection;
mod store;
mod strip;
mod zoom;

use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use uwcc_photo::sidecar::Exported;

/// Exportiert wird immer in den Unterordner `uw-korrigiert`.
const EXPORT_TARGET: uwcc_photo::ExportTarget = uwcc_photo::ExportTarget::Subfolder;

use slint::{
    winit_030::{
        winit::event::{MouseScrollDelta, WindowEvent},
        EventResult, WinitWindowAccessor,
    },
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
    /// Programme für „In … öffnen“, aufgebaut aus `config.programs` und der Suche.
    programs: Vec<programs::Program>,
    /// Bilder, die nach dem laufenden Export im gewählten Programm geöffnet werden sollen.
    pending_open: Option<Vec<PathBuf>>,
    /// Ausgewählte Bilder (Indizes in `files`), das aktuelle Bild ist immer dabei.
    selection: BTreeSet<usize>,
    /// Vorherige Einstellungen der Bilder der letzten Übernahme (`None` = vorher keine).
    undo: Option<Vec<(PathBuf, Option<uwcc_core::Settings>)>>,
    /// Rückgängig/Wiederholen je Bild.
    history: HashMap<PathBuf, history::History>,
    /// Eigene Presets (die mitgelieferten kommen aus `presets::builtin`).
    presets: Vec<presets::Preset>,
}

thread_local! {
    // Nur im Event-Loop-Thread benutzt; Hintergrund-Threads melden sich über `upgrade_in_event_loop`.
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn main() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|a| {
            use slint::winit_030::winit::{
                platform::windows::{Color, WindowAttributesExtWindows},
                window::Theme,
            };
            // Titelleiste im Grau der Reglerleiste (Fluent dunkel: #2C2C2C).
            let panel = Color::from_rgb(0x2C, 0x2C, 0x2C);
            a.with_theme(Some(Theme::Dark))
                .with_title_background_color(Some(panel))
                .with_title_text_color(Color::from_rgb(0xE6, 0xE6, 0xE6))
                .with_border_color(Some(panel))
        })
        .select()?;

    let app = LabWindow::new()?;
    let cfg = config::load();
    app.set_jpeg_quality(cfg.jpeg_quality as f32);
    app.set_strip_corrected(cfg.strip_corrected);
    app.set_panel_tab(cfg.panel_tab);
    app.set_grid_size(cfg.grid_size as f32);
    with_state(|st| st.presets = presets::load_user());
    presets::show(&app);
    app.set_transfer_color(cfg.transfer_groups.color);
    app.set_transfer_tone(cfg.transfer_groups.tone);
    app.set_transfer_detail(cfg.transfer_groups.detail);
    app.set_transfer_geometry(cfg.transfer_groups.geometry);
    with_state(|st| st.config = cfg);
    programs::refresh(&app);

    let weak = app.as_weak();
    app.on_open_in_program(move || {
        if let Some(app) = weak.upgrade() {
            programs::open(&app);
        }
    });
    let weak = app.as_weak();
    app.on_choose_program(move |i| {
        if let (Some(app), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
            programs::choose(&app, i);
        }
    });
    let weak = app.as_weak();
    app.on_add_program(move || {
        if let Some(app) = weak.upgrade() {
            programs::add(&app);
        }
    });

    let weak = app.as_weak();
    let mut maximized = false;
    app.window().on_winit_window_event(move |_, event| {
        let Some(app) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        // Erst nach dem ersten Zeichnen maximieren: Slint setzt beim Anzeigen noch die
        // Vorgabegröße, die sonst ein bereits maximiertes Fenster wieder verkleinert (der Rahmen
        // bleibt maximiert, die Fläche nicht). Aus und wieder an, damit Windows neu misst.
        if !maximized && matches!(event, WindowEvent::RedrawRequested) {
            maximized = true;
            let weak = app.as_weak();
            slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                if let Some(app) = weak.upgrade() {
                    app.window().with_winit_window(|w| {
                        w.set_maximized(false);
                        w.set_maximized(true);
                    });
                }
            });
        }
        match event {
            // Das Mausrad meldet Rasterstufen, das Touchpad Pixel; Slint unterscheidet das nicht.
            WindowEvent::MouseWheel { delta, .. } => {
                app.set_wheel_from_touchpad(matches!(delta, MouseScrollDelta::PixelDelta(_)));
            }            WindowEvent::HoveredFile(_) => app.set_drag_over(true),
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
            if let Some(next) = store::next_shown(&app, app.get_current(), step) {
                select(&app, next);
                selection::single(&app, next);
            }
        }
    });
    let weak = app.as_weak();
    app.on_strip_filter_changed(move |_| {
        if let Some(app) = weak.upgrade() {
            store::refresh_marks(&app);
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
    app.on_undo(move || {
        if let Some(app) = weak.upgrade() {
            history::undo(&app);
        }
    });
    let weak = app.as_weak();
    app.on_redo(move || {
        if let Some(app) = weak.upgrade() {
            history::redo(&app);
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
    app.on_pick(move |fx, fy| {
        if let Some(app) = weak.upgrade() {
            controls::pick(&app, fx, fy);
        }
    });
    let weak = app.as_weak();
    app.on_zoom_changed(move || {
        if let Some(app) = weak.upgrade() {
            zoom::request(&app);
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
    app.on_strip_mode_changed(move |corrected| {
        let Some(app) = weak.upgrade() else { return };
        app.set_strip_corrected(corrected);
        update_config(&app, |c| c.strip_corrected = corrected);
    });
    let weak = app.as_weak();
    app.on_rotate(move |clockwise| {
        if let Some(app) = weak.upgrade() {
            controls::rotate(&app, clockwise);
        }
    });
    let weak = app.as_weak();
    app.on_flip(move || {
        if let Some(app) = weak.upgrade() {
            controls::flip(&app);
        }
    });
    let weak = app.as_weak();
    app.on_crop_changed(move |x, y, w, h| {
        if let Some(app) = weak.upgrade() {
            controls::crop(&app, x, y, w, h);
        }
    });
    let weak = app.as_weak();
    app.on_heal_click(move |x, y| {
        if let Some(app) = weak.upgrade() {
            controls::heal_click(&app, x, y);
        }
    });
    let weak = app.as_weak();
    app.on_clear_spots(move || {
        if let Some(app) = weak.upgrade() {
            controls::clear_spots(&app);
        }
    });
    let weak = app.as_weak();
    app.on_reset_geometry(move || {
        if let Some(app) = weak.upgrade() {
            controls::reset_geometry(&app);
        }
    });
    // Zuschneiden an/aus: die Vorschau zeigt den ganzen Rahmen bzw. den Zuschnitt.
    let weak = app.as_weak();
    app.on_cropping_changed(move || {
        if let Some(app) = weak.upgrade() {
            histogram::show_corrected(&app);
        }
    });
    let weak = app.as_weak();
    app.on_apply_preset(move |index| {
        if let Some(app) = weak.upgrade() {
            presets::apply(&app, index as usize);
        }
    });
    let weak = app.as_weak();
    app.on_save_preset(move |name| {
        if let Some(app) = weak.upgrade() {
            presets::save_current(&app, &name);
        }
    });
    let weak = app.as_weak();
    app.on_delete_preset(move |index| {
        if let Some(app) = weak.upgrade() {
            presets::delete(&app, index as usize);
        }
    });
    let weak = app.as_weak();
    app.on_transfer_groups_changed(move || {
        if let Some(app) = weak.upgrade() {
            let groups = selection::groups(&app);
            update_config(&app, |c| c.transfer_groups = groups);
        }
    });
    let weak = app.as_weak();
    app.on_panel_tab_changed(move |tab| {
        if let Some(app) = weak.upgrade() {
            update_config(&app, |c| c.panel_tab = tab);
        }
    });
    let weak = app.as_weak();
    app.on_grid_size_changed(move |size| {
        if let Some(app) = weak.upgrade() {
            update_config(&app, |c| c.grid_size = config::clamp_grid_size(size.round() as i64));
        }
    });
    let weak = app.as_weak();
    app.on_clip_warning_changed(move || {
        if let Some(app) = weak.upgrade() {
            histogram::show_corrected(&app);
        }
    });
    let weak = app.as_weak();
    app.on_jpeg_quality_changed(move |q| {
        let Some(app) = weak.upgrade() else { return };
        let q = (q.round() as u8).clamp(config::MIN_JPEG_QUALITY, 100);
        app.set_jpeg_quality(q as f32);
        update_config(&app, |c| c.jpeg_quality = q);
    });

    // Ordner oder Bild als Argument: „Öffnen mit“ oder auf die .exe gezogen; sonst der zuletzt
    // geöffnete Ordner, sofern es ihn noch gibt.
    if let Some(path) = std::env::args_os().nth(1) {
        open(&app, Path::new(&path));
    } else if let Some(dir) = with_state(|st| st.config.last_folder.clone()).filter(|d| d.is_dir()) {
        open(&app, &dir);
    }

    let result = app.run();
    store::save_now(&app);
    result
}

fn update_config(app: &LabWindow, f: impl FnOnce(&mut config::Config)) {
    let cfg = with_state(|st| {
        f(&mut st.config);
        st.config.clone()
    });
    if let Err(e) = config::save(&cfg) {
        app.invoke_notify(format!("Einstellungen nicht gespeichert: {e}").into());
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
        app.invoke_notify("Bitte warten, bis der Export fertig ist.".into());
        return;
    }
    let files = match uwcc_photo::scan_folder(&dir) {
        Ok(files) => files,
        Err(e) => {
            app.invoke_notify(e.into());
            return;
        }
    };
    store::save_now(app);
    if with_state(|st| st.config.last_folder.as_deref() != Some(dir.as_path())) {
        update_config(app, |c| c.last_folder = Some(dir.clone()));
    }

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
            corrected: Image::default(),
            has_corrected: false,
            slot: -1,
        })
        .collect();
    app.set_thumbs(ModelRc::new(VecModel::from(rows)));
    app.set_folder(dir.display().to_string().into());
    app.set_has_preview(false);
    info::clear(app);
    app.set_current(-1);
    if files.is_empty() {
        app.invoke_notify("Keine Bilder (JPG, PNG, TIFF) in diesem Ordner.".into());
    }

    let selected = wanted.and_then(|w| files.iter().position(|f| *f == w)).unwrap_or(0);
    let (counter, generation, jobs) = with_state(|st| {
        st.settings.clear();
        st.exported.clear();
        store::load(st, &dir, &files);
        st.dir = Some(dir);
        st.files = files.clone();
        st.current = None;
        st.undo = None;
        st.history.clear();
        let jobs: Vec<(PathBuf, Option<PathBuf>)> = files
            .iter()
            .map(|f| (f.clone(), st.exported.get(f).map(|x| uwcc_photo::output_path(f, x.target))))
            .collect();
        (st.folder_generation.clone(), st.folder_generation.fetch_add(1, Ordering::SeqCst) + 1, jobs)
    });
    app.set_can_undo_transfer(false);
    store::refresh_marks(app);
    strip::load(app.as_weak(), jobs, counter, generation);
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
    store::refresh_marks(app);
    app.set_can_undo(false);
    app.set_can_redo(false);
    app.set_cropping(false);
    app.set_spot_marks(Default::default());
    zoom::new_image(app, None);

    let thumb = app.get_thumbs().row_data(index as usize).filter(|t| t.loaded && !t.failed);
    app.set_has_preview(thumb.is_some());
    if let Some(t) = thumb {
        app.set_preview_original(t.image.clone());
        app.set_preview_corrected(t.image);
    }
    info::show(app, &path, None);
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
            app.set_has_preview(true);
            info::show(app, &l.path, Some(&l));
            with_state(|st| st.current = Some(l));
            histogram::show_corrected(app);
            controls::refresh(app);
        }
        Err(e) => {
            histogram::clear(app);
            info::show_error(app, &e);
        }
    }
}
