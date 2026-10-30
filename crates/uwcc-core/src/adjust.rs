//! Einstellungen des UnderwaterPhotoLab und die Pixel-Pipeline, die Vorschau und Export teilen.
//!
//! Reihenfolge je Pixel ([`PixelOps`]): Farbmatrix (Auto-Analyse + Abweichung, Rot-Obergrenze) →
//! Weißabgleich → Dynamik → Belichtung → Kontrast → Überblendung mit dem Original. Filter mit
//! Nachbarschaft (Dehaze, Lichter/Tiefen) rechnet `uwcc-filters` zwischen diesen Schritten.

use crate::{Correction, Levels, Transform};

/// Standard für [`Settings::red_limit`]; die Beispielbilder des Originals liegen bei höchstens 3,4.
pub const DEFAULT_RED_LIMIT: f64 = 4.0;
/// Größter Hue-Shift, wie im Experten-Plug-in.
pub const MAX_HUE_SHIFT: f64 = 180.0;
/// ±100 auf den Reglern ergibt Kanalfaktoren e^±0,25 (Rot zu Blau also bis ×1,65). Größere Stiche
/// nimmt schon die Farbmatrix heraus; mehr Spielraum machte die Regler zu empfindlich.
const WHITE_BALANCE_SCALE: f32 = 0.0025;
/// Belichtung ±100 = ±2 Blenden.
const EXPOSURE_EV: f32 = 2.0;
/// Kontrast ±100: so viel der S-Kurve `3v² − 2v³` wird beigemischt bzw. abgezogen.
const CONTRAST_AMOUNT: f32 = 0.6;
/// Gamma der Pixelwerte; Belichtung wirkt so wie in linearem Licht.
const GAMMA: f32 = 2.2;

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

/// Ton und Dehaze; −100 … +100 bzw. 0 … 100, 0 = neutral.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tone {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    /// 0 … 100: Schleier aus Schwebeteilchen entfernen.
    pub dehaze: f32,
}

/// Standard für [`Detail::sharpen_radius`].
pub const DEFAULT_SHARPEN_RADIUS: f32 = 1.0;

/// Details; 0 = aus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detail {
    /// Lokaler Kontrast, −100 … +100.
    pub clarity: f32,
    /// Schärfen, 0 … 100.
    pub sharpen: f32,
    /// Radius des Schärfens in Pixeln des ganzen Bildes in voller Auflösung, 0,5 … 3.
    pub sharpen_radius: f32,
    /// 0 … 100: schärft nur Stellen mit so viel Struktur; glatte Flächen (Wasser) bleiben ruhig.
    pub sharpen_mask: f32,
    /// Farbrauschen reduzieren, 0 … 100.
    pub noise_color: f32,
    /// Helligkeitsrauschen reduzieren, 0 … 100.
    pub noise_luma: f32,
}

impl Default for Detail {
    fn default() -> Self {
        Self {
            clarity: 0.0,
            sharpen: 0.0,
            sharpen_radius: DEFAULT_SHARPEN_RADIUS,
            sharpen_mask: 0.0,
            noise_color: 0.0,
            noise_luma: 0.0,
        }
    }
}

/// Ein Bereich des Farbmischers; je −100 … +100, 0 = unverändert.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColorRange {
    /// Farbton verschieben (±100 = ±30°).
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

/// Effekte: Vignette und vereinfachter Farbmischer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Effects {
    /// −100 (Ecken dunkler) … +100 (Ecken heller, z. B. gegen Blitz-Abfall).
    pub vignette: f32,
    /// Wasserblau (um 215°).
    pub water: ColorRange,
    /// Cyan/Türkis (um 180°).
    pub cyan: ColorRange,
    /// Orange/Gelb (um 35°): Fische, Korallen, Haut.
    pub warm: ColorRange,
}

/// Mitte und halbe Breite (Grad) der Farbmischer-Bereiche: Wasserblau, Cyan, Orange/Gelb.
const MIXER_RANGES: [(f32, f32); 3] = [(215.0, 35.0), (180.0, 25.0), (35.0, 35.0)];
/// Farbmischer: ±100 Farbton = ±30°, ±100 Helligkeit = ±0,7 Blenden.
const MIXER_HUE: f32 = 30.0;
const MIXER_STOPS: f32 = 0.7;

