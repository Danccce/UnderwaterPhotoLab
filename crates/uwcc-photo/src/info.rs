//! Aufnahmedaten aus dem EXIF, für die Anzeige auf Deutsch formatiert.

use std::path::Path;

use crate::exif::{self, Tiff};

/// Felder fehlen (`None`), wenn die Kamera sie nicht schreibt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhotoInfo {
    /// „13.04.2024, 10:21:54“
    pub taken: Option<String>,
    /// „OLYMPUS E-M10MarkII“
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// „1/125 s“
    pub exposure_time: Option<String>,
    /// „f/4,0“
    pub aperture: Option<String>,
    /// „ISO 200“
    pub iso: Option<String>,
    /// „14 mm (28 mm KB)“
    pub focal_length: Option<String>,
    /// „+0,3 EV“
    pub exposure_bias: Option<String>,
    /// „ausgelöst“ / „nicht ausgelöst“
    pub flash: Option<String>,
    pub software: Option<String>,
}

/// Liest die Aufnahmedaten aus dem Dateianfang (nur JPG); ohne EXIF bleibt alles leer.
pub fn read_info(path: &Path) -> PhotoInfo {
    exif::read_segment(path).map(|tiff| parse(&tiff)).unwrap_or_default()
}

const TYPE_SIZES: [usize; 11] = [0, 1, 1, 2, 4, 8, 1, 1, 2, 4, 8];

enum Value {
    Text(String),
    Numbers(Vec<f64>),
}

/// Wert eines Eintrags: ASCII als Text, BYTE/SHORT/LONG/RATIONAL (auch vorzeichenbehaftet) als Zahlen.
fn value(t: &Tiff, entry: usize) -> Option<Value> {
    let typ = t.u16(entry + 2)? as usize;
    let count = (t.u32(entry + 4)? as usize).min(1024);
    let size = *TYPE_SIZES.get(typ).filter(|s| **s > 0)?;
    let len = count * size;
    let pos = if len <= 4 { entry + 8 } else { t.u32(entry + 8)? as usize };
    let raw = t.bytes(pos, len)?;
    if typ == 2 {
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let text = String::from_utf8_lossy(&raw[..end]).trim().to_owned();
        return (!text.is_empty()).then_some(Value::Text(text));
    }
    let numbers = (0..count)
        .map(|i| {
            let o = pos + i * size;
            Some(match typ {
                1 | 7 => raw[i] as f64,
                3 => t.u16(o)? as f64,
                4 => t.u32(o)? as f64,
                8 => t.u16(o)? as i16 as f64,
                9 => t.u32(o)? as i32 as f64,
                5 => ratio(t.u32(o)? as f64, t.u32(o + 4)? as f64)?,
                10 => ratio(t.u32(o)? as i32 as f64, t.u32(o + 4)? as i32 as f64)?,
                _ => return None,
            })
        })
        .collect::<Option<Vec<f64>>>()?;
    Some(Value::Numbers(numbers))
}

fn ratio(n: f64, d: f64) -> Option<f64> {
    (d != 0.0).then_some(n / d)
}

