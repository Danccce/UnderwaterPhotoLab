//! Filter mit Nachbarschaft und die gemeinsame Pipeline, die Vorschau, Zoom-Ansicht und Export
//! rechnen.
//!
//! Radien werden in Promille der langen Kante des *ganzen* Bildes in der gerade gerechneten
//! Auflösung angegeben ([`Context::radius_px`]). Damit wirkt ein Filter in der 1280-px-Vorschau wie
//! im Export, und ein Ausschnitt der Zoom-Ansicht wie das ganze Bild in voller Auflösung, sofern er
//! mit [`margin_px`] Rand gerechnet wird. Ausnahme sind Schärfen und Rauschreduzierung: Sie wirken
//! auf Pixelebene, ihre Radien sind Pixel der vollen Auflösung ([`Context::full_px`]); die
//! verkleinerte Vorschau zeigt sie nur ungefähr (mindestens ein Pixel), genau die 100-%-Ansicht.

pub mod plane;

use std::{sync::Arc, thread};

use plane::{box_blur, gaussian, guided_coefficients, guided_filter, guided_filter_many, min_filter, Plane};
use uwcc_core::{luminance, process_rgba8, unit_rgb, Correction, PixelOps, Settings};

/// Lange Kante der [`Reference`].
const REFERENCE_EDGE: u32 = 256;
/// Dehaze: Fenster des dunklen Kanals (‰).
const DEHAZE_PATCH: f32 = 8.0;
/// Dehaze: Radius, mit dem die Durchlässigkeit kantenerhaltend geglättet wird (‰).
const DEHAZE_REFINE: f32 = 30.0;
/// Dehaze bei 100: Anteil des Schleiers, der entfernt wird (etwas bleibt, sonst wirkt es künstlich).
const DEHAZE_MAX: f32 = 0.95;
/// Untere Grenze der Durchlässigkeit; darunter würde Rauschen beliebig verstärkt.
const MIN_TRANSMISSION: f32 = 0.1;
/// Anteil der Referenz-Pixel mit dem hellsten dunklen Kanal, aus dem die Wasserfarbe gemittelt wird.
const BACKGROUND_SHARE: usize = 1000;
/// Lichter/Tiefen: Radius der geglätteten Helligkeit (‰).
const TONE_RADIUS: f32 = 30.0;
/// Lichter/Tiefen: ab dieser Varianz bleibt eine Kante in der geglätteten Helligkeit erhalten.
const TONE_EPS: f32 = 0.02;
/// Lichter/Tiefen bei ±100: so viele Blenden heller bzw. dunkler.
const TONE_STOPS: f32 = 1.2;
/// Grobe Filter rechnen auf einem Raster, in dem ihr Radius etwa so viele Zellen hat.
const GRID_RADIUS: f32 = 4.0;
/// Klarheit: Radius der Grundhelligkeit, gegen die der lokale Kontrast verstärkt wird (‰).
const CLARITY_RADIUS: f32 = 20.0;
/// Klarheit: Kanten mit mehr Varianz bleiben in der Grundhelligkeit, so entstehen keine Lichthöfe.
const CLARITY_EPS: f32 = 0.01;
/// Farbrauschen bei 100: Radius in Pixeln der vollen Auflösung.
const NOISE_COLOR_RADIUS: f32 = 8.0;
/// Farbrauschen: Varianz der Helligkeit, ab der die Farbe einer Kante folgt. Kleiner, und das
/// Helligkeitsrauschen des Leitbilds trägt das Farbrauschen mit hindurch (die Auto-Korrektur
/// spreizt die Kanäle bis ×4, entsprechend kräftig ist das Rauschen danach).
const NOISE_COLOR_EPS: f32 = 0.02;
/// Helligkeitsrauschen bei 100: Radius (Pixel der vollen Auflösung) und geglättete Schwankung (0–1).
const NOISE_LUMA_RADIUS: f32 = 3.0;
const NOISE_LUMA_LEVEL: f32 = 0.08;
/// Schärfen bei 100: so viel des Unterschieds zur unscharfen Kopie kommt dazu.
const SHARPEN_GAIN: f32 = 1.5;
/// Maskieren bei 100: Struktur (mittlere Abweichung von der unscharfen Kopie), ab der voll geschärft wird.
const SHARPEN_MASK_LEVEL: f32 = 0.04;

/// Kleine Kopie des ganzen, unbearbeiteten Bildes (lange Kante 256 px). Daraus schätzt Dehaze die
/// Wasserfarbe, für jeden Ausschnitt und jede Auflösung gleich.
#[derive(Clone, Debug)]
pub struct Reference {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
    /// Lange Kante des Originals in voller Auflösung.
    full_long_edge: u32,
}

impl Reference {
    /// Blockmittel von `rgba` (`width × height`) auf höchstens 256 px lange Kante.
    pub fn from_rgba(rgba: &[u8], width: u32, height: u32) -> Self {
        let full_long_edge = width.max(height);
        let f = full_long_edge.div_ceil(REFERENCE_EDGE).max(1) as usize;
        let (width, height) = (width as usize, height as usize);
        let (w, h) = (width.div_ceil(f), height.div_ceil(f));
        let mut sum = vec![[0f32; 4]; w * h];
        for (y, row) in rgba.chunks_exact(width * 4).enumerate() {
            let line = &mut sum[(y / f) * w..(y / f + 1) * w];
            for (x, p) in row.chunks_exact(4).enumerate() {
                let s = &mut line[x / f];
                s[0] += p[0] as f32;
                s[1] += p[1] as f32;
                s[2] += p[2] as f32;
                s[3] += 1.0;
            }
        }
        let rgb = sum.iter().map(|s| [s[0], s[1], s[2]].map(|v| v / s[3].max(1.0) / 255.0)).collect();
        Self { width: w, height: h, rgb, full_long_edge }
    }