/// Ausschnitt in normalisierten Koordinaten (0–1) des gedrehten und geradegerichteten Bildes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Geometrie; wirkt vor allen Farbfiltern.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    /// Drehung in Vierteln im Uhrzeigersinn (0–3).
    pub quarter_turns: u8,
    pub flip_horizontal: bool,
    /// Geraderichten in Grad, −45 … +45.
    pub straighten: f32,
    pub crop: Option<Crop>,
}

/// Gruppen von Einstellungen, die sich einzeln auf andere Bilder übertragen lassen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Groups {
    /// Auto, Abweichung, Stärke, Weißabgleich, Dynamik, Rot-Obergrenze, Farbmischer.
    pub color: bool,
    /// Ton und Vignette.
    pub tone: bool,
    pub detail: bool,
    /// Drehen, Spiegeln, Geraderichten, Zuschnitt; gehört meist zum einzelnen Bild.
    pub geometry: bool,
}

impl Default for Groups {
    fn default() -> Self {
        Self { color: true, tone: true, detail: true, geometry: false }
    }
}

impl Geometry {
    /// Um 90° im (`clockwise`) bzw. gegen den Uhrzeigersinn; der Zuschnitt dreht mit.
    pub fn rotate(&mut self, clockwise: bool) {
        self.quarter_turns = (self.quarter_turns + if clockwise { 1 } else { 3 }) % 4;
        self.crop = self.crop.map(|c| {
            if clockwise {
                Crop { x: 1.0 - c.y - c.height, y: c.x, width: c.height, height: c.width }
            } else {
                Crop { x: c.y, y: 1.0 - c.x - c.width, width: c.height, height: c.width }
            }
        });
    }

    /// Waagerecht spiegeln, so wie das Bild gerade angezeigt wird. Gespiegelt wird vor dem Drehen;
    /// bei Hochkant-Drehung ist das angezeigte Waagerecht daher Spiegeln plus 180°. Zuschnitt und
    /// Geraderichten spiegeln mit.
    pub fn flip(&mut self) {
        self.flip_horizontal = !self.flip_horizontal;
        if self.quarter_turns % 2 == 1 {
            self.quarter_turns = (self.quarter_turns + 2) % 4;
        }
        self.straighten = -self.straighten;
        self.crop = self.crop.map(|c| Crop { x: 1.0 - c.x - c.width, ..c });
    }
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
    pub tone: Tone,
    pub detail: Detail,
    pub geometry: Geometry,
    pub effects: Effects,
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
            tone: Tone::default(),
            detail: Detail::default(),
            geometry: Geometry::default(),
            effects: Effects::default(),
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
    Exposure,
    Contrast,
    Highlights,
    Shadows,
    Dehaze,
    Clarity,
    Sharpen,
    SharpenRadius,
    SharpenMask,
    NoiseColor,
    NoiseLuma,
    Straighten,
    Vignette,
    WaterHue,
    WaterSaturation,
    WaterLuminance,
    CyanHue,
    CyanSaturation,
    CyanLuminance,
    WarmHue,
    WarmSaturation,
    WarmLuminance,
}

/// Wert eines Effekt-Feldes; `None` für alle anderen Felder.
fn effects_value(e: &Effects, field: Field) -> Option<f32> {
    let mut copy = *e;
    effects_slot(&mut copy, field).map(|v| *v)
}

fn effects_slot(e: &mut Effects, field: Field) -> Option<&mut f32> {
    Some(match field {
        Field::Vignette => &mut e.vignette,
        Field::WaterHue => &mut e.water.hue,
        Field::WaterSaturation => &mut e.water.saturation,
        Field::WaterLuminance => &mut e.water.luminance,
        Field::CyanHue => &mut e.cyan.hue,
        Field::CyanSaturation => &mut e.cyan.saturation,
        Field::CyanLuminance => &mut e.cyan.luminance,
        Field::WarmHue => &mut e.warm.hue,
        Field::WarmSaturation => &mut e.warm.saturation,
        Field::WarmLuminance => &mut e.warm.luminance,
        _ => return None,
    })
}

