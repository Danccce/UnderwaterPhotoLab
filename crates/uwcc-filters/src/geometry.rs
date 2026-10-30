//! Geometrie in einem Durchgang: Spiegeln, Vierteldrehungen, Geraderichten und Zuschnitt.
//!
//! Reihenfolge: das aufrechte Bild wird (falls gewählt) waagerecht gespiegelt, dann in Vierteln
//! im Uhrzeigersinn gedreht – das ergibt den *Rahmen*. Geraderichten dreht den Inhalt um seine
//! Mitte und vergrößert ihn so weit, dass keine leeren Ecken bleiben; der Rahmen behält seine
//! Größe. Der Zuschnitt (Anteile 0–1) bezieht sich auf diesen Rahmen. Jedes Ausgabepixel wird
//! einmal aus dem aufrechten Bild abgetastet: ohne Geraderichten exakt, sonst bilinear.

use uwcc_core::{Crop, Geometry};

/// Größter Winkel fürs Geraderichten (Grad).
pub const MAX_STRAIGHTEN: f32 = 45.0;

/// Kein Spiegeln, Drehen, Geraderichten oder Zuschnitt.
pub fn is_identity(g: &Geometry) -> bool {
    g.quarter_turns % 4 == 0 && !g.flip_horizontal && g.straighten == 0.0 && crop_of(g).is_none()
}

/// Der Zuschnitt, sofern er nicht das ganze Bild umfasst.
fn crop_of(g: &Geometry) -> Option<Crop> {
    g.crop.filter(|c| c.x > 0.0 || c.y > 0.0 || c.width < 1.0 || c.height < 1.0)
}

/// Größe des Rahmens für ein aufrechtes Bild `w × h`: bei ungeraden Vierteln vertauscht.
pub fn frame_size(w: u32, h: u32, g: &Geometry) -> (u32, u32) {
    if g.quarter_turns % 2 == 1 {
        (h, w)
    } else {
        (w, h)
    }
}

/// Zuschnitt in Pixeln des Rahmens `fw × fh`: Anfang und Größe, mindestens ein Pixel, im Rahmen.
fn crop_px(fw: u32, fh: u32, g: &Geometry) -> (u32, u32, u32, u32) {
    let Some(c) = crop_of(g) else { return (0, 0, fw, fh) };
    let span = |start: f32, len: f32, size: u32| {
        let s = ((start.clamp(0.0, 1.0) * size as f32).round() as u32).min(size - 1);
        let l = ((len.clamp(0.0, 1.0) * size as f32).round() as u32).clamp(1, size - s);
        (s, l)
    };
    let (x, w) = span(c.x, c.width, fw);
    let (y, h) = span(c.y, c.height, fh);
    (x, y, w, h)
}

/// Größe des Ergebnisses für ein aufrechtes Bild `w × h`.
pub fn output_size(w: u32, h: u32, g: &Geometry) -> (u32, u32) {
    let (fw, fh) = frame_size(w, h, g);
    let (_, _, cw, ch) = crop_px(fw, fh, g);
    (cw, ch)
}

/// Vergrößerung beim Geraderichten um `deg`, damit der gedrehte Rahmen `fw × fh` ihn ganz füllt.
pub fn straighten_zoom(fw: u32, fh: u32, deg: f32) -> f32 {
    let a = deg.abs().to_radians();
    let ratio = fw.max(fh) as f32 / fw.min(fh).max(1) as f32;
    a.cos() + ratio * a.sin()
}

/// Umrechnung zwischen Punkten (Pixel, stetig) des aufrechten Bildes `w × h` und des Ergebnisses.
pub struct Mapping {
    sw: f32,
    sh: f32,
    fw: f32,
    fh: f32,
    q: u8,
    flip: bool,
    deg: f32,
    sin: f32,
    cos: f32,
    zoom: f32,
    /// Anfang des Zuschnitts im Rahmen.
    cx: f32,
    cy: f32,
}

impl Mapping {
    pub fn new(w: u32, h: u32, g: &Geometry) -> Self {
        let (fw, fh) = frame_size(w, h, g);
        let (cx, cy, _, _) = crop_px(fw, fh, g);
        let deg = g.straighten.clamp(-MAX_STRAIGHTEN, MAX_STRAIGHTEN);
        let (sin, cos) = deg.to_radians().sin_cos();
        Self {
            sw: w as f32,
            sh: h as f32,
            fw: fw as f32,
            fh: fh as f32,
            q: g.quarter_turns % 4,
            flip: g.flip_horizontal,
            deg,
            sin,
            cos,
            zoom: straighten_zoom(fw, fh, deg),
            cx: cx as f32,
            cy: cy as f32,
        }
    }

