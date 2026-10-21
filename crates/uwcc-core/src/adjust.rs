//! Einstellungen des UnderwaterPhotoLab und die Pixel-Pipeline, die Vorschau und Export teilen.
//!
//! Reihenfolge: Farbmatrix (Auto-Analyse + Abweichung, Rot-Obergrenze) → Weißabgleich →
//! Dynamik → Überblendung mit dem Original.

use crate::{Correction, Levels, Transform};

/// Standard für [`Settings::red_limit`]; die Beispielbilder des Originals liegen bei höchstens 3,4.
pub const DEFAULT_RED_LIMIT: f64 = 4.0;
/// Größter Hue-Shift, wie im Experten-Plug-in.
pub const MAX_HUE_SHIFT: f64 = 180.0;
/// ±100 auf den Reglern entspricht einem Faktor e^±1 zwischen Rot und Blau bzw. für Grün.
const WHITE_BALANCE_SCALE: f32 = 0.01;

/// Verschiebung von Schwarz- und Weißpunkt eines Kanals (Skala 0–255 wie [`Levels`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelsDelta {
    pub low: f64,
    pub high: f64,
}

/// Abweichung von der Basis-Korrektur.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Delta {
    pub hue_shift: f64,
    pub red: LevelsDelta,
    pub green: LevelsDelta,
    pub blue: LevelsDelta,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Basis ist die Analyse des jeweiligen Bildes, sonst [`Correction::NONE`].
    pub auto: bool,
    pub delta: Delta,
    /// 0–1: Überblendung mit dem Original.
    pub strength: f32,
    /// −100 (kühler) … +100 (wärmer).
    pub temperature: f32,
    /// −100 (grüner) … +100 (magenta).
    pub tint: f32,
    /// −100 … +100; wirkt auf schwach gesättigte Farben stärker als auf kräftige.
    pub vibrance: f32,
    /// Höchste Spreizung des Rotkanals (`256 / (high − low)`), damit Rauschen und
    /// JPG-Artefakte im fast leeren Rotkanal nicht beliebig verstärkt werden.
    pub red_limit: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto: true,
            delta: Delta::default(),
            strength: 1.0,
            temperature: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            red_limit: DEFAULT_RED_LIMIT,
        }
    }
}

impl Settings {
    /// Die wirksame Korrektur: Basis + Abweichung, in gültigen Grenzen und mit Rot-Obergrenze.
    pub fn resolve(&self, auto: &Correction) -> Correction {
        let base = if self.auto { *auto } else { Correction::NONE };
        let d = &self.delta;
        let mut red = shift(base.red, d.red);
        let min_span = 256.0 / self.red_limit.max(1.0);
        if red.high - red.low < min_span {
            red.high = red.low + min_span;
            if red.high > 256.0 {
                red.high = 256.0;
                red.low = 256.0 - min_span;
            }
        }
        Correction {
            hue_shift: (base.hue_shift + d.hue_shift).clamp(0.0, MAX_HUE_SHIFT),
            red,
            green: shift(base.green, d.green),
            blue: shift(base.blue, d.blue),
        }
    }

    /// Kanalfaktoren des Weißabgleichs, so normiert, dass Grau gleich hell bleibt.
    pub fn white_balance_gains(&self) -> [f32; 3] {
        let a = self.temperature * WHITE_BALANCE_SCALE;
        let c = self.tint * WHITE_BALANCE_SCALE;
        let (r, g, b) = (libm::expf(a), libm::expf(-c), libm::expf(-a));
        let y = luminance(r, g, b);
        [r / y, g / y, b / y]
    }

    /// Pipette: Temperatur und Tint, mit denen das Originalpixel `(r, g, b)` (0–1) nach der
    /// Farbmatrix neutral grau wird. Ergebnis auf ±100 begrenzt.
    pub fn white_balance_from_sample(&self, auto: &Correction, r: f32, g: f32, b: f32) -> (f32, f32) {
        let (r, g, b) = Transform::new(&self.resolve(auto)).apply(r, g, b);
        let (r, g, b) = (r.clamp(1e-4, 1.0), g.clamp(1e-4, 1.0), b.clamp(1e-4, 1.0));
        let a = libm::logf(b / r) / 2.0;
        let c = libm::logf(g / r) - a;
        (
            (a / WHITE_BALANCE_SCALE).clamp(-100.0, 100.0),
            (c / WHITE_BALANCE_SCALE).clamp(-100.0, 100.0),
        )
    }
}