impl Field {
    pub const ALL: [Field; 34] = [
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
        Field::Exposure,
        Field::Contrast,
        Field::Highlights,
        Field::Shadows,
        Field::Dehaze,
        Field::Clarity,
        Field::Sharpen,
        Field::SharpenRadius,
        Field::SharpenMask,
        Field::NoiseColor,
        Field::NoiseLuma,
        Field::Straighten,
        Field::Vignette,
        Field::WaterHue,
        Field::WaterSaturation,
        Field::WaterLuminance,
        Field::CyanHue,
        Field::CyanSaturation,
        Field::CyanLuminance,
        Field::WarmHue,
        Field::WarmSaturation,
        Field::WarmLuminance,
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
            Field::Exposure => "exposure",
            Field::Contrast => "contrast",
            Field::Highlights => "highlights",
            Field::Shadows => "shadows",
            Field::Dehaze => "dehaze",
            Field::Clarity => "clarity",
            Field::Sharpen => "sharpen",
            Field::SharpenRadius => "sharpen-radius",
            Field::SharpenMask => "sharpen-mask",
            Field::NoiseColor => "noise-color",
            Field::NoiseLuma => "noise-luma",
            Field::Straighten => "straighten",
            Field::Vignette => "vignette",
            Field::WaterHue => "water-hue",
            Field::WaterSaturation => "water-saturation",
            Field::WaterLuminance => "water-luminance",
            Field::CyanHue => "cyan-hue",
            Field::CyanSaturation => "cyan-saturation",
            Field::CyanLuminance => "cyan-luminance",
            Field::WarmHue => "warm-hue",
            Field::WarmSaturation => "warm-saturation",
            Field::WarmLuminance => "warm-luminance",
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
            Field::Exposure => self.tone.exposure as f64,
            Field::Contrast => self.tone.contrast as f64,
            Field::Highlights => self.tone.highlights as f64,
            Field::Shadows => self.tone.shadows as f64,
            Field::Dehaze => self.tone.dehaze as f64,
            Field::Clarity => self.detail.clarity as f64,
            Field::Sharpen => self.detail.sharpen as f64,
            Field::SharpenRadius => self.detail.sharpen_radius as f64,
            Field::SharpenMask => self.detail.sharpen_mask as f64,
            Field::NoiseColor => self.detail.noise_color as f64,
            Field::NoiseLuma => self.detail.noise_luma as f64,
            Field::Straighten => self.geometry.straighten as f64,
            _ => effects_value(&self.effects, field).unwrap_or(0.0) as f64,
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
            Field::Exposure => self.tone.exposure = value as f32,
            Field::Contrast => self.tone.contrast = value as f32,
            Field::Highlights => self.tone.highlights = value as f32,
            Field::Shadows => self.tone.shadows = value as f32,
            Field::Dehaze => self.tone.dehaze = value as f32,
            Field::Clarity => self.detail.clarity = value as f32,
            Field::Sharpen => self.detail.sharpen = value as f32,
            Field::SharpenRadius => self.detail.sharpen_radius = value as f32,
            Field::SharpenMask => self.detail.sharpen_mask = value as f32,
            Field::NoiseColor => self.detail.noise_color = value as f32,
            Field::NoiseLuma => self.detail.noise_luma = value as f32,
            Field::Straighten => self.geometry.straighten = (value as f32).clamp(-45.0, 45.0),
            _ => {
                if let Some(v) = effects_slot(&mut self.effects, field) {
                    *v = (value as f32).clamp(-100.0, 100.0);
                }
            }
        }
    }
}

impl Settings {
    /// `self` mit den in `groups` gewählten Gruppen aus `source`; alles andere bleibt.
    pub fn with_groups(&self, source: &Settings, groups: Groups) -> Settings {
        let mut s = *self;
        if groups.color {
            s.auto = source.auto;
            s.delta = source.delta;
            s.strength = source.strength;
            s.temperature = source.temperature;
            s.tint = source.tint;
            s.vibrance = source.vibrance;
            s.red_limit = source.red_limit;
            s.effects = Effects { vignette: s.effects.vignette, ..source.effects };
        }
        if groups.tone {
            s.tone = source.tone;
            s.effects.vignette = source.effects.vignette;
        }
        if groups.detail {
            s.detail = source.detail;
        }
        if groups.geometry {
            s.geometry = source.geometry;
        }
        s
    }

