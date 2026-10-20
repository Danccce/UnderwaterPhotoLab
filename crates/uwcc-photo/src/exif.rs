//! Liest das EXIF-Segment und das darin eingebettete Vorschaubild (IFD1) aus dem Anfang einer
//! JPEG-Datei, ohne das Bild zu dekodieren.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

/// Das EXIF-Segment ab dem TIFF-Kopf (ohne `Exif\0\0`), wie es auch `image` liefert.
pub(crate) fn read_segment(path: &Path) -> Option<Vec<u8>> {
    let mut r = BufReader::new(File::open(path).ok()?);
    let mut two = [0u8; 2];
    r.read_exact(&mut two).ok()?;
    if two != [0xFF, 0xD8] {
        return None;
    }
    loop {
        let mut byte = [0u8; 1];
        r.read_exact(&mut byte).ok()?;
        if byte[0] != 0xFF {
            return None;
        }
        // Füllbytes 0xFF vor dem eigentlichen Marker überspringen.
        while byte[0] == 0xFF {
            r.read_exact(&mut byte).ok()?;
        }
        let marker = byte[0];
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        r.read_exact(&mut two).ok()?;
        let len = u16::from_be_bytes(two) as usize;
        let mut segment = vec![0u8; len.checked_sub(2)?];
        r.read_exact(&mut segment).ok()?;
        if marker == 0xE1 && segment.starts_with(b"Exif\0\0") {
            segment.drain(..6);
            return Some(segment);
        }
    }
}

/// Die JPEG-Daten des Vorschaubilds aus IFD1 (Tags 0x0201/0x0202).
pub(crate) fn thumbnail_jpeg(tiff: &[u8]) -> Option<&[u8]> {
    let little = match tiff.get(0..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |o: usize| {
        let b: [u8; 2] = tiff.get(o..o + 2)?.try_into().ok()?;
        Some(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |o: usize| {
        let b: [u8; 4] = tiff.get(o..o + 4)?.try_into().ok()?;
        Some(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };

    let ifd0 = u32_at(4)? as usize;
    let entries0 = u16_at(ifd0)? as usize;
    let ifd1 = u32_at(ifd0 + 2 + entries0 * 12)? as usize;
    if ifd1 == 0 {
        return None;
    }
    let (mut offset, mut length) = (None, None);
    for i in 0..u16_at(ifd1)? as usize {
        let entry = ifd1 + 2 + i * 12;
        match u16_at(entry)? {
            0x0201 => offset = u32_at(entry + 8),
            0x0202 => length = u32_at(entry + 8),
            _ => {}
        }
    }
    let (offset, length) = (offset? as usize, length? as usize);
    tiff.get(offset..offset.checked_add(length)?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// TIFF/EXIF (Little Endian) mit Orientation in IFD0 und `thumbnail` als JPEG in IFD1.
    pub(crate) fn exif_with_thumbnail(orientation: u16, thumbnail: &[u8]) -> Vec<u8> {
        let entry = |tag: u16, typ: u16, value: u32| {
            [&tag.to_le_bytes()[..], &typ.to_le_bytes(), &1u32.to_le_bytes(), &value.to_le_bytes()].concat()
        };
        let ifd1 = 8 + 2 + 12 + 4;
        let data = ifd1 + 2 + 2 * 12 + 4;
        [
            &b"II*\0"[..],
            &8u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &entry(0x0112, 3, orientation as u32),
            &(ifd1 as u32).to_le_bytes(),
            &2u16.to_le_bytes(),
            &entry(0x0201, 4, data as u32),
            &entry(0x0202, 4, thumbnail.len() as u32),
            &0u32.to_le_bytes(),
            thumbnail,
        ]
        .concat()
    }

    #[test]
    fn finds_thumbnail_and_rejects_garbage() {
        let jpeg = [0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9];
        let tiff = exif_with_thumbnail(6, &jpeg);
        assert_eq!(thumbnail_jpeg(&tiff), Some(&jpeg[..]));
        assert_eq!(thumbnail_jpeg(b"nonsense"), None);
        assert_eq!(thumbnail_jpeg(&tiff[..tiff.len() - 2]), None, "abgeschnitten");
    }
}