/// Ein Regler der Oberfläche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Strength,
    Temperature,
    Tint,
    Vibrance,
    RedLimit,
    HueShift,
    RedLow,
    RedHigh,
    GreenLow,
    GreenHigh,
    BlueLow,
    BlueHigh,
}

impl Field {
    pub const ALL: [Field; 12] = [
        Field::Strength,
        Field::Temperature,
        Field::Tint,
        Field::Vibrance,
        Field::RedLimit,
        Field::HueShift,
        Field::RedLow,
        Field::RedHigh,
        Field::GreenLow,
        Field::GreenHigh,
        Field::BlueLow,
        Field::BlueHigh,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Field::Strength => "strength",
            Field::Temperature => "temperature",
            Field::Tint => "tint",
            Field::Vibrance => "vibrance",
            Field::RedLimit => "red-limit",
            Field::HueShift => "hue",
            Field::RedLow => "red-low",
            Field::RedHigh => "red-high",
            Field::GreenLow => "green-low",
            Field::GreenHigh => "green-high",
            Field::BlueLow => "blue-low",
            Field::BlueHigh => "blue-high",
        }
    }

    pub fn from_name(name: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.name() == name)
    }
}

impl Settings {
    /// Der Wert, den der Regler zeigt: Stärke in %, Hue-Shift und Levels so, wie sie wirken.
    pub fn effective(&self, field: Field, auto: &Correction) -> f64 {
        let c = self.resolve(auto);
        match field {
            Field::Strength => self.strength as f64 * 100.0,
            Field::Temperature => self.temperature as f64,
            Field::Tint => self.tint as f64,
            Field::Vibrance => self.vibrance as f64,
            Field::RedLimit => self.red_limit,
            Field::HueShift => c.hue_shift,
            Field::RedLow => c.red.low,
            Field::RedHigh => c.red.high,
            Field::GreenLow => c.green.low,
            Field::GreenHigh => c.green.high,
            Field::BlueLow => c.blue.low,
            Field::BlueHigh => c.blue.high,
        }
    }

    /// Übernimmt einen Reglerwert. Hue-Shift und Levels verschieben nur die Abweichung von der
    /// Basis, und nur das eine Feld – so bleibt z. B. eine durch `red_limit` angehobene
    /// Rot-Spreizung nicht in den Levels stehen, wenn die Grenze später gelockert wird.
    pub fn set_effective(&mut self, field: Field, value: f64, auto: &Correction) {
        let base = if self.auto { *auto } else { Correction::NONE };
        let d = &mut self.delta;
        match field {
            Field::Strength => self.strength = (value / 100.0) as f32,
            Field::Temperature => self.temperature = value as f32,
            Field::Tint => self.tint = value as f32,
            Field::Vibrance => self.vibrance = value as f32,
            Field::RedLimit => self.red_limit = value,
            Field::HueShift => d.hue_shift = value - base.hue_shift,
            Field::RedLow => d.red.low = value - base.red.low,
            Field::RedHigh => d.red.high = value - base.red.high,
            Field::GreenLow => d.green.low = value - base.green.low,
            Field::GreenHigh => d.green.high = value - base.green.high,
            Field::BlueLow => d.blue.low = value - base.blue.low,
            Field::BlueHigh => d.blue.high = value - base.blue.high,
        }
    }
}

fn shift(l: Levels, d: LevelsDelta) -> Levels {
    let low = (l.low + d.low).clamp(0.0, 255.0);
    let high = (l.high + d.high).clamp(low + 1.0, 256.0);
    Levels { low, high }
}

fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn vibrance(r: f32, g: f32, b: f32, amount: f32) -> (f32, f32, f32) {
    let sat = r.max(g).max(b) - r.min(g).min(b);
    let k = 1.0 + amount * (1.0 - sat);
    let y = luminance(r, g, b);
    (y + (r - y) * k, y + (g - y) * k, y + (b - y) * k)
}

/// Die Pipeline für 8-Bit-RGBA-Pixel; `auto` ist die Analyse des Bildes ([`crate::analyze`]).
/// Alpha bleibt unverändert.
pub fn process_rgba8(rgba: &mut [u8], settings: &Settings, auto: &Correction) {
    let t = Transform::new(&settings.resolve(auto));
    let [wr, wg, wb] = settings.white_balance_gains();
    let strength = settings.strength.clamp(0.0, 1.0);
    let vib = settings.vibrance.clamp(-100.0, 100.0) / 100.0;
    let unit = |v: f32| v.clamp(0.0, 1.0);
    let to_u8 = |v: f32| (unit(v) * 255.0 + 0.5) as u8;

    for p in rgba.chunks_exact_mut(4) {
        let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (mr, mg, mb) = t.apply(r, g, b);
        let (mut nr, mut ng, mut nb) = (unit(unit(mr) * wr), unit(unit(mg) * wg), unit(unit(mb) * wb));
        if vib != 0.0 {
            let (vr, vg, vb) = vibrance(nr, ng, nb, vib);
            (nr, ng, nb) = (unit(vr), unit(vg), unit(vb));
        }
        p[0] = to_u8(r + (nr - r) * strength);
        p[1] = to_u8(g + (ng - g) * strength);
        p[2] = to_u8(b + (nb - b) * strength);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTO: Correction = Correction {
        hue_shift: 61.0,
        red: Levels { low: 0.0, high: 205.0 },
        green: Levels { low: 4.0, high: 202.0 },
        blue: Levels { low: 27.0, high: 255.0 },
    };

    fn sample() -> [u8; 24] {
        [
            20, 90, 140, 255, //
            200, 210, 220, 128, //
            5, 60, 70, 255, //
            128, 128, 128, 0, //
            0, 0, 0, 255, //
            255, 255, 255, 255,
        ]
    }

    #[test]
    fn defaults_equal_plain_matrix() {
        let mut got = sample();
        process_rgba8(&mut got, &Settings::default(), &AUTO);

        let t = Transform::new(&AUTO);
        let mut want = sample();
        for p in want.chunks_exact_mut(4) {
            let f = |v: u8| v as f32 / 255.0;
            let (r, g, b) = t.apply(f(p[0]), f(p[1]), f(p[2]));
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            (p[0], p[1], p[2]) = (q(r), q(g), q(b));
        }
        assert_eq!(got, want);
    }

    #[test]
    fn zero_strength_keeps_original_and_alpha_is_untouched() {
        let s = Settings { strength: 0.0, temperature: 40.0, vibrance: 50.0, ..Settings::default() };
        let mut px = sample();
        process_rgba8(&mut px, &s, &AUTO);
        assert_eq!(px, sample());

        let mut px = sample();
        process_rgba8(&mut px, &Settings::default(), &AUTO);
        let alpha: [u8; 6] = core::array::from_fn(|i| px[i * 4 + 3]);
        assert_eq!(alpha, [255, 128, 255, 0, 255, 255]);
    }

    #[test]
    fn resolve_adds_delta_and_respects_limits() {
        let mut s = Settings::default();
        s.delta.hue_shift = -10.0;
        s.delta.green = LevelsDelta { low: -10.0, high: 100.0 };
        let c = s.resolve(&AUTO);
        assert_eq!(c.hue_shift, 51.0);
        assert_eq!(c.green, Levels { low: 0.0, high: 256.0 });

        s.auto = false;
        s.delta = Delta { hue_shift: 30.0, ..Delta::default() };
        assert_eq!(s.resolve(&AUTO), Correction { hue_shift: 30.0, ..Correction::NONE });

        s.delta.blue = LevelsDelta { low: 300.0, high: -500.0 };
        let blue = s.resolve(&AUTO).blue;
        assert_eq!(blue, Levels { low: 255.0, high: 256.0 });
    }

    #[test]
    fn sliders_show_and_set_effective_values() {
        let mut s = Settings::default();
        assert_eq!(s.effective(Field::HueShift, &AUTO), 61.0);
        assert_eq!(s.effective(Field::Strength, &AUTO), 100.0);

        s.set_effective(Field::HueShift, 70.0, &AUTO);
        assert_eq!(s.delta.hue_shift, 9.0);
        assert_eq!(s.effective(Field::HueShift, &AUTO), 70.0);

        s.set_effective(Field::Strength, 40.0, &AUTO);
        assert!((s.strength - 0.4).abs() < 1e-6);

        s.auto = false;
        assert_eq!(s.effective(Field::HueShift, &AUTO), 9.0, "ohne Auto bleibt nur die Abweichung");

        for f in Field::ALL {
            assert_eq!(Field::from_name(f.name()), Some(f));
        }
    }

    #[test]
    fn red_limit_is_not_baked_into_the_levels() {
        let narrow = Correction { red: Levels { low: 100.0, high: 120.0 }, ..AUTO };
        let mut s = Settings::default();
        assert_eq!(s.effective(Field::RedHigh, &narrow), 164.0);

        s.set_effective(Field::HueShift, 50.0, &narrow);
        assert_eq!(s.delta.red, LevelsDelta::default());

        s.set_effective(Field::RedLimit, 8.0, &narrow);
        assert_eq!(s.effective(Field::RedHigh, &narrow), 132.0);
    }

    #[test]
    fn red_limit_caps_the_red_spread() {
        let narrow = Correction { red: Levels { low: 100.0, high: 120.0 }, ..AUTO };
        let red = Settings::default().resolve(&narrow).red;
        assert_eq!(red.high - red.low, 64.0);
        assert_eq!(red.low, 100.0);

        let top = Correction { red: Levels { low: 250.0, high: 255.0 }, ..AUTO };
        assert_eq!(Settings::default().resolve(&top).red, Levels { low: 192.0, high: 256.0 });

        assert_eq!(Settings::default().resolve(&AUTO), AUTO, "205 / 256 liegt unter der Grenze");
    }

    #[test]
    fn neutral_white_balance_keeps_gray() {
        let s = Settings::default();
        let [r, g, b] = s.white_balance_gains();
        assert!((r - 1.0).abs() < 1e-6 && (g - 1.0).abs() < 1e-6 && (b - 1.0).abs() < 1e-6);

        let warm = Settings { temperature: 50.0, tint: 20.0, ..s }.white_balance_gains();
        assert!((luminance(warm[0], warm[1], warm[2]) - 1.0).abs() < 1e-5);
        assert!(warm[0] > 1.0 && warm[2] < warm[0]);
    }

    #[test]
    fn pipette_makes_the_sample_gray() {
        let plain = Settings { auto: false, ..Settings::default() };
        let (r, g, b) = (90u8, 140u8, 160u8);
        let f = |v: u8| v as f32 / 255.0;
        let (temperature, tint) = plain.white_balance_from_sample(&AUTO, f(r), f(g), f(b));
        assert!(temperature > 0.0, "bläulich → wärmer");

        let s = Settings { temperature, tint, ..plain };
        let mut px = [r, g, b, 255];
        process_rgba8(&mut px, &s, &AUTO);
        let spread = px[..3].iter().max().unwrap() - px[..3].iter().min().unwrap();
        assert!(spread <= 1, "nicht neutral: {px:?}");
    }

    #[test]
    fn vibrance_boosts_weak_colours_more_and_leaves_gray() {
        assert_eq!(vibrance(0.5, 0.5, 0.5, 1.0), (0.5, 0.5, 0.5));

        let weak = (0.5, 0.45, 0.4);
        let strong = (0.9, 0.2, 0.1);
        let gain = |(r, g, b): (f32, f32, f32)| {
            let (vr, _, vb) = vibrance(r, g, b, 0.5);
            (vr - vb) / (r - b)
        };
        assert!(gain(weak) > gain(strong));
        assert!(gain(strong) >= 1.0);
    }
}