    /// Dieselben Einstellungen als feste Werte: Auto aus und die Abweichung so, dass jedes Bild
    /// genau die Korrektur bekommt, die `self` mit `auto` ergibt („gleiche Werte übernehmen“).
    pub fn fixed(&self, auto: &Correction) -> Settings {
        let c = self.resolve(auto);
        let n = Correction::NONE;
        let levels = |l: Levels, base: Levels| LevelsDelta { low: l.low - base.low, high: l.high - base.high };
        Settings {
            auto: false,
            delta: Delta {
                hue_shift: c.hue_shift - n.hue_shift,
                red: levels(c.red, n.red),
                green: levels(c.green, n.green),
                blue: levels(c.blue, n.blue),
            },
            ..*self
        }
    }

    /// Setzt nur `field` auf den Wert, den es nach „Zurücksetzen“ hätte (Standard bzw. der
    /// Auto-Wert), ohne die anderen Felder zu ändern. Ist Auto aus, wird die Abweichung so
    /// gewählt, dass der Auto-Wert trotzdem erreicht wird.
    pub fn reset_field(&mut self, field: Field, auto: &Correction) {
        let defaults = Settings::default();
        let target = |auto_value: f64, none_value: f64| if self.auto { 0.0 } else { auto_value - none_value };
        let none = Correction::NONE;
        let value_of = |c: &Correction| match field {
            Field::HueShift => c.hue_shift,
            Field::RedLow => c.red.low,
            Field::RedHigh => c.red.high,
            Field::GreenLow => c.green.low,
            Field::GreenHigh => c.green.high,
            Field::BlueLow => c.blue.low,
            Field::BlueHigh => c.blue.high,
            _ => 0.0,
        };
        let delta = target(value_of(auto), value_of(&none));
        let d = &mut self.delta;
        match field {
            Field::Strength => self.strength = defaults.strength,
            Field::Temperature => self.temperature = defaults.temperature,
            Field::Tint => self.tint = defaults.tint,
            Field::Vibrance => self.vibrance = defaults.vibrance,
            Field::RedLimit => self.red_limit = defaults.red_limit,
            Field::HueShift => d.hue_shift = delta,
            Field::RedLow => d.red.low = delta,
            Field::RedHigh => d.red.high = delta,
            Field::GreenLow => d.green.low = delta,
            Field::GreenHigh => d.green.high = delta,
            Field::BlueLow => d.blue.low = delta,
            Field::BlueHigh => d.blue.high = delta,
            Field::Exposure => self.tone.exposure = defaults.tone.exposure,
            Field::Contrast => self.tone.contrast = defaults.tone.contrast,
            Field::Highlights => self.tone.highlights = defaults.tone.highlights,
            Field::Shadows => self.tone.shadows = defaults.tone.shadows,
            Field::Dehaze => self.tone.dehaze = defaults.tone.dehaze,
            Field::Clarity => self.detail.clarity = defaults.detail.clarity,
            Field::Sharpen => self.detail.sharpen = defaults.detail.sharpen,
            Field::SharpenRadius => self.detail.sharpen_radius = defaults.detail.sharpen_radius,
            Field::SharpenMask => self.detail.sharpen_mask = defaults.detail.sharpen_mask,
            Field::NoiseColor => self.detail.noise_color = defaults.detail.noise_color,
            Field::NoiseLuma => self.detail.noise_luma = defaults.detail.noise_luma,
            Field::Straighten => self.geometry.straighten = defaults.geometry.straighten,
            _ => {
                if let Some(v) = effects_slot(&mut self.effects, field) {
                    *v = 0.0;
                }
            }
        }
    }
}

fn shift(l: Levels, d: LevelsDelta) -> Levels {
    let low = (l.low + d.low).clamp(0.0, 255.0);
    let high = (l.high + d.high).clamp(low + 1.0, 256.0);
    Levels { low, high }
}