    /// Hintergrundlicht (Wasserfarbe) nach der Farbmatrix: Mittel der hellsten 0,1 % im dunklen
    /// Kanal aus Grün und Blau (Unterwasser-Variante; Rot ist unter Wasser immer dunkel).
    fn background(&self, ops: &PixelOps) -> [f32; 3] {
        let rgb: Vec<[f32; 3]> = self.rgb.iter().map(|&p| ops.matrix(p)).collect();
        let dark = Plane { width: self.width, height: self.height, data: rgb.iter().map(|p| p[1].min(p[2])).collect() };
        let r = (DEHAZE_PATCH * self.width.max(self.height) as f32 / 1000.0).round().max(1.0) as usize;
        let dark = min_filter(&dark, r);
        let n = (rgb.len() / BACKGROUND_SHARE).max(1);
        let mut order: Vec<usize> = (0..rgb.len()).collect();
        let nth = rgb.len() - n;
        order.select_nth_unstable_by(nth, |&a, &b| dark.data[a].total_cmp(&dark.data[b]));
        let mut a = [0f32; 3];
        for &i in &order[nth..] {
            for c in 0..3 {
                a[c] += rgb[i][c] / n as f32;
            }
        }
        a.map(|v| v.max(0.05))
    }
}

/// Was jeder Ausschnitt eines Bildes gemeinsam hat.
#[derive(Clone, Debug)]
pub struct Context {
    /// Auto-Analyse des ganzen Bildes.
    pub auto: Correction,
    pub reference: Arc<Reference>,
    /// Lange Kante des ganzen Bildes in der gerade gerechneten Auflösung (Pixel).
    pub long_edge: u32,
    /// Lage des gerechneten Ausschnitts im ganzen Bild (Pixel dieser Auflösung); `(0, 0)` für das
    /// ganze Bild. Hält das Raster der groben Filter am ganzen Bild ausgerichtet.
    pub origin: (u32, u32),
}

impl Context {
    pub fn new(auto: Correction, reference: Arc<Reference>, width: u32, height: u32) -> Self {
        Self { auto, reference, long_edge: width.max(height), origin: (0, 0) }
    }

    /// Radius in Pixeln für `permille` Promille der langen Kante.
    pub fn radius_px(&self, permille: f32) -> f32 {
        permille * self.long_edge as f32 / 1000.0
    }

    /// `px` Pixel der vollen Auflösung in Pixeln der gerade gerechneten Auflösung.
    pub fn full_px(&self, px: f32) -> f32 {
        px * self.long_edge as f32 / self.reference.full_long_edge.max(1) as f32
    }
}

/// Grobes Raster für Filter, deren Ergebnis sich nur langsam ändert: Zellen von `s × s` Pixeln,
/// am ganzen Bild ausgerichtet, damit jeder Ausschnitt dieselben Zellen sieht.
struct Grid {
    s: usize,
    origin: (usize, usize),
    /// Erste Zelle (Spalte, Zeile) des Ausschnitts im Raster des ganzen Bildes.
    first: (usize, usize),
    w: usize,
    h: usize,
    lw: usize,
    lh: usize,
}

impl Grid {
    /// Raster, in dem `radius_px` etwa [`GRID_RADIUS`] Zellen sind.
    fn new(radius_px: f32, w: usize, h: usize, origin: (u32, u32)) -> Self {
        let s = ((radius_px / GRID_RADIUS) as usize).max(1);
        let origin = (origin.0 as usize, origin.1 as usize);
        let first = (origin.0 / s, origin.1 / s);
        let lw = (origin.0 + w).div_ceil(s) - first.0;
        let lh = (origin.1 + h).div_ceil(s) - first.1;
        Self { s, origin, first, w, h, lw, lh }
    }

    /// Radius in Zellen für `px` Pixel.
    fn radius(&self, px: f32) -> usize {
        (px / self.s as f32).round().max(1.0) as usize
    }

    /// Pixelbereich des Ausschnitts, der in Zelle `l` (Spalte bzw. Zeile) liegt.
    fn span(&self, l: usize, first: usize, origin: usize, len: usize) -> std::ops::Range<usize> {
        let start = (first + l) * self.s;
        start.max(origin) - origin..(start + self.s).min(origin + len) - origin
    }

    /// Je Zelle für jede der `K` Größen aus `value(Pixelindex)` das Mittel bzw. – wo `min` gesetzt
    /// ist – das Minimum; alle in einem Durchgang über die Pixel.
    fn pool<const K: usize>(&self, value: impl Fn(usize) -> [f32; K] + Sync, min: [bool; K]) -> [Plane; K] {
        let mut cells = vec![[0f32; K]; self.lw * self.lh];
        let mut rows: Vec<&mut [[f32; K]]> = cells.chunks_mut(self.lw).collect();
        par_each(&mut rows, |ly, row| {
            let ys = self.span(ly, self.first.1, self.origin.1, self.h);
            for (lx, cell) in row.iter_mut().enumerate() {
                let xs = self.span(lx, self.first.0, self.origin.0, self.w);
                let mut acc = min.map(|m| if m { f32::MAX } else { 0.0 });
                for y in ys.clone() {
                    for x in xs.clone() {
                        for (k, v) in value(y * self.w + x).into_iter().enumerate() {
                            acc[k] = if min[k] { acc[k].min(v) } else { acc[k] + v };
                        }
                    }
                }
                let n = (ys.len() * xs.len()) as f32;
                *cell = core::array::from_fn(|k| if min[k] { acc[k] } else { acc[k] / n });
            }
        });
        core::array::from_fn(|k| Plane { width: self.lw, height: self.lh, data: cells.iter().map(|c| c[k]).collect() })
    }

