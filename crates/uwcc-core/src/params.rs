//! Liest die von PhotoCraft validierten Plug-in-Parameter (UTF-8-JSON) ohne Allocator.

/// Zahl unter `key` auf oberster Ebene; `Some(default)`, wenn der Schlüssel fehlt, `None`, wenn
/// der Wert keine Zahl ist.
pub fn number(json: &[u8], key: &[u8], default: f64) -> Option<f64> {
    match top_level_value(json, key) {
        Some(v) => core::str::from_utf8(v).ok()?.parse().ok(),
        None => Some(default),
    }
}

/// Wert eines Schlüssels auf oberster Ebene des Parameter-JSON (Strings ohne Anführungszeichen).
/// Verschachtelte Objekte wie `_image` werden übersprungen, sie enthalten ebenfalls `"mode"`.
pub fn top_level_value<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_nested_objects() {
        let json = br#"{"_image": {"mode": "rgb", "x": 3}, "hueShift": 80, "x": 7}"#;
        assert_eq!(number(json, b"hueShift", 0.0), Some(80.0));
        assert_eq!(number(json, b"x", 0.0), Some(7.0));
        assert_eq!(top_level_value(json, b"mode"), None);
        assert_eq!(number(json, b"strength", 100.0), Some(100.0));
    }
}