/// Helligkeit nach Rec. 709 (auf den gamma-kodierten Werten).
pub fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn vibrance(r: f32, g: f32, b: f32, amount: f32) -> (f32, f32, f32) {
    let sat = r.max(g).max(b) - r.min(g).min(b);
    let k = 1.0 + amount * (1.0 - sat);
    let y = luminance(r, g, b);
    (y + (r - y) * k, y + (g - y) * k, y + (b - y) * k)
}

fn unit(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// S-Kurve um die Mitte; `a` −1 … +1, monoton für alle `a` in diesem Bereich.
fn contrast(v: f32, a: f32) -> f32 {
    v + a * (v * v * (3.0 - 2.0 * v) - v)
}

/// Die Schritte der Pipeline, die je Pixel wirken (Werte 0–1, gamma-kodiert).
#[derive(Clone, Copy, Debug)]
pub struct PixelOps {
    t: Transform,
    wb: [f32; 3],
    vibrance: f32,
    exposure: f32,
    contrast: f32,
    strength: f32,
    /// Farbmischer je Bereich: Farbton (Grad), Sättigung (−1 … 1), Helligkeit (Blenden);
    /// `None`, wenn alles 0 ist.
    mixer: Option<[(f32, f32, f32); 3]>,
}

/// RGB (0–1) → Farbton (Grad), Sättigung, Wert.
fn to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    if d <= 1e-6 || max <= 1e-6 {
        return (0.0, 0.0, max);
    }
    let h = if max == r {
        60.0 * libm::fmodf((g - b) / d + 6.0, 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, d / max, max)
}

fn from_hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = libm::fmodf(libm::fmodf(h, 360.0) + 360.0, 360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - libm::fabsf(libm::fmodf(h, 2.0) - 1.0));
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (r + m, g + m, b + m)
}

/// Farbmischer: jeder Bereich wirkt mit weichem Übergang (Kosinus) um seine Mitte und auf kräftige
/// Farben stärker als auf fast graue.
fn mix(r: f32, g: f32, b: f32, mixer: &[(f32, f32, f32); 3]) -> (f32, f32, f32) {
    let (h, s, v) = to_hsv(r, g, b);
    let colourful = ((s - 0.05) / 0.2).clamp(0.0, 1.0);
    if colourful == 0.0 {
        return (r, g, b);
    }
    let (mut dh, mut ds, mut dl) = (0.0, 0.0, 0.0);
    for ((centre, half), (hue, sat, stops)) in MIXER_RANGES.iter().zip(mixer) {
        let dist = libm::fabsf(libm::fmodf(h - centre + 540.0, 360.0) - 180.0);
        if dist < *half {
            let w = 0.5 * (1.0 + libm::cosf(core::f32::consts::PI * dist / half)) * colourful;
            dh += w * hue;
            ds += w * sat;
            dl += w * stops;
        }
    }
    if dh == 0.0 && ds == 0.0 && dl == 0.0 {
        return (r, g, b);
    }
    from_hsv(h + dh, (s * (1.0 + ds)).clamp(0.0, 1.0), v * libm::exp2f(dl))
}

impl PixelOps {
    pub fn new(settings: &Settings, auto: &Correction) -> Self {
        let tone = &settings.tone;
        let ev = tone.exposure.clamp(-100.0, 100.0) / 100.0 * EXPOSURE_EV;
        Self {
            t: Transform::new(&settings.resolve(auto)),
            wb: settings.white_balance_gains(),
            vibrance: settings.vibrance.clamp(-100.0, 100.0) / 100.0,
            // In linearem Licht ×2^ev; bei reinem Gamma ist das auf den kodierten Werten ×2^(ev/γ).
            exposure: libm::exp2f(ev / GAMMA),
            contrast: tone.contrast.clamp(-100.0, 100.0) / 100.0 * CONTRAST_AMOUNT,
            strength: settings.strength.clamp(0.0, 1.0),
            mixer: {
                let e = &settings.effects;
                let range = |c: &ColorRange| {
                    let u = |v: f32| v.clamp(-100.0, 100.0) / 100.0;
                    (u(c.hue) * MIXER_HUE, u(c.saturation), u(c.luminance) * MIXER_STOPS)
                };
                let m = [range(&e.water), range(&e.cyan), range(&e.warm)];
                m.iter().any(|&(h, s, l)| h != 0.0 || s != 0.0 || l != 0.0).then_some(m)
            },
        }
    }

