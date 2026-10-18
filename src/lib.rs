//! Unterwasser-Farbkorrektur als PhotoCraft-Plug-in (Plug-in-ABI v1).
//!
//! `no_std`, kein Allocator, keine Imports: PhotoCraft lehnt Module mit Host-Imports ab.
#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

const MANIFEST: &str = r#"{
  "id": "local.underwater-color-correction",
  "name": "Unterwasser-Farbkorrektur…",
  "version": "0.1.0",
  "kind": "filter",
  "author": "marcl",
  "description": "Korrigiert den Blau-/Grünstich von Unterwasserfotos.",
  "params": {},
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

struct FilterParams {}

impl FilterParams {
    fn parse(_json: &[u8]) -> Option<Self> {
        Some(Self {})
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
    let Some(_p) = FilterParams::parse(json) else {
        return ERR_BAD_PARAMS;
    };
    let px = unsafe { core::slice::from_raw_parts_mut(buf, len as usize / 4) };
    let colour = if format & HAS_ALPHA != 0 { n - 1 } else { n };

    match (colour, (format >> 16) & 0xff) {
        (3, MODE_RGB | MODE_INDEXED) => {
            for _c in px.chunks_exact_mut(n) {
                // Farbkorrektur folgt.
            }
        }
        _ => return ERR_UNSUPPORTED_MODE,
    }
    0
}

/// Wert eines Schlüssels auf oberster Ebene des Parameter-JSON (Strings ohne Anführungszeichen).
/// Verschachtelte Objekte wie `_image` werden übersprungen, sie enthalten ebenfalls `"mode"`.
#[allow(dead_code)]
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

#[allow(dead_code)]
fn parse_f32(v: &[u8]) -> Option<f32> {
    core::str::from_utf8(v).ok()?.parse().ok()
}