    /// Stützstellen (Zellmitten) und Gewicht für Pixel `i` einer Richtung.
    fn weights(&self, i: usize, origin: usize, first: usize, cells: usize) -> Lerp {
        let g = (((i + origin) as f32 + 0.5) / self.s as f32 - 0.5 - first as f32).clamp(0.0, (cells - 1) as f32);
        let i0 = g as usize;
        (i0, (i0 + 1).min(cells - 1), g - i0 as f32)
    }

    /// Gewichte für alle Spalten des Ausschnitts.
    fn columns(&self) -> Vec<Lerp> {
        (0..self.w).map(|x| self.weights(x, self.origin.0, self.first.0, self.lw)).collect()
    }

    /// Zeile `y` des Ausschnitts aus `p`, in y-Richtung schon interpoliert (ein Wert je Rasterspalte);
    /// je Pixel bleibt dann nur [`lerp`] in x-Richtung.
    fn row(&self, p: &Plane, y: usize) -> Vec<f32> {
        let (y0, y1, fy) = self.weights(y, self.origin.1, self.first.1, self.lh);
        (0..self.lw).map(|lx| p.get(lx, y0) + (p.get(lx, y1) - p.get(lx, y0)) * fy).collect()
    }
}

/// Zwei Stützstellen und das Gewicht der zweiten.
type Lerp = (usize, usize, f32);

fn lerp(row: &[f32], (i0, i1, f): Lerp) -> f32 {
    row[i0] + (row[i1] - row[i0]) * f
}

/// Stärke der Filter mit Nachbarschaft, jeweils −1 … 1 bzw. 0 … 1.
struct Neighborhood {
    dehaze: f32,
    shadows: f32,
    highlights: f32,
    clarity: f32,
    sharpen: f32,
    /// Pixel der vollen Auflösung.
    sharpen_radius: f32,
    sharpen_mask: f32,
    noise_color: f32,
    noise_luma: f32,
}

impl Neighborhood {
    fn of(settings: &Settings) -> Self {
        let (t, d) = (&settings.tone, &settings.detail);
        let unit = |v: f32| v.clamp(0.0, 100.0) / 100.0;
        Self {
            dehaze: unit(t.dehaze),
            shadows: t.shadows.clamp(-100.0, 100.0) / 100.0,
            highlights: t.highlights.clamp(-100.0, 100.0) / 100.0,
            clarity: d.clarity.clamp(-100.0, 100.0) / 100.0,
            sharpen: unit(d.sharpen),
            sharpen_radius: d.sharpen_radius.clamp(0.5, 3.0),
            sharpen_mask: unit(d.sharpen_mask),
            noise_color: unit(d.noise_color),
            noise_luma: unit(d.noise_luma),
        }
    }

    fn tone(&self) -> bool {
        self.shadows != 0.0 || self.highlights != 0.0
    }

    fn detail(&self) -> bool {
        self.clarity != 0.0 || self.sharpen > 0.0 || self.noise_color > 0.0 || self.noise_luma > 0.0
    }

    fn any(&self) -> bool {
        self.dehaze > 0.0 || self.tone() || self.detail()
    }
}

/// Rand in Pixeln (dieser Auflösung), den ein Ausschnitt rundum mitrechnen muss, damit Filter mit
/// Nachbarschaft wie im ganzen Bild wirken. Die Filter wirken nacheinander, ihre Reichweiten
/// addieren sich. Der Guided Filter reicht zwei Radien weit, Rasterzellen und Interpolation noch
/// einmal einige Zellen; die Gauß-Näherung etwa drei Sigma.
pub fn margin_px(settings: &Settings, ctx: &Context) -> u32 {
    let n = Neighborhood::of(settings);
    let grid = |radius: f32, patch: f32| radius * 2.0 + patch + 3.0 * (radius / GRID_RADIUS).max(1.0);
    let mut r = 0.0f32;
    if n.dehaze > 0.0 {
        r += grid(ctx.radius_px(DEHAZE_REFINE), ctx.radius_px(DEHAZE_PATCH));
    }
    if n.tone() {
        r += grid(ctx.radius_px(TONE_RADIUS), 0.0);
    }
    if n.clarity != 0.0 {
        r += grid(ctx.radius_px(CLARITY_RADIUS), 0.0);
    }
    if n.noise_color > 0.0 {
        r += 2.0 * noise_color_radius(&n, ctx) as f32;
    }
    if n.noise_luma > 0.0 {
        r += 2.0 * noise_luma_radius(&n, ctx) as f32;
    }
    if n.sharpen > 0.0 {
        let sigma = ctx.full_px(n.sharpen_radius);
        r += 3.0 * sigma + sigma.round().max(1.0) + 1.0;
    }
    (r.ceil() as u32 + 2) * u32::from(n.any())
}