    /// Vergrößerung durch das Geraderichten (Längen im Ergebnis = im aufrechten Bild × diese).
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Rahmenpunkt → Punkt im aufrechten Bild.
    fn source(&self, fx: f32, fy: f32) -> (f32, f32) {
        let (mut tx, mut ty) = (fx, fy);
        if self.deg != 0.0 {
            let (u, v) = (fx - self.fw / 2.0, fy - self.fh / 2.0);
            // Inhalt im Uhrzeigersinn gedreht: zurück gegen den Uhrzeigersinn, dann verkleinert.
            tx = self.fw / 2.0 + (u * self.cos + v * self.sin) / self.zoom;
            ty = self.fh / 2.0 + (-u * self.sin + v * self.cos) / self.zoom;
        }
        let (x, y) = match self.q {
            0 => (tx, ty),
            1 => (ty, self.sh - tx),
            2 => (self.sw - tx, self.sh - ty),
            _ => (self.sw - ty, tx),
        };
        (if self.flip { self.sw - x } else { x }, y)
    }

    /// Punkt im Ergebnis → Punkt im aufrechten Bild.
    pub fn to_upright(&self, ox: f32, oy: f32) -> (f32, f32) {
        self.source(self.cx + ox, self.cy + oy)
    }

    /// Punkt im aufrechten Bild → Punkt im Ergebnis (Umkehrung von [`Self::to_upright`]).
    pub fn to_output(&self, sx: f32, sy: f32) -> (f32, f32) {
        let x = if self.flip { self.sw - sx } else { sx };
        let (tx, ty) = match self.q {
            0 => (x, sy),
            1 => (self.sh - sy, x),
            2 => (self.sw - x, self.sh - sy),
            _ => (sy, self.sw - x),
        };
        let (mut fx, mut fy) = (tx, ty);
        if self.deg != 0.0 {
            let (u, v) = (tx - self.fw / 2.0, ty - self.fh / 2.0);
            fx = self.fw / 2.0 + self.zoom * (u * self.cos - v * self.sin);
            fy = self.fh / 2.0 + self.zoom * (u * self.sin + v * self.cos);
        }
        (fx - self.cx, fy - self.cy)
    }
}