fn parse(tiff: &[u8]) -> PhotoInfo {
    let Some(t) = Tiff::new(tiff) else { return PhotoInfo::default() };
    let Some(ifd0) = t.ifd0() else { return PhotoInfo::default() };
    let get = |ifd: usize, tag: u16| t.entry(ifd, tag).and_then(|e| value(&t, e));
    let text = |ifd: usize, tag: u16| match get(ifd, tag) {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    };
    let number = |ifd: Option<usize>, tag: u16| match get(ifd?, tag) {
        Some(Value::Numbers(n)) => n.first().copied(),
        _ => None,
    };
    let exif_ifd = number(Some(ifd0), 0x8769).map(|o| o as usize);
    let exif_text = |tag: u16| exif_ifd.and_then(|ifd| text(ifd, tag));

    let focal = number(exif_ifd, 0x920A).filter(|f| *f > 0.0);
    let focal_35 = number(exif_ifd, 0xA405).filter(|f| *f > 0.0);

    PhotoInfo {
        taken: exif_text(0x9003).or_else(|| text(ifd0, 0x0132)).map(|d| format_date(&d)),
        camera: camera(text(ifd0, 0x010F), text(ifd0, 0x0110)),
        lens: exif_text(0xA434),
        exposure_time: number(exif_ifd, 0x829A).filter(|v| *v > 0.0).map(format_exposure),
        aperture: number(exif_ifd, 0x829D).filter(|v| *v > 0.0).map(|f| format!("f/{}", de(f, 1))),
        iso: number(exif_ifd, 0x8827).or_else(|| number(exif_ifd, 0x8833)).map(|i| format!("ISO {i:.0}")),
        focal_length: focal.map(|f| match focal_35 {
            Some(kb) if (kb - f).abs() >= 1.0 => format!("{} mm ({kb:.0} mm KB)", de_short(f)),
            _ => format!("{} mm", de_short(f)),
        }),
        exposure_bias: number(exif_ifd, 0x9204).map(|b| {
            if b.abs() < 0.05 {
                "0 EV".to_owned()
            } else {
                format!("{}{} EV", if b > 0.0 { "+" } else { "−" }, de(b.abs(), 1))
            }
        }),
        flash: number(exif_ifd, 0x9209).map(|f| if f as u32 & 1 == 1 { "ausgelöst" } else { "nicht ausgelöst" }.to_owned()),
        software: text(ifd0, 0x0131),
    }
}

/// Zahl mit Dezimalkomma.
fn de(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}").replace('.', ",")
}

/// Ganze Zahlen ohne Nachkommastelle, sonst eine.
fn de_short(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 { format!("{v:.0}") } else { de(v, 1) }
}

fn format_exposure(t: f64) -> String {
    if t < 0.3 {
        format!("1/{:.0} s", 1.0 / t)
    } else {
        format!("{} s", de_short(t))
    }
}

/// „2024:04:13 10:21:54“ → „13.04.2024, 10:21:54“; Unbekanntes bleibt wie es ist.
fn format_date(d: &str) -> String {
    let (date, time) = d.split_once(' ').unwrap_or((d, ""));
    let parts: Vec<&str> = date.split(':').collect();
    match parts.as_slice() {
        [y, m, day] if !time.is_empty() => format!("{day}.{m}.{y}, {time}"),
        [y, m, day] => format!("{day}.{m}.{y}"),
        _ => d.to_owned(),
    }
}