fn noise_color_radius(n: &Neighborhood, ctx: &Context) -> usize {
    ctx.full_px(1.0 + (NOISE_COLOR_RADIUS - 1.0) * n.noise_color).round().max(1.0) as usize
}

fn noise_luma_radius(n: &Neighborhood, ctx: &Context) -> usize {
    ctx.full_px(1.0 + (NOISE_LUMA_RADIUS - 1.0) * n.noise_luma).round().max(1.0) as usize
}

fn threads() -> usize {
    thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8)
}

/// `f(index, item)` für alle Elemente, auf alle Kerne verteilt.
fn par_each<T: Send>(data: &mut [T], f: impl Fn(usize, &mut T) + Sync) {
    let chunk = data.len().div_ceil(threads()).max(1);
    thread::scope(|s| {
        for (k, part) in data.chunks_mut(chunk).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (i, v) in part.iter_mut().enumerate() {
                    f(k * chunk + i, v);
                }
            });
        }
    });
}

/// Die Pipeline für 8-Bit-RGBA: `width × height` Pixel, gerechnet mit `settings`.
/// Ohne Filter mit Nachbarschaft genügt die Pipeline je Pixel, sonst: Farbmatrix → Dehaze →
/// Weißabgleich, Dynamik, Belichtung → Lichter/Tiefen → Klarheit → Farbrauschen →
/// Helligkeitsrauschen → Schärfen → Kontrast, Stärke.
pub fn render(rgba: &mut [u8], width: u32, height: u32, settings: &Settings, ctx: &Context) {
    debug_assert_eq!(rgba.len(), width as usize * height as usize * 4);
    let n = Neighborhood::of(settings);
    if !n.any() {
        let chunk = ((rgba.len() / 4).div_ceil(threads()) * 4).max(4);
        thread::scope(|s| {
            for part in rgba.chunks_mut(chunk) {
                s.spawn(|| process_rgba8(part, settings, &ctx.auto));
            }
        });
        return;
    }

    let (w, h) = (width as usize, height as usize);
    let ops = PixelOps::new(settings, &ctx.auto);
    let mut rgb = vec![[0f32; 3]; w * h];
    {
        let source: &[u8] = rgba;
        let later = n.dehaze > 0.0;
        par_each(&mut rgb, |i, v| {
            let m = ops.matrix(unit_rgb(&source[i * 4..]));
            *v = if later { m } else { ops.color(m) };
        });
    }
    if n.dehaze > 0.0 {
        dehaze(&mut rgb, w, h, n.dehaze, &ops, ctx);
    }
    let mut tone = n.tone().then(|| ToneMap::new(&rgb, w, h, &n, ctx));
    if n.detail() {
        // Die Details brauchen das Ergebnis von Lichter/Tiefen als ganze Ebene.
        if let Some(t) = tone.take() {
            par_rows(&mut rgb, w, |y, row| {
                let r = t.row(y);
                for (x, p) in row.iter_mut().enumerate() {
                    *p = t.apply(*p, &r, x);
                }
            });
        }
        details(&mut rgb, w, h, &n, ctx);
    }
    par_rows(rgba, w * 4, |y, row| {
        let tone_row = tone.as_ref().map(|t| t.row(y));
        for (x, p) in row.chunks_exact_mut(4).enumerate() {
            let v = rgb[y * w + x];
            let v = match (&tone, &tone_row) {
                (Some(t), Some(r)) => t.apply(v, r, x),
                _ => v,
            };
            let out = ops.finish(v, unit_rgb(p));
            p[..3].copy_from_slice(&out);
        }
    });
}

/// `f(y, Zeile)` für Zeilen aus je `stride` Elementen, auf alle Kerne verteilt.
fn par_rows<T: Send>(data: &mut [T], stride: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let per = (data.len() / stride.max(1)).div_ceil(threads()).max(1);
    thread::scope(|s| {
        for (k, part) in data.chunks_mut(per * stride).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (j, row) in part.chunks_mut(stride).enumerate() {
                    f(k * per + j, row);
                }
            });
        }
    });
}

fn lum(p: &[f32; 3]) -> f32 {
    luminance(p[0], p[1], p[2])
}

/// Unterwasser-Dehaze nach dem Modell `I = J·t + A·(1 − t)`: Durchlässigkeit `t` aus dem dunklen
/// Kanal von Grün und Blau (relativ zur Wasserfarbe `A`), mit dem Guided Filter an die Kanten des
/// Bildes angepasst, dann `J = (I − A) / t + A`. `t` ändert sich nur langsam und wird auf dem
/// groben Raster geschätzt; die Kanten kommen über das volle Leitbild hinein. Danach folgen
/// Weißabgleich, Dynamik und Belichtung im selben Durchgang.
fn dehaze(rgb: &mut [[f32; 3]], w: usize, h: usize, amount: f32, ops: &PixelOps, ctx: &Context) {
    let a = ctx.reference.background(ops);
    let refine = ctx.radius_px(DEHAZE_REFINE);
    let grid = Grid::new(refine, w, h, ctx.origin);
    let [dark, guide] = grid.pool(|i| [(rgb[i][1] / a[1]).min(rgb[i][2] / a[2]), lum(&rgb[i])], [true, false]);
    let t = min_filter(&dark, grid.radius(ctx.radius_px(DEHAZE_PATCH))).map(|d| 1.0 - amount * DEHAZE_MAX * d);
    let (ca, cb) = guided_coefficients(&guide, &t, grid.radius(refine), 1e-3);
    let cols = grid.columns();
    par_rows(rgb, w, |y, row| {
        let (ra, rb) = (grid.row(&ca, y), grid.row(&cb, y));
        for (x, p) in row.iter_mut().enumerate() {
            let t = (lerp(&ra, cols[x]) * lum(p) + lerp(&rb, cols[x])).max(MIN_TRANSMISSION);
            *p = ops.color(core::array::from_fn(|c| (p[c] - a[c]) / t + a[c]));
        }
    });
}