    /// Farbmatrix; Ergebnis 0–1.
    pub fn matrix(&self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let (r, g, b) = self.t.apply(r, g, b);
        [unit(r), unit(g), unit(b)]
    }

    /// Weißabgleich, Dynamik und Belichtung. Das Ergebnis kann über 1 liegen; Lichter holen das
    /// zurück, sonst begrenzt [`Self::finish`].
    pub fn color(&self, [r, g, b]: [f32; 3]) -> [f32; 3] {
        let [wr, wg, wb] = self.wb;
        let (mut r, mut g, mut b) = (unit(unit(r) * wr), unit(unit(g) * wg), unit(unit(b) * wb));
        if self.vibrance != 0.0 {
            let (vr, vg, vb) = vibrance(r, g, b, self.vibrance);
            (r, g, b) = (unit(vr), unit(vg), unit(vb));
        }
        if let Some(m) = &self.mixer {
            let (mr, mg, mb) = mix(r, g, b, m);
            (r, g, b) = (unit(mr), unit(mg), unit(mb));
        }
        [r * self.exposure, g * self.exposure, b * self.exposure]
    }

    /// Begrenzen, Kontrast und Überblendung mit dem Original `orig`; Ergebnis 8 Bit.
    pub fn finish(&self, rgb: [f32; 3], orig: [f32; 3]) -> [u8; 3] {
        core::array::from_fn(|i| {
            let v = contrast(unit(rgb[i]), self.contrast);
            (unit(orig[i] + (v - orig[i]) * self.strength) * 255.0 + 0.5) as u8
        })
    }
}

/// 8-Bit-Wert → 0–1.
pub fn unit_rgb(p: &[u8]) -> [f32; 3] {
    [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]
}