/// Hersteller nur einmal: „OLYMPUS CORPORATION“ + „E-M10MarkII“ → „OLYMPUS E-M10MarkII“,
/// „Canon“ + „Canon EOS R6“ → „Canon EOS R6“.
fn camera(make: Option<String>, model: Option<String>) -> Option<String> {
    let brand = make.as_deref().and_then(|m| m.split_whitespace().next()).map(str::to_owned);
    match (brand, model) {
        (Some(b), Some(m)) if m.to_lowercase().starts_with(&b.to_lowercase()) => Some(m),
        (Some(b), Some(m)) => Some(format!("{b} {m}")),
        (None, Some(m)) => Some(m),
        (Some(_), None) => make,
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Entry = (u16, u16, u32, Vec<u8>);

    fn ascii(s: &str) -> (u16, u32, Vec<u8>) {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        (2, b.len() as u32, b)
    }
    fn short(v: u16) -> (u16, u32, Vec<u8>) {
        (3, 1, v.to_le_bytes().to_vec())
    }
    fn rational(n: u32, d: u32) -> (u16, u32, Vec<u8>) {
        (5, 1, [n.to_le_bytes(), d.to_le_bytes()].concat())
    }
    fn srational(n: i32, d: i32) -> (u16, u32, Vec<u8>) {
        (10, 1, [n.to_le_bytes(), d.to_le_bytes()].concat())
    }
    fn e(tag: u16, (typ, count, data): (u16, u32, Vec<u8>)) -> Entry {
        (tag, typ, count, data)
    }

    fn ifd_size(entries: &[Entry]) -> usize {
        let data: usize = entries.iter().map(|(.., d)| if d.len() > 4 { d.len() + d.len() % 2 } else { 0 }).sum();
        2 + entries.len() * 12 + 4 + data
    }

    fn write_ifd(out: &mut Vec<u8>, entries: &[Entry]) {
        let start = out.len();
        let mut data_at = start + 2 + entries.len() * 12 + 4;
        let mut data = Vec::new();
        out.extend((entries.len() as u16).to_le_bytes());
        for (tag, typ, count, bytes) in entries {
            out.extend(tag.to_le_bytes());
            out.extend(typ.to_le_bytes());
            out.extend(count.to_le_bytes());
            if bytes.len() <= 4 {
                let mut v = bytes.clone();
                v.resize(4, 0);
                out.extend(v);
            } else {
                out.extend((data_at as u32).to_le_bytes());
                data.extend(bytes);
                if bytes.len() % 2 == 1 {
                    data.push(0);
                }
                data_at += bytes.len() + bytes.len() % 2;
            }
        }
        out.extend(0u32.to_le_bytes());
        out.extend(data);
    }

    /// EXIF (Little Endian) aus IFD0- und Exif-IFD-Einträgen; der Exif-Zeiger wird ergänzt.
    fn build(ifd0: Vec<Entry>, exif_ifd: Vec<Entry>) -> Vec<u8> {
        let mut ifd0 = ifd0;
        ifd0.push((0x8769, 4, 1, vec![0; 4]));
        let exif_at = 8 + ifd_size(&ifd0);
        ifd0.last_mut().unwrap().3 = (exif_at as u32).to_le_bytes().to_vec();
        let mut out = b"II*\0".to_vec();
        out.extend(8u32.to_le_bytes());
        write_ifd(&mut out, &ifd0);
        assert_eq!(out.len(), exif_at);
        write_ifd(&mut out, &exif_ifd);
        out
    }

    #[test]
    fn reads_and_formats_the_usual_fields() {
        let tiff = build(
            vec![e(0x010F, ascii("OLYMPUS CORPORATION")), e(0x0110, ascii("E-M10MarkII")), e(0x0131, ascii("Version 1.4"))],
            vec![
                e(0x9003, ascii("2024:04:13 10:21:54")),
                e(0x829A, rational(1, 125)),
                e(0x829D, rational(40, 10)),
                e(0x8827, short(200)),
                e(0x920A, rational(140, 10)),
                e(0xA405, short(28)),
                e(0x9204, srational(-3, 10)),
                e(0x9209, short(0x18)),
                e(0xA434, ascii("OLYMPUS M.14-42mm F3.5-5.6 EZ")),
            ],
        );
        let info = parse(&tiff);
        assert_eq!(info.taken.as_deref(), Some("13.04.2024, 10:21:54"));
        assert_eq!(info.camera.as_deref(), Some("OLYMPUS E-M10MarkII"));
        assert_eq!(info.lens.as_deref(), Some("OLYMPUS M.14-42mm F3.5-5.6 EZ"));
        assert_eq!(info.exposure_time.as_deref(), Some("1/125 s"));
        assert_eq!(info.aperture.as_deref(), Some("f/4,0"));
        assert_eq!(info.iso.as_deref(), Some("ISO 200"));
        assert_eq!(info.focal_length.as_deref(), Some("14 mm (28 mm KB)"));
        assert_eq!(info.exposure_bias.as_deref(), Some("−0,3 EV"));
        assert_eq!(info.flash.as_deref(), Some("nicht ausgelöst"));
        assert_eq!(info.software.as_deref(), Some("Version 1.4"));
    }

    #[test]
    fn missing_or_broken_exif_gives_empty_fields() {
        assert_eq!(parse(b"kein exif"), PhotoInfo::default());
        let tiff = build(vec![e(0x0110, ascii("Canon EOS R6"))], vec![e(0x829A, rational(2, 1))]);
        let info = parse(&tiff);
        assert_eq!(info.camera.as_deref(), Some("Canon EOS R6"));
        assert_eq!(info.exposure_time.as_deref(), Some("2 s"));
        assert_eq!(info.lens, None);
    }

    #[test]
    fn camera_names() {
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(camera(s("Canon"), s("Canon EOS R6")), s("Canon EOS R6"));
        assert_eq!(camera(s("OLYMPUS IMAGING CORP."), s("XZ-1")), s("OLYMPUS XZ-1"));
        assert_eq!(camera(None, s("XZ-1")), s("XZ-1"));
    }
}
