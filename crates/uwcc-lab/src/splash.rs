//! Startbildschirm: ein kleines rahmenloses Fenster in der Bildschirmmitte, vor dem Hauptfenster.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use slint::{
    winit_030::winit::{platform::windows::WindowAttributesExtWindows, window::WindowAttributes},
    ComponentHandle, PhysicalPosition,
};

use crate::Splash;

/// So lange steht der Startbildschirm allein, bevor das Hauptfenster erscheint …
pub const BEFORE_MAIN: Duration = Duration::from_millis(1200);
/// … und so lange bleibt er danach noch davor.
pub const AFTER_MAIN: Duration = Duration::from_millis(3000);

pub const ALGORITHM_URL: &str = "https://github.com/nikolajbech/underwater-image-color-correction";
pub const COFFEE_URL: &str = "https://buymeacoffee.com/marclob";

/// Größe von `Splash` in lab.slint (logische Pixel).
const SIZE: (f32, f32) = (460.0, 310.0);

#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(index: i32) -> i32;
    fn GetDpiForSystem() -> u32;
    fn SetProcessDpiAwarenessContext(context: isize) -> i32;
}

const SM_CXSCREEN: i32 = 0;
const SM_CYSCREEN: i32 = 1;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

/// Ganz am Anfang aufrufen: dieselbe DPI-Einstellung, die winit später auch setzt, damit die
/// Bildschirmmaße für [`center`] echte Pixel sind.
pub fn dpi_aware() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Mitte des Hauptbildschirms; vor `show()`, sonst erschiene das Fenster erst woanders.
pub fn center(splash: &Splash) {
    let (screen_w, screen_h, dpi) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN), GetDpiForSystem()) };
    let scale = dpi as f32 / 96.0;
    let x = (screen_w as f32 - SIZE.0 * scale) / 2.0;
    let y = (screen_h as f32 - SIZE.1 * scale) / 2.0;
    splash.window().set_position(PhysicalPosition::new(x as i32, y as i32));
}

static FIRST_WINDOW: AtomicBool = AtomicBool::new(true);

/// Für den Fenster-Hook: Das zuerst angelegte Fenster ist der Startbildschirm; er bekommt keinen
/// Eintrag in der Taskleiste. (Titel und Größe kennt der Hook noch nicht.)
pub fn attributes(a: WindowAttributes) -> WindowAttributes {
    if FIRST_WINDOW.swap(false, Ordering::SeqCst) {
        a.with_skip_taskbar(true)
    } else {
        a
    }
}