/// Hebt Tiefen bzw. senkt Lichter nach der *geglätteten* Helligkeit der Umgebung; der Faktor
/// ändert sich nur langsam, so bleibt der lokale Kontrast erhalten, und der Guided Filter
/// verhindert Lichthöfe an Kanten.
struct ToneMap {
    grid: Grid,
    cols: Vec<Lerp>,
    a: Plane,
    b: Plane,
    shadows: f32,
    highlights: f32,
}

impl ToneMap {
    fn new(rgb: &[[f32; 3]], w: usize, h: usize, n: &Neighborhood, ctx: &Context) -> Self {
        let radius = ctx.radius_px(TONE_RADIUS);
        let grid = Grid::new(radius, w, h, ctx.origin);
        let [low] = grid.pool(|i| [lum(&rgb[i])], [false]);
        let (a, b) = guided_coefficients(&low, &low, grid.radius(radius), TONE_EPS);
        Self { cols: grid.columns(), grid, a, b, shadows: n.shadows, highlights: n.highlights }
    }

    /// Die Koeffizienten der Zeile `y`, für [`Self::apply`].
    fn row(&self, y: usize) -> (Vec<f32>, Vec<f32>) {
        (self.grid.row(&self.a, y), self.grid.row(&self.b, y))
    }

    fn apply(&self, p: [f32; 3], (ra, rb): &(Vec<f32>, Vec<f32>), x: usize) -> [f32; 3] {
        let base = lerp(ra, self.cols[x]) * lum(&p) + lerp(rb, self.cols[x]);
        let shadow = ((0.6 - base) / 0.6).clamp(0.0, 1.0).powi(2);
        let highlight = ((base - 0.4) / 0.6).clamp(0.0, 1.0).powi(2);
        let f = (TONE_STOPS * (self.shadows * shadow + self.highlights * highlight)).exp2();
        p.map(|v| v * f)
    }
}

/// Ebene aus `f(Pixelindex)`, auf alle Kerne verteilt.
fn plane_of(w: usize, h: usize, f: impl Fn(usize) -> f32 + Sync) -> Plane {
    let mut data = vec![0f32; w * h];
    par_each(&mut data, |i, v| *v = f(i));
    Plane { width: w, height: h, data }
}

