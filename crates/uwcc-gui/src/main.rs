//! Fenster zum Ermitteln der Werte für die Unterwasser-Farbkorrektur: Bild hineinziehen, Werte
//! ablesen, kopieren oder direkt im laufenden PhotoCraft anwenden.
//!
//! Algorithmus: „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0).
#![windows_subsystem = "windows"]

mod photocraft;

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

use image::RgbaImage;
use serde_json::json;
use slint::{
    winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor},
    ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel,
};
use uwcc_core::{analyze, Correction, Transform};

slint::include_modules!();

const DEFAULT_PORT: u16 = 7878;
const PREVIEW_MAX: u32 = 1000;
const EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "tif", "tiff"];

#[derive(Default)]
struct State {
    correction: Option<Correction>,
    /// Zählt Ladevorgänge, damit ein später fertiges älteres Bild kein neueres überschreibt.
    generation: AtomicU64,
}

fn main() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new().backend_name("winit".into()).select()?;

    let app = AppWindow::new()?;
    let state = Arc::new(Mutex::new(State::default()));

    app.set_port(DEFAULT_PORT.to_string().into());
    app.set_token_file(default_token_file().display().to_string().into());

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
                load(&app, &st, path.clone());
            }
            _ => {}
        }
        EventResult::Propagate
    });

    let weak = app.as_weak();
    let st = state.clone();
    app.on_choose_file(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Bilder", &EXTENSIONS)
            .pick_file()
        {
            load(&app, &st, path);
        }
    });

    let weak = app.as_weak();
    let st = state.clone();
    app.on_copy_values(move || {
        let Some(app) = weak.upgrade() else { return };
        let Some(c) = st.lock().unwrap().correction else { return };
        let text = semicolon_values(&c);
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text.clone())) {
            Ok(()) => set_status(&app, &format!("Kopiert: {text}"), false),
            Err(e) => set_status(&app, &format!("Zwischenablage nicht verfügbar: {e}"), true),
        }
    });

    let weak = app.as_weak();
    let st = state.clone();
    app.on_apply_in_photocraft(move || {
        let Some(app) = weak.upgrade() else { return };
        let Some(c) = st.lock().unwrap().correction else { return };
        let Ok(port) = app.get_port().trim().parse::<u16>() else {
            set_status(&app, "Ungültiger Port.", true);
            return;
        };
        let token_file = PathBuf::from(app.get_token_file().trim());
        app.set_applying(true);
        set_status(&app, "Sende an PhotoCraft …", false);

        let weak = app.as_weak();
        thread::spawn(move || {
            let result = photocraft::run_expert_plugin(port, &token_file, plugin_params(&c));
            let _ = weak.upgrade_in_event_loop(move |app| {
                app.set_applying(false);
                match result {
                    Ok(()) => set_status(&app, "In PhotoCraft auf das aktive Dokument angewendet.", false),
                    Err(e) => set_status(&app, &e, true),
                }
            });
        });
    });

    // Ein Bild als Argument: „Öffnen mit“ oder ein Bild auf die .exe gezogen.
    if let Some(path) = std::env::args_os().nth(1) {
        load(&app, &state, path.into());
    }

    app.run()
}

fn default_token_file() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("uwcc").join("photocraft-control.token")
}

fn set_status(app: &AppWindow, text: &str, error: bool) {
    app.set_status(text.into());
    app.set_status_error(error);
}

fn load(app: &AppWindow, state: &Arc<Mutex<State>>, path: PathBuf) {
    let supported = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    if !supported {
        set_status(app, "Bitte ein JPG-Bild verwenden (PNG und TIFF gehen auch).", true);
        return;
    }

    let generation = state.lock().unwrap().generation.fetch_add(1, Ordering::SeqCst) + 1;
    app.set_busy(true);
    set_status(app, "", false);

    let weak = app.as_weak();
    let state = state.clone();
    thread::spawn(move || {
        let result = analyze_file(&path);
        let _ = weak.upgrade_in_event_loop(move |app| {
            let mut st = state.lock().unwrap();
            if st.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            app.set_busy(false);
            match result {
                Ok(a) => {
                    st.correction = Some(a.correction);
                    app.set_preview_original(Image::from_rgba8(a.original));
                    app.set_preview_corrected(Image::from_rgba8(a.corrected));
                    app.set_file_name(a.label.into());
                    app.set_values(ModelRc::new(VecModel::from(value_rows(&a.correction))));
                    app.set_has_image(true);
                    app.set_has_values(true);
                    app.set_show_original(false);
                    set_status(&app, "", false);
                }
                Err(e) => set_status(&app, &e, true),
            }
        });
    });
}

struct Analysis {
    label: String,
    correction: Correction,
    original: SharedPixelBuffer<Rgba8Pixel>,
    corrected: SharedPixelBuffer<Rgba8Pixel>,
}

fn analyze_file(path: &Path) -> Result<Analysis, String> {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let img = image::open(path).map_err(|e| format!("{name}: {e}"))?;
    let (width, height) = (img.width(), img.height());

    let correction = analyze(img.to_rgba8().as_raw());
    let preview = img.thumbnail(PREVIEW_MAX, PREVIEW_MAX).to_rgba8();
    let corrected = apply(&preview, &correction);

    Ok(Analysis {
        label: format!("{name} ({width} × {height} px)"),
        correction,
        original: pixel_buffer(&preview),
        corrected: pixel_buffer(&corrected),
    })
}

fn apply(img: &RgbaImage, c: &Correction) -> RgbaImage {
    let t = Transform::new(c);
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let f = |v: u8| v as f32 / 255.0;
        let (r, g, b) = t.apply(f(p[0]), f(p[1]), f(p[2]));
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        p.0 = [q(r), q(g), q(b), p[3]];
    }
    out
}

fn pixel_buffer(img: &RgbaImage) -> SharedPixelBuffer<Rgba8Pixel> {
    SharedPixelBuffer::clone_from_slice(img.as_raw(), img.width(), img.height())
}

/// Die sieben Werte in der Reihenfolge des Plug-in-Dialogs.
fn ordered_values(c: &Correction) -> [(&'static str, f64); 7] {
    [
        ("hueShift", c.hue_shift),
        ("redLow", c.red.low),
        ("redHigh", c.red.high),
        ("greenLow", c.green.low),
        ("greenHigh", c.green.high),
        ("blueLow", c.blue.low),
        ("blueHigh", c.blue.high),
    ]
}

fn value_rows(c: &Correction) -> Vec<ValueRow> {
    ordered_values(c)
        .into_iter()
        .map(|(name, v)| ValueRow { name: name.into(), value: v.to_string().into() })
        .collect()
}

fn semicolon_values(c: &Correction) -> String {
    let values: Vec<String> = ordered_values(c).iter().map(|(_, v)| v.to_string()).collect();
    values.join(";")
}

fn plugin_params(c: &Correction) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    for (name, v) in ordered_values(c) {
        params.insert(name.into(), json!(v));
    }
    params.insert("strength".into(), json!(100));
    params.into()
}