/// Die Pipeline je Pixel für 8-Bit-RGBA; `auto` ist die Analyse des Bildes ([`crate::analyze`]).
/// Alpha bleibt unverändert. Dehaze und Lichter/Tiefen fehlen hier, sie brauchen Nachbarpixel.
pub fn process_rgba8(rgba: &mut [u8], settings: &Settings, auto: &Correction) {
    let ops = PixelOps::new(settings, auto);
    for p in rgba.chunks_exact_mut(4) {
        let orig = unit_rgb(p);
        let out = ops.finish(ops.color(ops.matrix(orig)), orig);
        p[..3].copy_from_slice(&out);
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
    fn fixed_settings_ignore_the_other_images_analysis() {
        let other = Correction {
            hue_shift: 100.0,
            red: Levels { low: 30.0, high: 120.0 },
            green: Levels { low: 0.0, high: 256.0 },
            blue: Levels { low: 50.0, high: 230.0 },
        };
        let mut s = Settings { temperature: 12.0, vibrance: 30.0, ..Settings::default() };
        s.set_effective(Field::HueShift, 75.0, &AUTO);

        let fixed = s.fixed(&AUTO);
        assert!(!fixed.auto);
        assert_eq!(fixed.resolve(&other), s.resolve(&AUTO));
        assert_eq!((fixed.temperature, fixed.vibrance), (12.0, 30.0));

        let (mut a, mut b) = (sample(), sample());
        process_rgba8(&mut a, &s, &AUTO);
        process_rgba8(&mut b, &fixed, &other);
        assert_eq!(a, b, "gleiche Wirkung auf jedem Bild");

        assert_ne!(s.resolve(&other), s.resolve(&AUTO), "relativ übernommen hängt vom Bild ab");
    }

    #[test]
    fn reset_field_touches_only_that_field() {
        let mut s = Settings { temperature: 30.0, tint: -20.0, strength: 0.5, ..Settings::default() };
        s.set_effective(Field::HueShift, 90.0, &AUTO);
        s.set_effective(Field::BlueLow, 50.0, &AUTO);
        let before = s;

        s.reset_field(Field::Temperature, &AUTO);
        assert_eq!(s.temperature, 0.0);
        assert_eq!(Settings { temperature: before.temperature, ..s }, before, "nur Temperatur geändert");

        s.reset_field(Field::HueShift, &AUTO);
        assert_eq!(s.effective(Field::HueShift, &AUTO), 61.0, "zurück auf den Auto-Wert");
        assert_eq!(s.effective(Field::BlueLow, &AUTO), 50.0);
        assert_eq!(s.tint, -20.0);

        s.auto = false;
        s.reset_field(Field::HueShift, &AUTO);
        assert_eq!(s.effective(Field::HueShift, &AUTO), 61.0, "auch ohne Auto der Wert von „Zurücksetzen“");

        for f in Field::ALL {
            let mut one = before;
            one.reset_field(f, &AUTO);
            assert_eq!(one.effective(f, &AUTO), Settings::default().effective(f, &AUTO), "{f:?}");
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
        let (r, g, b) = (120u8, 140u8, 150u8);
        let f = |v: u8| v as f32 / 255.0;
        let (temperature, tint) = plain.white_balance_from_sample(&AUTO, f(r), f(g), f(b));
        assert!(temperature > 0.0 && temperature < 100.0, "bläulich → wärmer, im Bereich: {temperature}");

        let s = Settings { temperature, tint, ..plain };
        let mut px = [r, g, b, 255];
        process_rgba8(&mut px, &s, &AUTO);
        let spread = px[..3].iter().max().unwrap() - px[..3].iter().min().unwrap();
        assert!(spread <= 1, "nicht neutral: {px:?}");

        let (strong, _) = plain.white_balance_from_sample(&AUTO, f(60), f(140), f(170));
        assert_eq!(strong, 100.0, "starke Stiche enden am Reglerende");
    }

    #[test]
    fn exposure_acts_like_linear_light() {
        let plain = Settings { auto: false, ..Settings::default() };
        let one_stop = Settings { tone: Tone { exposure: 50.0, ..Tone::default() }, ..plain };
        let ops = PixelOps::new(&one_stop, &AUTO);
        let [r, _, _] = ops.color([0.4, 0.4, 0.4]);
        let linear = |v: f32| libm::powf(v, GAMMA);
        assert!((linear(r) / linear(0.4) - 2.0).abs() < 1e-3, "+50 = +1 Blende");

        let mut px = [200, 200, 200, 255];
        process_rgba8(&mut px, &one_stop, &AUTO);
        assert_eq!(&px[..3], &[255, 255, 255], "Überbelichtung wird begrenzt");
    }

    #[test]
    fn contrast_is_monotonic_and_keeps_black_mid_and_white() {
        for a in [-0.6, 0.0, 0.6] {
            assert_eq!(contrast(0.0, a), 0.0);
            assert!((contrast(0.5, a) - 0.5).abs() < 1e-6);
            assert_eq!(contrast(1.0, a), 1.0);
            let mut last = -1.0;
            for i in 0..=100 {
                let v = contrast(i as f32 / 100.0, a);
                assert!(v >= last, "monoton bei a = {a}");
                last = v;
            }
        }
        assert!(contrast(0.25, 0.6) < 0.25 && contrast(0.75, 0.6) > 0.75, "mehr Kontrast spreizt");
        assert!(contrast(0.25, -0.6) > 0.25, "weniger Kontrast staucht");
    }

    #[test]
    fn neutral_tone_changes_nothing() {
        let ops = PixelOps::new(&Settings::default(), &AUTO);
        let rgb = [0.3, 0.6, 0.9];
        assert_eq!(ops.color(rgb), rgb);
        assert_eq!(ops.finish(rgb, rgb), [77, 153, 230]);
        for f in [Field::Exposure, Field::Contrast, Field::Highlights, Field::Shadows, Field::Dehaze] {
            assert_eq!(Settings::default().effective(f, &AUTO), 0.0, "{f:?}");
        }
    }

    #[test]
    fn hsv_round_trip_and_mixer_acts_only_on_its_colours() {
        for &(r, g, b) in &[(0.1, 0.5, 0.9), (0.9, 0.6, 0.1), (0.3, 0.3, 0.3), (0.0, 0.8, 0.7)] {
            let (h, s, v) = to_hsv(r, g, b);
            let (r2, g2, b2) = from_hsv(h, s, v);
            assert!((r - r2).abs() < 1e-5 && (g - g2).abs() < 1e-5 && (b - b2).abs() < 1e-5, "{r} {g} {b}");
        }
        // Wasserblau dunkler und blasser, Orange unberührt, Grau unberührt.
        let m = [(0.0, -0.5, -0.5), (0.0, 0.0, 0.0), (0.0, 0.0, 0.0)];
        let water = (0.1, 0.4, 0.8);
        let (wr, wg, wb) = mix(water.0, water.1, water.2, &m);
        assert!(wb < water.2 && to_hsv(wr, wg, wb).1 < to_hsv(water.0, water.1, water.2).1);
        assert_eq!(mix(0.9, 0.5, 0.1, &m), (0.9, 0.5, 0.1));
        assert_eq!(mix(0.5, 0.5, 0.5, &m), (0.5, 0.5, 0.5));

        let warm = Settings { effects: Effects { warm: ColorRange { saturation: 60.0, ..ColorRange::default() }, ..Effects::default() }, ..Settings::default() };
        let ops = PixelOps::new(&warm, &AUTO);
        let [r, _, b] = ops.color([0.8, 0.5, 0.3]);
        assert!(r - b > 0.8 - 0.3, "Orange kräftiger");
        assert_eq!(Settings::default().effective(Field::WarmSaturation, &AUTO), 0.0);
    }

    #[test]
    fn groups_copy_only_what_is_chosen() {
        let target = Settings {
            temperature: -10.0,
            tone: Tone { exposure: 20.0, ..Tone::default() },
            geometry: Geometry { quarter_turns: 1, ..Geometry::default() },
            ..Settings::default()
        };
        let source = Settings {
            auto: false,
            temperature: 30.0,
            strength: 0.5,
            tone: Tone { dehaze: 40.0, ..Tone::default() },
            detail: Detail { sharpen: 50.0, ..Detail::default() },
            geometry: Geometry { straighten: 3.0, ..Geometry::default() },
            ..Settings::default()
        };
        let all = target.with_groups(&source, Groups { geometry: true, ..Groups::default() });
        assert_eq!(all, source);
        let only_tone = target.with_groups(&source, Groups { color: false, tone: true, detail: false, geometry: false });
        assert_eq!(only_tone, Settings { tone: source.tone, ..target });
        let standard = target.with_groups(&source, Groups::default());
        assert_eq!(standard.geometry, target.geometry, "Zuschnitt standardmäßig nicht");
        assert_eq!((standard.temperature, standard.auto, standard.strength), (30.0, false, 0.5));
    }

    #[test]
    fn rotating_four_times_or_flipping_twice_restores_the_geometry() {
        let start = Geometry {
            quarter_turns: 1,
            straighten: 3.5,
            crop: Some(Crop { x: 0.1, y: 0.2, width: 0.5, height: 0.25 }),
            ..Geometry::default()
        };
        let close = |a: Geometry, b: Geometry| {
            let (ca, cb) = (a.crop.unwrap(), b.crop.unwrap());
            a.quarter_turns == b.quarter_turns
                && a.flip_horizontal == b.flip_horizontal
                && a.straighten == b.straighten
                && [ca.x - cb.x, ca.y - cb.y, ca.width - cb.width, ca.height - cb.height].iter().all(|d| d.abs() < 1e-6)
        };
        let mut g = start;
        for _ in 0..4 {
            g.rotate(true);
        }
        assert!(close(g, start));
        g.rotate(true);
        g.rotate(false);
        assert!(close(g, start), "rechts und links heben sich auf");
        let crop = g.crop.unwrap();
        g.rotate(true);
        assert_eq!(g.crop.unwrap().width, crop.height, "Zuschnitt dreht mit");
        g.rotate(false);
        g.flip();
        assert_eq!(g.straighten, -3.5);
        assert_eq!(g.quarter_turns, 3, "hochkant: Spiegeln plus 180°");
        g.flip();
        assert!(close(g, start));
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
