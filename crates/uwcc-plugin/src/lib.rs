//! Unterwasser-Farbkorrektur als PhotoCraft-Plug-in (Plug-in-ABI v1).
//!
//! Algorithmus: „underwater-image-color-correction“ von Nikolaj Bech Andersen,
//! <https://github.com/nikolajbech/underwater-image-color-correction> (GPL-3.0).
//!
//! `no_std`, kein Allocator, keine Imports: PhotoCraft lehnt Module mit Host-Imports ab.
//! Das Plug-in sieht immer nur einen Streifen des Bildes; die Werte für das ganze Bild
//! ermittelt `uwcc-analyze`.
#![cfg(target_arch = "wasm32")]
#![no_std]

use uwcc_core::{Correction, Levels, Transform};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

const MANIFEST: &str = r#"{
  "id": "local.underwater-color-correction",
  "name": "Unterwasser-Farbkorrektur…",
  "version": "0.1.0",
  "kind": "filter",
  "author": "Nikolaj Bech Andersen (Algorithmus), Marc Lob (PhotoCraft-Plug-in)",
  "description": "Stellt das von Wasser geschluckte Rot wieder her und spreizt die Farbkanäle. Algorithmus: underwater-image-color-correction von Nikolaj Bech Andersen, github.com/nikolajbech/underwater-image-color-correction (GPL-3.0). Die Werte für ein Foto ermittelt uwcc-analyze.",
  "params": {
    "hueShift":  {"type": "int", "min": 0, "max": 180, "default": 0},
    "redLow":    {"type": "int", "min": 0, "max": 255, "default": 0},
    "redHigh":   {"type": "int", "min": 1, "max": 256, "default": 256},
    "greenLow":  {"type": "int", "min": 0, "max": 255, "default": 0},
    "greenHigh": {"type": "int", "min": 1, "max": 256, "default": 256},
    "blueLow":   {"type": "int", "min": 0, "max": 255, "default": 0},
    "blueHigh":  {"type": "int", "min": 1, "max": 256, "default": 256},
    "strength":  {"type": "number", "min": 0, "max": 100, "default": 100}
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

struct FilterParams {
    correction: Correction,
    /// 0.0 = Original, 1.0 = volle Korrektur.
    strength: f32,
}

impl FilterParams {
    fn parse(json: &[u8]) -> Option<Self> {
        let num = |key: &[u8], default: f64| match top_level_value(json, key) {
            Some(v) => parse_f64(v),
            None => Some(default),
        };
        let levels = |low: &[u8], high: &[u8]| {
            Some(Levels {
                low: num(low, Levels::FULL.low)?,
                high: num(high, Levels::FULL.high)?,
            })
        };
        Some(Self {
            correction: Correction {
                hue_shift: num(b"hueShift", 0.0)?,
                red: levels(b"redLow", b"redHigh")?,
                green: levels(b"greenLow", b"greenHigh")?,
                blue: levels(b"blueLow", b"blueHigh")?,
            },
            strength: (num(b"strength", 100.0)? / 100.0) as f32,
        })
    }
}

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
    let Some(p) = FilterParams::parse(json) else {
        return ERR_BAD_PARAMS;
    };
    let px = unsafe { core::slice::from_raw_parts_mut(buf, len as usize / 4) };
    let colour = if format & HAS_ALPHA != 0 { n - 1 } else { n };
    // 32-Bit-Dokumente dürfen Werte über 1 enthalten.
    let max = if format & 0xff == 32 { f32::INFINITY } else { 1.0 };

    match (colour, (format >> 16) & 0xff) {
        (3, MODE_RGB | MODE_INDEXED) => {
            let t = Transform::new(&p.correction);
            let s = p.strength;
            for c in px.chunks_exact_mut(n) {
                let r = c[0];
                let g = c[1];
                let b = c[2];
                let (nr, ng, nb) = t.apply(r, g, b);
                c[0] = r + (nr.clamp(0.0, max) - r) * s;
                c[1] = g + (ng.clamp(0.0, max) - g) * s;
                c[2] = b + (nb.clamp(0.0, max) - b) * s;
            }
        }
        _ => return ERR_UNSUPPORTED_MODE,
    }
    0
}

/// Wert eines Schlüssels auf oberster Ebene des Parameter-JSON (Strings ohne Anführungszeichen).
/// Verschachtelte Objekte wie `_image` werden übersprungen, sie enthalten ebenfalls `"mode"`.
fn top_level_value<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut depth = 0u32;
    let mut i = 0;
    while i < json.len() {
        match json[i] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'"' => {
                let start = i + 1;
                let end = string_end(json, start)?;
                i = end + 1;
                if depth == 1 {
                    let colon = skip_ws(json, i);
                    if json.get(colon) == Some(&b':') && &json[start..end] == key {
                        return scalar_at(json, skip_ws(json, colon + 1));
                    }
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn string_end(json: &[u8], mut i: usize) -> Option<usize> {
    while i < json.len() {
        match json[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn skip_ws(json: &[u8], mut i: usize) -> usize {
    while matches!(json.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

fn scalar_at(json: &[u8], i: usize) -> Option<&[u8]> {
    if json.get(i) == Some(&b'"') {
        let end = string_end(json, i + 1)?;
        return Some(&json[i + 1..end]);
    }
    let rest = json.get(i..)?;
    let len = rest
        .iter()
        .position(|b| matches!(b, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'))
        .unwrap_or(rest.len());
    Some(&rest[..len])
}

fn parse_f64(v: &[u8]) -> Option<f64> {
    core::str::from_utf8(v).ok()?.parse().ok()
}
