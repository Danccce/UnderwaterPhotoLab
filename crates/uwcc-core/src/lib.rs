//! Unterwasser-Farbkorrektur: Analyse und Farbmatrix.
//!
//! Rust-Portierung von `getColorFilterMatrix` aus „underwater-image-color-correction“ von
//! Nikolaj Bech Andersen, <https://github.com/nikolajbech/underwater-image-color-correction>,
//! lizenziert unter GPL-3.0. Die Analyse folgt dem Original Schritt für Schritt, damit die
//! Ergebnisse mit dessen Matrizen übereinstimmen.
//!
//! `no_std`, damit das PhotoCraft-Plug-in denselben Code nutzen kann.
#![no_std]

pub mod adjust;
pub mod params;

pub use adjust::{process_rgba8, Delta, LevelsDelta, Settings};

use core::f64::consts::PI;

const THRESHOLD_RATIO: f64 = 2000.0;
const MIN_AVG_RED: f64 = 60.0;
const MAX_HUE_SHIFT: u32 = 120;
const BLUE_MAGIC_VALUE: f64 = 1.2;

/// Schwarz- und Weißpunkt eines Kanals auf der Skala 0–255 des Originals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub low: f64,
    pub high: f64,
}

impl Levels {
    /// Keine Spreizung: das Original rechnet mit `256 / (high − low)`.
    pub const FULL: Self = Self { low: 0.0, high: 256.0 };

    fn gain_offset(self) -> (f64, f64) {
        let high = self.high.max(self.low + 1.0);
        let gain = 256.0 / (high - self.low);
        (gain, (-self.low / 256.0) * gain)
    }
}

/// Die freien Werte der Farbmatrix: Hue-Shift in Grad und die Levels je Kanal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Correction {
    pub hue_shift: f64,
    pub red: Levels,
    pub green: Levels,
    pub blue: Levels,
}

impl Correction {
    pub const NONE: Self = Self {
        hue_shift: 0.0,
        red: Levels::FULL,
        green: Levels::FULL,
        blue: Levels::FULL,
    };

    /// 4×5-Farbmatrix im Format von `getColorFilterMatrix` (Zeilen R, G, B, A; Spalte 5 ist der
    /// Offset in Einheiten des vollen Wertebereichs).
    pub fn matrix(&self) -> [f64; 20] {
        let [wr, wg, wb] = red_weights(self.hue_shift);
        let (red_gain, red_offset) = self.red.gain_offset();
        let (green_gain, green_offset) = self.green.gain_offset();
        let (blue_gain, blue_offset) = self.blue.gain_offset();
        [
            wr * red_gain, wg * red_gain, wb * red_gain * BLUE_MAGIC_VALUE, 0.0, red_offset,
            0.0, green_gain, 0.0, 0.0, green_offset,
            0.0, 0.0, blue_gain, 0.0, blue_offset,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]
    }
}

/// Die Farbmatrix in `f32` für die Anwendung auf normalisierte Pixel (0–1).
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    m: [f32; 20],
}

impl Transform {
    pub fn new(correction: &Correction) -> Self {
        Self { m: correction.matrix().map(|v| v as f32) }
    }

    /// Wendet die Matrix an, ohne zu begrenzen.
    #[inline(always)]
    pub fn apply(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let m = &self.m;
        (
            r * m[0] + g * m[1] + b * m[2] + m[4],
            g * m[6] + m[9],
            b * m[12] + m[14],
        )
    }
}

/// Rotzeile der Hue-Rotation um `hue_shift` Grad (`hueShiftRed` im Original).
pub fn red_weights(hue_shift: f64) -> [f64; 3] {
    let u = libm::cos(hue_shift * PI / 180.0);
    let w = libm::sin(hue_shift * PI / 180.0);
    [
        0.299 + 0.701 * u + 0.168 * w,
        0.587 - 0.587 * u + 0.330 * w,
        0.114 - 0.114 * u - 0.497 * w,
    ]
}

fn shifted_red(r: f64, g: f64, b: f64, hue_shift: f64) -> f64 {
    let [wr, wg, wb] = red_weights(hue_shift);
    wr * r + wg * g + wb * b
}

/// Ermittelt die Korrektur für ein ganzes Bild aus 8-Bit-RGBA-Pixeln
/// (`getColorFilterMatrix` im Original).
pub fn analyze(rgba: &[u8]) -> Correction {
    let num_of_pixels = (rgba.len() / 4) as f64;
    if num_of_pixels == 0.0 {
        return Correction::NONE;
    }
    let threshold_level = num_of_pixels / THRESHOLD_RATIO;

    let (mut sum_r, mut sum_g, mut sum_b) = (0.0f64, 0.0f64, 0.0f64);
    for p in rgba.chunks_exact(4) {
        sum_r += p[0] as f64;
        sum_g += p[1] as f64;
        sum_b += p[2] as f64;
    }
    let (avg_r, avg_g, avg_b) = (
        sum_r / num_of_pixels,
        sum_g / num_of_pixels,
        sum_b / num_of_pixels,
    );

    // Wie im Original wird der Winkel nach dem Treffer noch einmal erhöht.
    let mut hue_shift = 0u32;
    let mut new_avg_red = avg_r;
    while new_avg_red < MIN_AVG_RED {
        new_avg_red = shifted_red(avg_r, avg_g, avg_b, hue_shift as f64);
        hue_shift += 1;
        if hue_shift > MAX_HUE_SHIFT {
            new_avg_red = MIN_AVG_RED;
        }
    }
    let hue_shift = hue_shift as f64;

    let mut hist = [[0u32; 256]; 3];
    for p in rgba.chunks_exact(4) {
        let red = libm::round(
            shifted_red(p[0] as f64, p[1] as f64, p[2] as f64, hue_shift).clamp(0.0, 255.0),
        );
        hist[0][red as usize] += 1;
        hist[1][p[1] as usize] += 1;
        hist[2][p[2] as usize] += 1;
    }

    Correction {
        hue_shift,
        red: normalizing_interval(&hist[0], threshold_level),
        green: normalizing_interval(&hist[1], threshold_level),
        blue: normalizing_interval(&hist[2], threshold_level),
    }
}

/// Größte Lücke zwischen dünn besetzten Histogrammwerten, eingerahmt von 0 und 255.
fn normalizing_interval(hist: &[u32; 256], threshold_level: f64) -> Levels {
    let sparse = (0..256u32).filter(|&i| (hist[i as usize] as f64) - threshold_level < 2.0);
    let (mut low, mut high, mut max_dist) = (0u32, 255u32, 0u32);
    let mut prev = 0u32;
    for v in sparse.chain(core::iter::once(255)) {
        let dist = v - prev;
        if dist > max_dist {
            max_dist = dist;
            high = v;
            low = prev;
        }
        prev = v;
    }
    Levels { low: low as f64, high: high as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_identity() {
        let t = Transform::new(&Correction::NONE);
        let (r, g, b) = t.apply(0.25, 0.5, 0.75);
        assert!((r - 0.25).abs() < 1e-6 && (g - 0.5).abs() < 1e-6 && (b - 0.75).abs() < 1e-6);
    }

    #[test]
    fn bluish_image_gets_red() {
        let px: [u8; 8] = [10, 80, 120, 255, 20, 100, 160, 255];
        let c = analyze(&px);
        assert!(c.hue_shift > 0.0);
    }
}