/// Wendet `g` auf das aufrechte RGBA-Bild `w × h` an; Ergebnis und seine Größe.
pub fn transform(rgba: &[u8], w: u32, h: u32, g: &Geometry) -> (Vec<u8>, u32, u32) {
    if is_identity(g) {
        return (rgba.to_vec(), w, h);
    }
    let (fw, fh) = frame_size(w, h, g);
    let (cx, cy, ow, oh) = crop_px(fw, fh, g);
    let map = Mapping::new(w, h, g);
    let deg = map.deg;
    let (sw, sh) = (w as f32, h as f32);
    let source = |fx: f32, fy: f32| map.source(fx, fy);

    let mut out = vec![0u8; ow as usize * oh as usize * 4];
    let stride = w as usize * 4;
    let pixel = |x: usize, y: usize| &rgba[y * stride + x * 4..y * stride + x * 4 + 4];
    crate::par_rows(&mut out, ow as usize * 4, |oy, row| {
        for (ox, p) in row.chunks_exact_mut(4).enumerate() {
            let (sx, sy) = source((cx as usize + ox) as f32 + 0.5, (cy as usize + oy) as f32 + 0.5);
            if deg == 0.0 {
                let x = (sx.floor() as isize).clamp(0, w as isize - 1) as usize;
                let y = (sy.floor() as isize).clamp(0, h as isize - 1) as usize;
                p.copy_from_slice(pixel(x, y));
            } else {
                let (gx, gy) = ((sx - 0.5).clamp(0.0, sw - 1.0), (sy - 0.5).clamp(0.0, sh - 1.0));
                let (x0, y0) = (gx as usize, gy as usize);
                let (x1, y1) = ((x0 + 1).min(w as usize - 1), (y0 + 1).min(h as usize - 1));
                let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
                for c in 0..4 {
                    let at = |x, y| pixel(x, y)[c] as f32;
                    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
                    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
                    p[c] = (top + (bottom - top) * fy).round() as u8;
                }
            }
        }
    });
    (out, ow, oh)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 × 2 Pixel mit eindeutigen Werten (Rot = Index).
    fn tiny() -> Vec<u8> {
        (0..6u8).flat_map(|i| [i, 0, 0, 255]).collect()
    }

    fn reds(px: &[u8]) -> Vec<u8> {
        px.chunks_exact(4).map(|p| p[0]).collect()
    }

    fn turned(q: u8, flip: bool) -> (Vec<u8>, u32, u32) {
        transform(&tiny(), 3, 2, &Geometry { quarter_turns: q, flip_horizontal: flip, ..Geometry::default() })
    }

    // Ausgang:  0 1 2
    //           3 4 5
    #[test]
    fn quarter_turns_and_flip_move_pixels_exactly() {
        let (px, w, h) = turned(1, false);
        assert_eq!((w, h), (2, 3));
        assert_eq!(reds(&px), [3, 0, 4, 1, 5, 2], "90° im Uhrzeigersinn");
        assert_eq!(reds(&turned(2, false).0), [5, 4, 3, 2, 1, 0], "180°");
        assert_eq!(reds(&turned(3, false).0), [2, 5, 1, 4, 0, 3], "90° gegen den Uhrzeigersinn");
        assert_eq!(reds(&turned(0, true).0), [2, 1, 0, 5, 4, 3], "gespiegelt");
        assert_eq!(reds(&turned(1, true).0), [5, 2, 4, 1, 3, 0], "erst spiegeln, dann drehen");
        assert_eq!(reds(&turned(4, false).0), reds(&tiny()), "vier Viertel = unverändert");
    }

    #[test]
    fn mapping_to_output_reverses_to_upright() {
        let g = Geometry {
            quarter_turns: 3,
            flip_horizontal: true,
            straighten: 7.0,
            crop: Some(Crop { x: 0.1, y: 0.2, width: 0.6, height: 0.5 }),
        };
        let m = Mapping::new(400, 300, &g);
        for &(ox, oy) in &[(0.0, 0.0), (37.5, 81.25), (120.0, 10.0)] {
            let (sx, sy) = m.to_upright(ox, oy);
            let (bx, by) = m.to_output(sx, sy);
            assert!((bx - ox).abs() < 1e-3 && (by - oy).abs() < 1e-3, "({ox}, {oy}) → ({bx}, {by})");
        }
    }

    #[test]
    fn crop_is_pixel_exact() {
        let (w, h) = (100u32, 80u32);
        let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, (i / 251) as u8, 7, 255]).collect();
        let g = Geometry { crop: Some(Crop { x: 0.25, y: 0.5, width: 0.5, height: 0.25 }), ..Geometry::default() };
        let (out, ow, oh) = transform(&px, w, h, &g);
        assert_eq!((ow, oh), (50, 20));
        assert_eq!(output_size(w, h, &g), (50, 20));
        for y in 0..oh {
            for x in 0..ow {
                let o = ((y * ow + x) * 4) as usize;
                let s = (((y + 40) * w + x + 25) * 4) as usize;
                assert_eq!(out[o..o + 4], px[s..s + 4]);
            }
        }
    }

    #[test]
    fn full_crop_and_no_turn_is_the_identity() {
        let g = Geometry { crop: Some(Crop { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }), ..Geometry::default() };
        assert!(is_identity(&g));
        assert_eq!(transform(&tiny(), 3, 2, &g), (tiny(), 3, 2));
    }

    #[test]
    fn straightening_keeps_the_frame_filled_and_the_centre_in_place() {
        let (w, h) = (120u32, 80u32);
        // Senkrechte Streifen: nach Geraderichten weiter Streifen, aber schräg.
        let px: Vec<u8> = (0..w * h).flat_map(|i| if (i % w) / 10 % 2 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
        let g = Geometry { straighten: 10.0, ..Geometry::default() };
        let (out, ow, oh) = transform(&px, w, h, &g);
        assert_eq!((ow, oh), (w, h), "Rahmen bleibt gleich groß");
        assert!(straighten_zoom(w, h, 10.0) > 1.0);
        // Die Mitte bleibt die Mitte: x = 65 liegt mitten im weißen Streifen 60–69.
        let mid = |img: &[u8]| img[((40 * w + 65) * 4) as usize];
        assert_eq!(mid(&out), mid(&px));
        // Oben und unten derselbe Streifen an verschiedenen x: schräg.
        let row_white = |y: u32| (0..w).filter(|&x| out[((y * w + x) * 4) as usize] > 128).collect::<Vec<_>>();
        assert_ne!(row_white(5), row_white(75));
        assert!(out.chunks_exact(4).all(|p| p[3] == 255), "keine leeren Ecken");
    }
}
