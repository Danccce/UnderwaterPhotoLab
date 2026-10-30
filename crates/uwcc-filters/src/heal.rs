//! Partikel (Schwebeteilchen im Blitzlicht) ausbessern: jeden Fleck aus der passendsten Stelle
//! seiner Umgebung füllen, mit weichem Rand und angeglichener Helligkeit (wie ein Reparaturpinsel).
//! Gerechnet wird im aufrechten Bild vor Drehen und Zuschnitt; die Flecken sind in Anteilen
//! gespeichert und wirken daher in jeder Auflösung gleich.

use uwcc_core::Spot;

/// Richtungen, in denen eine Quelle gesucht wird.
const DIRECTIONS: usize = 16;
/// Abstände der Quelle (in Radien); weit genug, dass sich Quelle und Ziel mit Rand nicht überlappen.
const DISTANCES: [f32; 2] = [2.8, 4.0];
/// Bis hier (in Radien) wird voll ersetzt, bis `FEATHER` weich ausgeblendet.
const SOLID: f32 = 0.9;
const FEATHER: f32 = 1.3;
/// Vergleichsring um Ziel und Quelle (in Radien).
const RING: (f32, f32) = (1.35, 1.9);

/// Bessert alle `spots` in `rgba` (`w × h`, aufrecht) aus, der Reihe nach.
pub fn heal(rgba: &mut [u8], w: u32, h: u32, spots: &[Spot]) {
    let long = w.max(h) as f32;
    for s in spots {
        let (cx, cy) = (s.x * w as f32, s.y * h as f32);
        let r = (s.radius * long).max(1.0);
        if let Some((dx, dy)) = best_source(rgba, w, h, cx, cy, r) {
            fill(rgba, w, h, cx, cy, r, dx, dy);
        }
    }
}

/// Ganzzahlige Punkte im Ring `RING` um (0, 0), bei großen Radien ausgedünnt.
fn ring(r: f32) -> Vec<(i32, i32)> {
    let step = ((r / 4.0) as i32).max(1);
    let outer = (r * RING.1).ceil() as i32;
    let mut pts = Vec::new();
    for y in (-outer..=outer).step_by(step as usize) {
        for x in (-outer..=outer).step_by(step as usize) {
            let d = ((x * x + y * y) as f32).sqrt() / r;
            if (RING.0..=RING.1).contains(&d) {
                pts.push((x, y));
            }
        }
    }
    pts
}

fn pixel(rgba: &[u8], w: u32, x: i32, y: i32) -> [f32; 3] {
    let i = (y as usize * w as usize + x as usize) * 4;
    [rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32]
}

fn inside(w: u32, h: u32, x: f32, y: f32, margin: f32) -> bool {
    x - margin >= 0.0 && y - margin >= 0.0 && x + margin < w as f32 - 1.0 && y + margin < h as f32 - 1.0
}

