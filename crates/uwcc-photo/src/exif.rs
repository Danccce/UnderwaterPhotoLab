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

/// Größtes EXIF (ab TIFF-Kopf), das in ein APP1-Segment passt: 65535 − 2 (Länge) − 6 (`Exif\0\0`).
const MAX_EXIF: usize = 65_527;

const TAG_COMPRESSION: u16 = 0x0103;
const TAG_THUMB_OFFSET: u16 = 0x0201;
const TAG_THUMB_LENGTH: u16 = 0x0202;
const TYPE_SHORT: u16 = 3;
const TYPE_LONG: u16 = 4;

/// Lesezugriff auf ein TIFF/EXIF-Segment in seiner Byte-Reihenfolge.
struct Tiff<'a> {
    data: &'a [u8],
    little: bool,
}

impl<'a> Tiff<'a> {
    fn new(data: &'a [u8]) -> Option<Self> {
        let little = match data.get(0..4)? {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        Some(Self { data, little })
    }

    fn u16(&self, o: usize) -> Option<u16> {
        let b: [u8; 2] = self.data.get(o..o + 2)?.try_into().ok()?;
        Some(if self.little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    }

    fn u32(&self, o: usize) -> Option<u32> {
        let b: [u8; 4] = self.data.get(o..o + 4)?.try_into().ok()?;
        Some(if self.little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }

    /// Position des Zeigers von IFD0 auf IFD1.
    fn next_ifd_pointer(&self) -> Option<usize> {
        let ifd0 = self.u32(4)? as usize;
        Some(ifd0 + 2 + self.u16(ifd0)? as usize * 12)
    }

    fn ifd1(&self) -> Option<usize> {
        let ifd1 = self.u32(self.next_ifd_pointer()?)? as usize;
        (ifd1 != 0).then_some(ifd1)
    }

    /// Positionen der IFD1-Einträge für Offset und Länge des Vorschaubilds.
    fn thumbnail_entries(&self, ifd1: usize) -> Option<(usize, usize)> {
        let (mut offset, mut length) = (None, None);
        for i in 0..self.u16(ifd1)? as usize {
            let entry = ifd1 + 2 + i * 12;
            match self.u16(entry)? {
                TAG_THUMB_OFFSET => offset = Some(entry),
                TAG_THUMB_LENGTH => length = Some(entry),
                _ => {}
            }
        }
        Some((offset?, length?))
    }

    fn put_u16(&self, out: &mut [u8], o: usize, v: u16) {
        let b = if self.little { v.to_le_bytes() } else { v.to_be_bytes() };
        out[o..o + 2].copy_from_slice(&b);
    }

    fn put_u32(&self, out: &mut [u8], o: usize, v: u32) {
        let b = if self.little { v.to_le_bytes() } else { v.to_be_bytes() };
        out[o..o + 4].copy_from_slice(&b);
    }
}

/// Die JPEG-Daten des Vorschaubilds aus IFD1 (Tags 0x0201/0x0202).
pub(crate) fn thumbnail_jpeg(tiff: &[u8]) -> Option<&[u8]> {
    let t = Tiff::new(tiff)?;
    let (offset_entry, length_entry) = t.thumbnail_entries(t.ifd1()?)?;
    let offset = t.u32(offset_entry + 8)? as usize;
    let length = t.u32(length_entry + 8)? as usize;
    tiff.get(offset..offset.checked_add(length)?)
}

/// Das EXIF mit `jpeg` als Vorschaubild: ersetzt ein vorhandenes (liegt es am Ende, wird sein
/// Platz wiederverwendet) oder legt IFD1 neu an. `None`, wenn das nicht geht oder das Ergebnis
/// nicht mehr in ein APP1-Segment passt.
pub(crate) fn with_thumbnail(tiff: &[u8], jpeg: &[u8]) -> Option<Vec<u8>> {
    let t = Tiff::new(tiff)?;
    let next_pointer = t.next_ifd_pointer()?;
    let mut out = tiff.to_vec();
    if out.len() < next_pointer + 4 {
        return None;
    }
    let pad = |out: &mut Vec<u8>| {
        if out.len() % 2 == 1 {
            out.push(0);
        }
    };

    match t.ifd1() {
        Some(ifd1) => {
            let (offset_entry, length_entry) = t.thumbnail_entries(ifd1)?;
            let old_offset = t.u32(offset_entry + 8)? as usize;
            let old_length = t.u32(length_entry + 8)? as usize;
            if old_offset > ifd1 && old_offset.checked_add(old_length) == Some(out.len()) {
                out.truncate(old_offset);
            }
            pad(&mut out);
            let offset = out.len();
            out.extend_from_slice(jpeg);
            t.put_u32(&mut out, offset_entry + 8, offset as u32);
            t.put_u32(&mut out, length_entry + 8, jpeg.len() as u32);
        }
        None => {
            pad(&mut out);
            let ifd1 = out.len();
            let data = ifd1 + 2 + 3 * 12 + 4;
            out.resize(data, 0);
            t.put_u16(&mut out, ifd1, 3);
            for (i, (tag, typ, value)) in [
                (TAG_COMPRESSION, TYPE_SHORT, 6u32),
                (TAG_THUMB_OFFSET, TYPE_LONG, data as u32),
                (TAG_THUMB_LENGTH, TYPE_LONG, jpeg.len() as u32),
            ]
            .into_iter()
            .enumerate()
            {
                let e = ifd1 + 2 + i * 12;
                t.put_u16(&mut out, e, tag);
                t.put_u16(&mut out, e + 2, typ);
                t.put_u32(&mut out, e + 4, 1);
                if typ == TYPE_SHORT {
                    t.put_u16(&mut out, e + 8, value as u16);
                } else {
                    t.put_u32(&mut out, e + 8, value);
                }
            }
            out.extend_from_slice(jpeg);
            t.put_u32(&mut out, next_pointer, ifd1 as u32);
        }
    }
    (out.len() <= MAX_EXIF).then_some(out)
}

/// Das EXIF ohne Vorschaubild (IFD1 abgehängt), damit kein veraltetes Bild angezeigt wird.
pub(crate) fn without_thumbnail(tiff: &[u8]) -> Vec<u8> {
    let mut out = tiff.to_vec();
    if let Some(t) = Tiff::new(tiff) {
        if let Some(p) = t.next_ifd_pointer().filter(|p| p + 4 <= out.len()) {
            t.put_u32(&mut out, p, 0);
        }
    }
    out
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

    /// TIFF/EXIF (Big Endian) nur mit IFD0 (Orientation), ohne Vorschaubild.
    fn exif_without_ifd1(orientation: u16) -> Vec<u8> {
        [
            &b"MM\0*"[..],
            &8u32.to_be_bytes(),
            &1u16.to_be_bytes(),
            &0x0112u16.to_be_bytes(),
            &3u16.to_be_bytes(),
            &1u32.to_be_bytes(),
            &orientation.to_be_bytes(),
            &0u16.to_be_bytes(),
            &0u32.to_be_bytes(),
        ]
        .concat()
    }

    fn orientation(tiff: &[u8]) -> Option<image::metadata::Orientation> {
        image::metadata::Orientation::from_exif_chunk(tiff)
    }

    #[test]
    fn replaces_a_thumbnail_at_the_end() {
        let old = [0xFF, 0xD8, 1, 2, 3, 4, 5, 6, 0xFF, 0xD9];
        let new = [0xFF, 0xD8, 9, 9, 0xFF, 0xD9];
        let tiff = exif_with_thumbnail(6, &old);
        let out = with_thumbnail(&tiff, &new).unwrap();
        assert_eq!(thumbnail_jpeg(&out), Some(&new[..]));
        assert_eq!(out.len(), tiff.len() - old.len() + new.len(), "Platz wiederverwendet");
        assert_eq!(orientation(&out), orientation(&tiff));
    }

    #[test]
    fn appends_when_the_old_thumbnail_is_not_at_the_end() {
        let old = [0xFF, 0xD8, 1, 0xFF, 0xD9];
        let new = [0xFF, 0xD8, 7, 7, 7, 0xFF, 0xD9];
        let mut tiff = exif_with_thumbnail(1, &old);
        tiff.extend_from_slice(b"weitere Daten");
        let out = with_thumbnail(&tiff, &new).unwrap();
        assert_eq!(thumbnail_jpeg(&out), Some(&new[..]));
        assert!(out.ends_with(&new), "angehängt");
        assert!(out[..tiff.len()].ends_with(b"weitere Daten"), "folgende Daten unverändert");
    }

    #[test]
    fn adds_ifd1_when_there_is_none() {
        let tiff = exif_without_ifd1(8);
        assert_eq!(thumbnail_jpeg(&tiff), None);
        let new = [0xFF, 0xD8, 5, 0xFF, 0xD9];
        let out = with_thumbnail(&tiff, &new).unwrap();
        assert_eq!(thumbnail_jpeg(&out), Some(&new[..]));
        assert_eq!(orientation(&out), orientation(&tiff));
    }

    #[test]
    fn refuses_oversized_and_can_drop_the_thumbnail() {
        let tiff = exif_with_thumbnail(1, &[0xFF, 0xD8, 0xFF, 0xD9]);
        assert!(with_thumbnail(&tiff, &vec![0u8; MAX_EXIF]).is_none());
        let dropped = without_thumbnail(&tiff);
        assert_eq!(thumbnail_jpeg(&dropped), None);
        assert_eq!(orientation(&dropped), orientation(&tiff));
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