/// Klarheit, Rauschreduzierung und Schärfen auf der Helligkeit Y und den Farbanteilen R − Y,
/// B − Y; zum Schluss wieder RGB mit derselben Gewichtung wie [`luminance`].
fn details(rgb: &mut [[f32; 3]], w: usize, h: usize, n: &Neighborhood, ctx: &Context) {
    let y0 = plane_of(w, h, |i| lum(&rgb[i]));
    let mut y = y0.clone();

    // Klarheit: Abstand zur kantenerhaltend geglätteten Grundhelligkeit verstärken, am meisten in
    // den Mitteltönen, damit Lichter und Tiefen nicht ausbrechen.
    if n.clarity != 0.0 {
        let radius = ctx.radius_px(CLARITY_RADIUS);
        let grid = Grid::new(radius, w, h, ctx.origin);
        let [low] = grid.pool(|i| [y.data[i]], [false]);
        let (a, b) = guided_coefficients(&low, &low, grid.radius(radius), CLARITY_EPS);
        let cols = grid.columns();
        par_rows(&mut y.data, w, |row_y, row| {
            let (ra, rb) = (grid.row(&a, row_y), grid.row(&b, row_y));
            for (x, v) in row.iter_mut().enumerate() {
                let base = lerp(&ra, cols[x]) * *v + lerp(&rb, cols[x]);
                let mid = 1.0 - (2.0 * v.clamp(0.0, 1.0) - 1.0).powi(2);
                *v += n.clarity * mid * (*v - base);
            }
        });
    }

    // Rauschen und Schärfen liegen auf Pixelebene. In der verkleinerten Vorschau wäre ihr Radius
    // kleiner als ein Pixel; dort rechnen sie mit mindestens einem Pixel und zeigen die Wirkung
    // nur ungefähr – genau stimmt sie in der 100-%-Ansicht und im Export.

    // Farbrauschen: Farbanteile mit Y als Leitbild glätten; Farbkanten bleiben, wo Helligkeitskanten sind.
    let chroma = (n.noise_color > 0.0).then(|| {
        let cr = plane_of(w, h, |i| rgb[i][0] - y0.data[i]);
        let cb = plane_of(w, h, |i| rgb[i][2] - y0.data[i]);
        let smooth = guided_filter_many(&y, &[&cr, &cb], noise_color_radius(n, ctx), NOISE_COLOR_EPS);
        let blend = |c: &Plane, s: &Plane| c.zip(s, |c, s| c + n.noise_color * (s - c));
        (blend(&cr, &smooth[0]), blend(&cb, &smooth[1]))
    });

    // Helligkeitsrauschen: Schwankungen bis etwa `NOISE_LUMA_LEVEL` glätten, größere (Kanten) bleiben.
    if n.noise_luma > 0.0 {
        let eps = (NOISE_LUMA_LEVEL * n.noise_luma).powi(2);
        y = guided_filter(&y, &y, noise_luma_radius(n, ctx), eps);
    }

    // Schärfen: Unscharf maskieren; mit Maskieren nur dort, wo genug Struktur ist.
    let sigma = ctx.full_px(n.sharpen_radius);
    if n.sharpen > 0.0 {
        // Unter einem halben Pixel (Vorschau) der kleinste Kern: 3 × 3-Mittel.
        let blur = if sigma >= 0.5 { gaussian(&y, sigma) } else { box_blur(&y, 1) };
        let detail = y.zip(&blur, |v, b| v - b);
        let mask = (n.sharpen_mask > 0.0).then(|| {
            let level = SHARPEN_MASK_LEVEL * n.sharpen_mask;
            box_blur(&detail.map(f32::abs), sigma.round().max(1.0) as usize)
                .map(|e| ((e - level * 0.5) / (level * 0.5)).clamp(0.0, 1.0))
        });
        let gain = SHARPEN_GAIN * n.sharpen;
        par_each(&mut y.data, |i, v| *v += gain * mask.as_ref().map_or(1.0, |m| m.data[i]) * detail.data[i]);
    }

    par_each(rgb, |i, p| {
        let v = y.data[i];
        let (cr, cb) = match &chroma {
            Some((cr, cb)) => (cr.data[i], cb.data[i]),
            None => (p[0] - y0.data[i], p[2] - y0.data[i]),
        };
        let (r, b) = (v + cr, v + cb);
        *p = [r, (v - 0.2126 * r - 0.0722 * b) / 0.7152, b];
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use uwcc_core::{Detail, Tone};

    /// Unterwasserartiges Testbild: blaugrüner Schleier, nach unten dunkler, mit Strukturen.
    fn scene(w: u32, h: u32) -> Vec<u8> {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
                let stripes = if ((fx * 12.0) as u32 + (fy * 9.0) as u32) % 2 == 0 { 0.25 } else { 0.0 };
                let haze = 0.5 * (1.0 - fy);
                let r = 0.1 + stripes * 0.5;
                let g = 0.3 + haze * 0.6 + stripes;
                let b = 0.35 + haze * 0.7 + stripes * 0.8;
                px.extend([r, g, b].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
                px.push(255);
            }
        }
        px
    }

    fn ctx(px: &[u8], w: u32, h: u32) -> Context {
        Context::new(Correction::NONE, Arc::new(Reference::from_rgba(px, w, h)), w, h)
    }

    /// Pseudo-Zufall −1 … 1 je Pixel und Kanal, ohne Abhängigkeiten.
    fn noise(i: u32, c: u32) -> f32 {
        let mut x = i.wrapping_mul(747_796_405).wrapping_add(c.wrapping_mul(2_891_336_453)).wrapping_add(1);
        x ^= x >> 13;
        x = x.wrapping_mul(1_274_126_177);
        x ^= x >> 16;
        (x % 2001) as f32 / 1000.0 - 1.0
    }

    fn with_detail(detail: Detail) -> Settings {
        Settings { auto: false, detail, ..Settings::default() }
    }

    /// Standardabweichung von Kanal `c` im Rechteck `x0..x1 × y0..y1`.
    fn spread(img: &[u8], w: u32, (x0, x1, y0, y1): (u32, u32, u32, u32), c: usize) -> f32 {
        let v: Vec<f32> = (y0..y1).flat_map(|y| (x0..x1).map(move |x| img[((y * w + x) * 4) as usize + c] as f32)).collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn sharpen_adds_edge_contrast_and_the_mask_spares_flat_water() {
        let (w, h) = (128, 64);
        // Links ruhiges Wasser mit leichtem Rauschen, rechts harte Streifen.
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let x = i % w;
                let base = if x < 64 { 120.0 + 2.0 * noise(i, 0) } else if (x / 4) % 2 == 0 { 60.0 } else { 190.0 };
                [base as u8, base as u8, base as u8, 255]
            })
            .collect();
        let c = ctx(&px, w, h);
        let sharpen = |mask: f32| {
            let mut out = px.clone();
            let d = Detail { sharpen: 100.0, sharpen_radius: 1.0, sharpen_mask: mask, ..Detail::default() };
            render(&mut out, w, h, &with_detail(d), &c);
            out
        };
        let (plain, masked) = (sharpen(0.0), sharpen(60.0));
        let water = (8, 56, 8, 56);
        let stripes = (72, 120, 8, 56);
        assert!(spread(&plain, w, stripes, 1) > spread(&px, w, stripes, 1) * 1.05, "Kanten werden kräftiger");
        assert!(spread(&plain, w, water, 1) > spread(&px, w, water, 1) * 1.3, "ohne Maske auch das Rauschen");
        assert!(spread(&masked, w, water, 1) < spread(&px, w, water, 1) * 1.1, "mit Maske bleibt Wasser ruhig");
        assert!(spread(&masked, w, stripes, 1) > spread(&px, w, stripes, 1) * 1.05, "Kanten trotzdem geschärft");
    }

    #[test]
    fn colour_noise_reduction_smooths_colour_but_keeps_brightness() {
        let (w, h) = (96, 96);
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (r, b) = (128.0 + 25.0 * noise(i, 0), 128.0 + 25.0 * noise(i, 2));
                // Grün so, dass die Helligkeit überall 128 ist: nur Farbrauschen.
                let g = (128.0 - 0.2126 * r - 0.0722 * b) / 0.7152;
                [r as u8, g.clamp(0.0, 255.0) as u8, b as u8, 255]
            })
            .collect();
        let mut out = px.clone();
        render(&mut out, w, h, &with_detail(Detail { noise_color: 100.0, ..Detail::default() }), &ctx(&px, w, h));
        let area = (10, 86, 10, 86);
        assert!(spread(&out, w, area, 0) < spread(&px, w, area, 0) * 0.5, "Rot ruhiger");
        assert!(spread(&out, w, area, 2) < spread(&px, w, area, 2) * 0.5, "Blau ruhiger");
        let mean_lum = |img: &[u8]| img.chunks_exact(4).map(|p| luminance(p[0] as f32, p[1] as f32, p[2] as f32)).sum::<f32>() / (w * h) as f32;
        assert!((mean_lum(&out) - mean_lum(&px)).abs() < 1.5, "Helligkeit bleibt");
    }

    #[test]
    fn clarity_raises_local_contrast_in_the_midtones() {
        let (w, h) = (200, 150);
        let px = scene(w, h);
        let mut out = px.clone();
        render(&mut out, w, h, &with_detail(Detail { clarity: 80.0, ..Detail::default() }), &ctx(&px, w, h));
        let area = (20, 180, 10, 70);
        assert!(spread(&out, w, area, 1) > spread(&px, w, area, 1) * 1.05);
        let mut soft = px.clone();
        render(&mut soft, w, h, &with_detail(Detail { clarity: -80.0, ..Detail::default() }), &ctx(&px, w, h));
        assert!(spread(&soft, w, area, 1) < spread(&px, w, area, 1));
    }

    /// Zeiten der Vorschau-Größe; `cargo test --release -p uwcc-filters timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing() {
        let (w, h) = (1280, 960);
        let px = scene(w, h);
        let c = ctx(&px, w, h);
        let time = |name: &str, s: &Settings| {
            let start = std::time::Instant::now();
            for _ in 0..10 {
                let mut out = px.clone();
                render(&mut out, w, h, s, &c);
            }
            println!("{name}: {:?}", start.elapsed() / 10);
        };
        let base = Settings::default();
        time("nur je Pixel", &base);
        time("Dehaze", &Settings { tone: Tone { dehaze: 50.0, ..Tone::default() }, ..base });
        time("Lichter/Tiefen", &Settings { tone: Tone { shadows: 50.0, ..Tone::default() }, ..base });
        time("beides", &Settings { tone: Tone { shadows: 50.0, dehaze: 50.0, ..Tone::default() }, ..base });
        let ops = PixelOps::new(&base, &c.auto);
        let start = std::time::Instant::now();
        for _ in 0..10 {
            std::hint::black_box(c.reference.background(&ops));
        }
        println!("Wasserfarbe: {:?}", start.elapsed() / 10);

        let rgb: Vec<[f32; 3]> = px.chunks_exact(4).map(unit_rgb).collect();
        let n = Neighborhood::of(&Settings { tone: Tone { shadows: 50.0, ..Tone::default() }, ..base });
        let (w, h) = (w as usize, h as usize);
        let grid = Grid::new(c.radius_px(TONE_RADIUS), w, h, (0, 0));
        let start = std::time::Instant::now();
        let [low] = grid.pool(|i| [lum(&rgb[i])], [false]);
        println!("Raster {}×{} (Zelle {}): {:?}", grid.lw, grid.lh, grid.s, start.elapsed());
        let start = std::time::Instant::now();
        let (a, b) = guided_coefficients(&low, &low, grid.radius(c.radius_px(TONE_RADIUS)), TONE_EPS);
        println!("Guided Filter grob: {:?}", start.elapsed());
        let tone = ToneMap { cols: grid.columns(), grid, a, b, shadows: n.shadows, highlights: n.highlights };
        let start = std::time::Instant::now();
        let mut sum = 0.0;
        for (y, row) in rgb.chunks_exact(w).enumerate() {
            let r = tone.row(y);
            for (x, p) in row.iter().enumerate() {
                sum += tone.apply(*p, &r, x)[0];
            }
        }
        println!("Anwenden (1 Kern): {:?} {sum}", start.elapsed());
    }

    #[test]
    fn radius_scales_with_the_image() {
        let px = scene(8, 6);
        let reference = Arc::new(Reference::from_rgba(&px, 8, 6));
        assert_eq!(Context::new(Correction::NONE, reference.clone(), 4608, 3456).radius_px(10.0), 46.08);
        assert_eq!(Context::new(Correction::NONE, reference, 960, 1280).radius_px(10.0), 12.8);
    }

    #[test]
    fn render_equals_the_per_pixel_pipeline() {
        let auto = Correction { hue_shift: 60.0, ..Correction::NONE };
        let s = Settings { temperature: 20.0, vibrance: 15.0, ..Settings::default() };
        let px: Vec<u8> = (0..97 * 4).map(|i| (i * 37 % 251) as u8).collect();
        let mut a = px.clone();
        render(&mut a, 97, 1, &s, &Context { auto, ..ctx(&px, 97, 1) });
        let mut b = px;
        process_rgba8(&mut b, &s, &auto);
        assert_eq!(a, b);
    }

    #[test]
    fn reference_averages_blocks() {
        let px: Vec<u8> = [[255, 0, 0, 255], [0, 0, 255, 255]].iter().cycle().take(600 * 2).flatten().copied().collect();
        let r = Reference::from_rgba(&px, 600, 2);
        assert_eq!((r.width, r.height), (200, 1));
        assert!((r.rgb[0][0] - 2.0 / 3.0).abs() < 1e-6 && (r.rgb[0][2] - 1.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn dehaze_adds_contrast_and_keeps_size() {
        let (w, h) = (160, 120);
        let px = scene(w, h);
        let s = Settings { auto: false, tone: Tone { dehaze: 80.0, ..Tone::default() }, ..Settings::default() };
        let mut out = px.clone();
        render(&mut out, w, h, &s, &ctx(&px, w, h));
        let spread = |img: &[u8]| {
            let g: Vec<f32> = img.chunks_exact(4).map(|p| p[1] as f32).collect();
            let mean = g.iter().sum::<f32>() / g.len() as f32;
            (g.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / g.len() as f32).sqrt()
        };
        assert!(spread(&out) > spread(&px) * 1.2, "{} vs {}", spread(&out), spread(&px));
        assert!(out.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn shadows_lift_dark_areas_more_than_bright_ones() {
        let (w, h) = (64, 64);
        let px: Vec<u8> = (0..w * h).flat_map(|i| if i % w < 32 { [40, 40, 40, 255] } else { [210, 210, 210, 255] }).collect();
        let s = Settings { auto: false, tone: Tone { shadows: 100.0, ..Tone::default() }, ..Settings::default() };
        let mut out = px.clone();
        render(&mut out, w, h, &s, &ctx(&px, w, h));
        let at = |x: u32| out[((32 * w + x) * 4) as usize];
        assert!(at(5) > 60, "Tiefen deutlich heller: {}", at(5));
        assert!(at(60) <= 212, "Lichter fast unverändert: {}", at(60));
        assert!(at(30) > 55 && at(34) <= 215, "kein Lichthof an der Kante: {} {}", at(30), at(34));
    }

    /// 9.4: Vorschau (verkleinert gerechnet) ≈ Export (voll gerechnet, dann verkleinert).
    #[test]
    fn preview_matches_export_with_neighborhood_filters() {
        let (w, h) = (640, 480);
        let px = scene(w, h);
        let s = Settings {
            auto: false,
            tone: Tone { dehaze: 60.0, shadows: 50.0, highlights: -50.0, ..Tone::default() },
            ..Settings::default()
        };
        let reference = Arc::new(Reference::from_rgba(&px, w, h));
        let mut full = px.clone();
        render(&mut full, w, h, &s, &Context::new(Correction::NONE, reference.clone(), w, h));

        let shrink = |img: &[u8]| -> Vec<u8> {
            let mut out = Vec::new();
            for y in 0..h / 4 {
                for x in 0..w / 4 {
                    let mut sum = [0u32; 4];
                    for dy in 0..4 {
                        for dx in 0..4 {
                            let i = (((y * 4 + dy) * w + x * 4 + dx) * 4) as usize;
                            (0..4).for_each(|c| sum[c] += img[i + c] as u32);
                        }
                    }
                    out.extend(sum.map(|v| (v / 16) as u8));
                }
            }
            out
        };
        let mut small = shrink(&px);
        render(&mut small, w / 4, h / 4, &s, &Context::new(Correction::NONE, reference, w / 4, h / 4));
        let export = shrink(&full);
        let diff: f32 = small.iter().zip(&export).map(|(&a, &b)| (a as f32 - b as f32).abs()).sum::<f32>() / small.len() as f32;
        assert!(diff < 4.0, "mittlere Abweichung {diff}");
    }

    #[test]
    fn a_crop_with_margin_matches_the_whole_image() {
        let (w, h) = (1600, 1200);
        let px = scene(w, h);
        let s = Settings {
            auto: false,
            tone: Tone { dehaze: 70.0, shadows: 60.0, ..Tone::default() },
            detail: Detail {
                clarity: 40.0,
                sharpen: 60.0,
                sharpen_radius: 1.5,
                sharpen_mask: 20.0,
                noise_color: 50.0,
                noise_luma: 30.0,
            },
            ..Settings::default()
        };
        let c = ctx(&px, w, h);
        let mut whole = px.clone();
        render(&mut whole, w, h, &s, &c);

        let m = margin_px(&s, &c);
        let (x0, y0, cw, ch) = (701u32, 523u32, 90u32, 70u32);
        let (cx, cy) = (x0 - m.min(x0), y0 - m.min(y0));
        let (cx1, cy1) = ((x0 + cw + m).min(w), (y0 + ch + m).min(h));
        let (ew, eh) = (cx1 - cx, cy1 - cy);
        let mut crop: Vec<u8> = (cy..cy1).flat_map(|y| px[((y * w + cx) * 4) as usize..((y * w + cx1) * 4) as usize].to_vec()).collect();
        render(&mut crop, ew, eh, &s, &Context { origin: (cx, cy), ..c.clone() });
        for y in y0..y0 + ch {
            for x in x0..x0 + cw {
                let a = &whole[((y * w + x) * 4) as usize..][..3];
                let b = &crop[(((y - cy) * ew + x - cx) * 4) as usize..][..3];
                for k in 0..3 {
                    assert!((a[k] as i32 - b[k] as i32).abs() <= 1, "({x},{y}): {a:?} vs {b:?}");
                }
            }
        }
    }
}
