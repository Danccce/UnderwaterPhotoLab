//! Filter mit Nachbarschaft und die gemeinsame Pipeline, die Vorschau, 100-%-Ansicht und Export
//! rechnen.
//!
//! Radien werden in Promille der langen Kante des *ganzen* Bildes in der gerade gerechneten
//! Auflösung angegeben ([`Context::radius_px`]). Damit wirkt ein Filter in der 1280-px-Vorschau wie
//! im Export, und ein Ausschnitt der 100-%-Ansicht wie das ganze Bild in voller Auflösung.

pub mod plane;

use std::thread;

use uwcc_core::{process_rgba8, Correction, Settings};

/// Was jeder Ausschnitt eines Bildes gemeinsam hat.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    /// Auto-Analyse des ganzen Bildes.
    pub auto: Correction,
    /// Lange Kante des ganzen Bildes in der gerade gerechneten Auflösung (Pixel).
    pub long_edge: u32,
}

impl Context {
    pub fn new(auto: Correction, width: u32, height: u32) -> Self {
        Self { auto, long_edge: width.max(height) }
    }

    /// Radius in Pixeln für `permille` Promille der langen Kante.
    pub fn radius_px(&self, permille: f32) -> f32 {
        permille * self.long_edge as f32 / 1000.0
    }
}

/// Die Pipeline für 8-Bit-RGBA: `width × height` Pixel, gerechnet mit `settings`.
/// Bisher nur die Farbkorrektur je Pixel, auf alle Kerne verteilt; Ton, Dehaze und Details
/// kommen in den folgenden Phasen hinzu.
pub fn render(rgba: &mut [u8], width: u32, height: u32, settings: &Settings, ctx: &Context) {
    debug_assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let threads = thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let chunk = ((rgba.len() / 4).div_ceil(threads) * 4).max(4);
    thread::scope(|s| {
        for part in rgba.chunks_mut(chunk) {
            s.spawn(|| process_rgba8(part, settings, &ctx.auto));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_scales_with_the_image() {
        let ctx = Context::new(Correction::NONE, 4608, 3456);
        assert_eq!(ctx.radius_px(10.0), 46.08);
        assert_eq!(Context::new(Correction::NONE, 960, 1280).radius_px(10.0), 12.8);
    }

    #[test]
    fn render_equals_the_per_pixel_pipeline() {
        let auto = Correction { hue_shift: 60.0, ..Correction::NONE };
        let s = Settings { temperature: 20.0, vibrance: 15.0, ..Settings::default() };
        let px: Vec<u8> = (0..97 * 4).map(|i| (i * 37 % 251) as u8).collect();
        let mut a = px.clone();
        render(&mut a, 97, 1, &s, &Context::new(auto, 97, 1));
        let mut b = px;
        process_rgba8(&mut b, &s, &auto);
        assert_eq!(a, b);
    }
}
