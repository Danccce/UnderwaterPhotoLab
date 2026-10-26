//! Histogramm der korrigierten Vorschau und Beschnitt-Warnung.

use slint::{Color, Image};

use crate::{with_state, LabWindow};

/// Ab diesem Anteil beschnittener Pixel leuchtet ein Dreieck (0,1 %). Die Auto-Korrektur
/// beschneidet absichtlich ein wenig; eine kleinere Schwelle leuchtet bei fast jedem Bild.
const CLIP_SHARE: u64 = 1_000;
/// Glättung nur für die Anzeige: die Tonwertspreizung lässt Lücken, die Kurve wäre sonst ein Kamm.
const KERNEL: [u64; 7] = [1, 2, 3, 4, 3, 2, 1];
/// Höhe der Pfade; die Breite ist 256 (ein Wert je Helligkeitsstufe).
const HEIGHT: f32 = 100.0;

struct Histogram {
    bins: [[u64; 256]; 3],
    pixels: u64,
}

impl Histogram {
    fn of(rgba: &[u8]) -> Self {
        let mut bins = [[0u64; 256]; 3];
        for px in rgba.chunks_exact(4) {
            bins[0][px[0] as usize] += 1;
            bins[1][px[1] as usize] += 1;
            bins[2][px[2] as usize] += 1;
        }
        Self { bins, pixels: (rgba.len() / 4) as u64 }
    }

    /// Gefüllte Kurven je Kanal. Alle Kanäle teilen eine Skala; die Ränder 0 und 255 zählen dafür
    /// nicht, sonst drückt ein Beschnitt-Ausschlag den Rest flach.
    fn paths(&self) -> [String; 3] {
        let smooth = self.bins.map(|c| smoothed(&c));
        let peak = smooth.iter().flat_map(|c| c[1..255].iter()).copied().fold(0.0f32, f32::max).max(1.0);
        smooth.map(|channel| {
            let mut d = format!("M 0 {HEIGHT}");
            for (i, &n) in channel.iter().enumerate() {
                let y = HEIGHT - (n / peak).min(1.0) * HEIGHT;
                d.push_str(&format!(" L {}.5 {y:.1}", i));
            }
            d.push_str(&format!(" L 256 {HEIGHT} Z"));
            d
        })
    }

    /// Farbe des Dreiecks: die beschnittenen Kanäle gemischt (Rot, Gelb, Weiß …), sonst durchsichtig.
    fn clip_color(&self, bin: usize) -> Color {
        let limit = self.pixels / CLIP_SHARE;
        let on = |c: usize| if self.bins[c][bin] > limit { 255 } else { 0 };
        let (r, g, b) = (on(0), on(1), on(2));
        if r == 0 && g == 0 && b == 0 {
            Color::from_argb_u8(0, 0, 0, 0)
        } else {
            Color::from_rgb_u8(r, g, b)
        }
    }
}

/// Gleitender Mittelwert über 1..=254; die Randwerte 0 und 255 bleiben roh und fließen nicht ein,
/// damit ein Beschnitt-Ausschlag nicht in die Nachbarn verschmiert.
fn smoothed(bins: &[u64; 256]) -> [f32; 256] {
    let mut out = [0.0; 256];
    out[0] = bins[0] as f32;
    out[255] = bins[255] as f32;
    for (i, o) in out.iter_mut().enumerate().take(255).skip(1) {
        let (mut sum, mut weight) = (0, 0);
        for (k, w) in KERNEL.iter().enumerate() {
            let j = i as isize + k as isize - 3;
            if (1..255).contains(&j) {
                sum += bins[j as usize] * w;
                weight += w;
            }
        }
        *o = sum as f32 / weight as f32;
    }
    out
}

/// Lichter mit einem Kanal bei 255 werden rot, Tiefen mit einem Kanal bei 0 blau.
fn mark_clipping(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        if px[0] == 255 || px[1] == 255 || px[2] == 255 {
            px[..3].copy_from_slice(&[255, 0, 0]);
        } else if px[0] == 0 || px[1] == 0 || px[2] == 0 {
            px[..3].copy_from_slice(&[0, 96, 255]);
        }
    }
}

/// Rechnet die korrigierte Vorschau des aktuellen Bildes, setzt sie samt Histogramm und
/// markiert bei eingeschalteter Warnung die beschnittenen Stellen.
pub fn show_corrected(app: &LabWindow) {
    let Some(mut buf) = with_state(|st| st.current.as_ref().map(|l| l.corrected_buffer())) else {
        return;
    };
    let hist = Histogram::of(buf.as_bytes());
    let [r, g, b] = hist.paths();
    app.set_hist_red(r.into());
    app.set_hist_green(g.into());
    app.set_hist_blue(b.into());
    app.set_clip_shadows(hist.clip_color(0));
    app.set_clip_highlights(hist.clip_color(255));
    if app.get_clip_warning() {
        mark_clipping(buf.make_mut_bytes());
    }
    app.set_preview_corrected(Image::from_rgba8(buf));
}

/// Ohne geladenes Bild bleibt das Histogramm leer.
pub fn clear(app: &LabWindow) {
    app.set_hist_red("".into());
    app.set_hist_green("".into());
    app.set_hist_blue("".into());
    app.set_clip_shadows(Color::from_argb_u8(0, 0, 0, 0));
    app.set_clip_highlights(Color::from_argb_u8(0, 0, 0, 0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_channels_mix_into_the_triangle_color() {
        let mut px = vec![128u8; 4 * 100_000];
        for p in px.chunks_exact_mut(4).take(500) {
            p[0] = 255;
            p[1] = 255;
        }
        let h = Histogram::of(&px);
        assert_eq!(h.clip_color(255), Color::from_rgb_u8(255, 255, 0));
        assert_eq!(h.clip_color(0).alpha(), 0);
    }

    #[test]
    fn smoothing_fills_gaps_but_keeps_the_edges() {
        let mut bins = [0u64; 256];
        bins[255] = 1000;
        for i in (10..30).step_by(3) {
            bins[i] = 30;
        }
        let s = smoothed(&bins);
        assert!(s[11] > 0.0 && s[12] > 0.0);
        assert_eq!(s[255], 1000.0);
        assert_eq!(s[254], 0.0);
    }

    #[test]
    fn paths_span_the_full_width() {
        let h = Histogram::of(&[10, 20, 30, 255, 200, 200, 200, 255]);
        for p in h.paths() {
            assert!(p.starts_with("M 0 100") && p.ends_with("L 256 100 Z"));
        }
    }

    #[test]
    fn marking_prefers_highlights() {
        let mut px = [255, 0, 10, 255, 0, 50, 50, 255, 50, 50, 50, 255];
        mark_clipping(&mut px);
        assert_eq!(&px[0..3], &[255, 0, 0]);
        assert_eq!(&px[4..7], &[0, 96, 255]);
        assert_eq!(&px[8..11], &[50, 50, 50]);
    }
}
