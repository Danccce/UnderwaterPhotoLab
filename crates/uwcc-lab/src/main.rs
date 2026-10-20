//! UnderwaterPhotoLab: Farbkorrektur für Unterwasserfotos.
//!
//! Algorithmus der Auto-Korrektur: „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0).
#![windows_subsystem = "windows"]

mod strip;

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use slint::{
    winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor},
    ComponentHandle, Image, ModelRc, VecModel,
};

slint::include_modules!();

#[derive(Default)]
struct State {
    dir: Option<PathBuf>,
    files: Vec<PathBuf>,
    /// Wird bei jedem geöffneten Ordner erhöht; laufende Hintergrundarbeit für einen älteren
    /// Ordner verwirft ihre Ergebnisse.
    generation: Arc<AtomicU64>,
}

fn main() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new().backend_name("winit".into()).select()?;

    let app = LabWindow::new()?;
    app.window().set_maximized(true);
    let state = Rc::new(RefCell::new(State::default()));
    app.set_status("Kein Ordner geöffnet.".into());

    let weak = app.as_weak();
    let st = state.clone();
    app.window().on_winit_window_event(move |_, event| {
        let Some(app) = weak.upgrade() else {
            return EventResult::Propagate;
        };
        match event {
            WindowEvent::HoveredFile(_) => app.set_drag_over(true),
            WindowEvent::HoveredFileCancelled => app.set_drag_over(false),
            WindowEvent::DroppedFile(path) => {
                app.set_drag_over(false);
                open(&app, &st, path);
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak = app.as_weak();
    let st = state.clone();
    app.on_open_folder(move || {
        let Some(app) = weak.upgrade() else { return };
        let start_dir = st.borrow().dir.clone();
        let mut dialog = rfd::FileDialog::new().set_title("Ordner mit Unterwasserfotos öffnen");
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(dir) = dialog.pick_folder() {
            open(&app, &st, &dir);
        }
    });

    let weak = app.as_weak();
    let st = state.clone();
    app.on_thumb_clicked(move |i| {
        let Some(app) = weak.upgrade() else { return };
        select(&app, &st.borrow(), i);
    });

    // Ordner oder Bild als Argument: „Öffnen mit“ oder auf die .exe gezogen.
    if let Some(path) = std::env::args_os().nth(1) {
        open(&app, &state, Path::new(&path));
    }

    app.run()
}

/// Öffnet einen Ordner; bei einem Bild dessen Ordner mit diesem Bild ausgewählt.
fn open(app: &LabWindow, state: &Rc<RefCell<State>>, path: &Path) {
    let (dir, wanted) = if path.is_dir() {
        (path.to_owned(), None)
    } else {
        match path.parent() {
            Some(dir) => (dir.to_owned(), Some(path.to_owned())),
            None => return,
        }
    };
    let files = match uwcc_photo::scan_folder(&dir) {
        Ok(files) => files,
        Err(e) => {
            app.set_status(e.into());
            return;
        }
    };

    let mut st = state.borrow_mut();
    let generation = st.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let rows: Vec<Thumb> = files
        .iter()
        .map(|f| Thumb { image: Image::default(), name: uwcc_photo::file_name(f).into(), loaded: false, failed: false })
        .collect();
    app.set_thumbs(ModelRc::new(VecModel::from(rows)));
    app.set_folder(dir.display().to_string().into());
    app.set_has_preview(false);
    app.set_status(if files.is_empty() {
        "Keine Bilder (JPG, PNG, TIFF) in diesem Ordner.".into()
    } else {
        format!("{} Bilder, lade Vorschaubilder …", files.len()).into()
    });

    let selected = wanted.and_then(|w| files.iter().position(|f| *f == w)).unwrap_or(0);
    strip::load(app.as_weak(), files.clone(), st.generation.clone(), generation);
    st.dir = Some(dir);
    st.files = files;
    select(app, &st, selected as i32);
}

fn select(app: &LabWindow, state: &State, index: i32) {
    if index < 0 || index as usize >= state.files.len() {
        app.set_current(-1);
        return;
    }
    app.set_current(index);
}