/// Versatz zur Quelle, deren Umgebungsring dem des Ziels am ähnlichsten ist (Summe der
/// quadratischen Abweichungen nach Abzug des Helligkeitsunterschieds).
fn best_source(rgba: &[u8], w: u32, h: u32, cx: f32, cy: f32, r: f32) -> Option<(i32, i32)> {
    let pts = ring(r);
    if pts.is_empty() || !inside(w, h, cx, cy, 0.0) {
        return None;
    }
    let mut best: Option<(f32, (i32, i32))> = None;
    for k in 0..DIRECTIONS {
        let a = k as f32 / DIRECTIONS as f32 * std::f32::consts::TAU;
        for dist in DISTANCES {
            let (dx, dy) = ((a.cos() * dist * r).round() as i32, (a.sin() * dist * r).round() as i32);
            let (sx, sy) = (cx + dx as f32, cy + dy as f32);
            if !inside(w, h, sx, sy, r * RING.1 + 1.0) {
                continue;
            }
            let (mut mt, mut ms) = ([0f32; 3], [0f32; 3]);
            let mut pairs = Vec::with_capacity(pts.len());
            for &(ox, oy) in &pts {
                let (tx, ty) = (cx as i32 + ox, cy as i32 + oy);
                if !inside(w, h, tx as f32, ty as f32, 0.0) {
                    continue;
                }
                let (t, s) = (pixel(rgba, w, tx, ty), pixel(rgba, w, tx + dx, ty + dy));
                for c in 0..3 {
                    mt[c] += t[c];
                    ms[c] += s[c];
                }
                pairs.push((t, s));
            }
            if pairs.is_empty() {
                continue;
            }
            let n = pairs.len() as f32;
            let offset: [f32; 3] = std::array::from_fn(|c| (mt[c] - ms[c]) / n);
            let score = pairs
                .iter()
                .map(|(t, s)| (0..3).map(|c| (t[c] - s[c] - offset[c]).powi(2)).sum::<f32>())
                .sum::<f32>()
                / n;
            if best.is_none_or(|(b, _)| score < b) {
                best = Some((score, (dx, dy)));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// Ersetzt die Scheibe um (cx, cy) durch die Quelle (Versatz dx, dy), Helligkeit an den Ring
/// angeglichen, mit weichem Rand.
#[allow(clippy::too_many_arguments)]
fn fill(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, dx: i32, dy: i32) {
    let pts = ring(r);
    let (mut mt, mut ms, mut n) = ([0f32; 3], [0f32; 3], 0f32);
    for &(ox, oy) in &pts {
        let (tx, ty) = (cx as i32 + ox, cy as i32 + oy);
        if inside(w, h, tx as f32, ty as f32, 0.0) {
            let (t, s) = (pixel(rgba, w, tx, ty), pixel(rgba, w, tx + dx, ty + dy));
            for c in 0..3 {
                mt[c] += t[c];
                ms[c] += s[c];
            }
            n += 1.0;
        }
    }
    let offset: [f32; 3] = std::array::from_fn(|c| if n > 0.0 { (mt[c] - ms[c]) / n } else { 0.0 });
    let reach = (r * FEATHER).ceil() as i32;
    for y in (cy as i32 - reach)..=(cy as i32 + reach) {
        for x in (cx as i32 - reach)..=(cx as i32 + reach) {
            if !inside(w, h, x as f32, y as f32, 0.0) {
                continue;
            }
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() / r;
            if d >= FEATHER {
                continue;
            }
            let t = ((FEATHER - d) / (FEATHER - SOLID)).clamp(0.0, 1.0);
            let alpha = t * t * (3.0 - 2.0 * t);
            let s = pixel(rgba, w, x + dx, y + dy);
            let i = (y as usize * w as usize + x as usize) * 4;
            for c in 0..3 {
                let old = rgba[i + c] as f32;
                rgba[i + c] = (old + (s[c] + offset[c] - old) * alpha).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bright_particle_on_textured_water_disappears() {
        let (w, h) = (200u32, 150u32);
        // Wasser mit leichtem Verlauf und Muster, darin ein heller Partikel (Radius 5) bei (100, 75).
        let base = |x: u32, y: u32| 60.0 + y as f32 * 0.3 + ((x / 3 + y / 3) % 2) as f32 * 6.0;
        let mut px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let v = base(x, y) as u8;
                [v / 2, v, v + 40, 255]
            })
            .collect();
        let clean = px.clone();
        for y in 70..=80u32 {
            for x in 95..=105u32 {
                if (x as i32 - 100).pow(2) + (y as i32 - 75).pow(2) <= 25 {
                    let i = ((y * w + x) * 4) as usize;
                    px[i..i + 3].copy_from_slice(&[240, 250, 255]);
                }
            }
        }
        let spot = Spot { x: 100.5 / w as f32, y: 75.5 / h as f32, radius: 6.0 / w as f32 };
        heal(&mut px, w, h, &[spot]);
        let i = ((75 * w + 100) * 4) as usize;
        for c in 0..3 {
            let diff = (px[i + c] as i32 - clean[i + c] as i32).abs();
            assert!(diff < 20, "Kanal {c}: {} statt etwa {}", px[i + c], clean[i + c]);
        }
        let far = ((10 * w + 10) * 4) as usize;
        assert_eq!(px[far..far + 4], clean[far..far + 4], "weit weg unverändert");
    }

    #[test]
    fn spots_at_the_border_or_without_room_are_skipped() {
        let mut px = vec![100u8; 20 * 20 * 4];
        let before = px.clone();
        heal(&mut px, 20, 20, &[Spot { x: 0.5, y: 0.5, radius: 0.4 }]);
        assert_eq!(px, before, "kein Platz für eine Quelle");
    }
}
