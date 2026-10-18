//! Unterwasser: Rot wiederherstellen, als PhotoCraft-Plug-in (Plug-in-ABI v1).
//!
//! Der Hue-Shift-Schritt aus „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0). Die
//! Kanalspreizung des Originals braucht das ganze Bild und übernimmt danach PhotoCraft mit
//! Image › Auto Tone.
//!
//! `no_std`, kein Allocator, keine Imports: PhotoCraft lehnt Module mit Host-Imports ab.
#![cfg(target_arch = "wasm32")]
#![no_std]

use uwcc_core::{params::number, Correction, Transform};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

/// Passt für typisches Blauwasser; auf den Beispielbildern des Originals liegt der automatisch
/// gesuchte Winkel bei 77–101°.
const DEFAULT_HUE_SHIFT: f64 = 80.0;

const MANIFEST: &str = r#"{
  "id": "local.underwater-red-restore",
  "name": "Unterwasser: Rot wiederherstellen…",
  "version": "0.1.0",
  "kind": "filter",
  "author": "Nikolaj Bech Andersen (Algorithmus), Marc Lob (PhotoCraft-Plug-in)",
  "description": "Mischt Grün und Blau in den Rotkanal zurück, den das Wasser geschluckt hat. Danach Image › Auto Tone anwenden. Algorithmus: underwater-image-color-correction von Nikolaj Bech Andersen, github.com/nikolajbech/underwater-image-color-correction (GPL-3.0).",
  "params": {
    "hueShift": {"type": "int", "min": 0, "max": 120, "default": 80},
    "strength": {"type": "number", "min": 0, "max": 100, "default": 100}
  },
  "overlap": 0,
  "area": "content"
}"#;

/// Bit 8 von `format`: der letzte Kanal ist Alpha.
const HAS_ALPHA: u32 = 1 << 8;

/// Farbmodi aus den Bits 16–23 von `format` (Nummerierung wie im PSD-Format).
const MODE_INDEXED: u32 = 2;
const MODE_RGB: u32 = 3;

const ERR_NO_CHANNELS: i32 = 1;
const ERR_UNSUPPORTED_MODE: i32 = 2;
const ERR_BAD_PARAMS: i32 = 3;

#[no_mangle]
pub extern "C" fn pc_abi_version() -> u32 {
    1
}

/// `len << 32 | ptr` des UTF-8-Manifest-JSON.
#[no_mangle]
pub extern "C" fn pc_manifest() -> u64 {
    ((MANIFEST.len() as u64) << 32) | MANIFEST.as_ptr() as u64
}

/// Frische Seiten per `memory.grow`; jeder Lauf bekommt eine neue Instanz, daher wird nie
/// freigegeben. Gibt 0 zurück, wenn das Speicherlimit des Hosts erreicht ist.
#[no_mangle]
pub extern "C" fn pc_alloc(size: u32) -> u32 {
    let pages = (size as usize).div_ceil(65536);
    let old = core::arch::wasm32::memory_grow(0, pages);
    if old == usize::MAX {
        0
    } else {
        (old * 65536) as u32
    }
}

/// # Safety
/// Der Host garantiert, dass `buf..buf + len` (`len / 4` Samples) und
/// `params..params + params_len` Blöcke aus `pc_alloc` sind.
#[no_mangle]
pub unsafe extern "C" fn pc_filter(
    buf: *mut f32,
    len: u32,
    _width: u32,
    _height: u32,
    channels: u32,
    format: u32,
    params: *const u8,
    params_len: u32,
) -> i32 {
    let n = channels as usize;
    if n == 0 {
        return ERR_NO_CHANNELS;
    }
    let json: &[u8] = if params_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(params, params_len as usize) }
    };
    let (Some(hue_shift), Some(strength)) = (
        number(json, b"hueShift", DEFAULT_HUE_SHIFT),
        number(json, b"strength", 100.0),
    ) else {
        return ERR_BAD_PARAMS;
    };
    let px = unsafe { core::slice::from_raw_parts_mut(buf, len as usize / 4) };
    let colour = if format & HAS_ALPHA != 0 { n - 1 } else { n };
    // 32-Bit-Dokumente dürfen Werte über 1 enthalten.
    let max = if format & 0xff == 32 { f32::INFINITY } else { 1.0 };

    match (colour, (format >> 16) & 0xff) {
        (3, MODE_RGB | MODE_INDEXED) => {
            let t = Transform::new(&Correction { hue_shift, ..Correction::NONE });
            let s = (strength / 100.0) as f32;
            for c in px.chunks_exact_mut(n) {
                let r = c[0];
                let (nr, _, _) = t.apply(r, c[1], c[2]);
                c[0] = r + (nr.clamp(0.0, max) - r) * s;
            }
        }
        _ => return ERR_UNSUPPORTED_MODE,
    }
    0
}
