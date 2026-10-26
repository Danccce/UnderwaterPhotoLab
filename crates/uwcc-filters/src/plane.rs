//! Ein Kanal als `f32`-Ebene und Filter mit Nachbarschaft: Box-, Gauß- und Guided Filter.
//! Alle laufen in O(Pixel) unabhängig vom Radius und auf alle Kerne verteilt; am Rand wird das
//! letzte Pixel wiederholt.

use std::thread;

#[derive(Clone, Debug, PartialEq)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Plane {
    pub fn new(width: usize, height: usize, value: f32) -> Self {
        Self { width, height, data: vec![value; width * height] }
    }

    pub fn from_fn(width: usize, height: usize, f: impl Fn(usize, usize) -> f32) -> Self {
        let data = (0..height).flat_map(|y| (0..width).map(move |x| (x, y))).map(|(x, y)| f(x, y)).collect();
        Self { width, height, data }
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    /// Wendet `f` elementweise auf zwei gleich große Ebenen an.
    pub fn zip(&self, other: &Plane, f: impl Fn(f32, f32) -> f32) -> Plane {
        debug_assert_eq!((self.width, self.height), (other.width, other.height));
        let data = self.data.iter().zip(&other.data).map(|(&a, &b)| f(a, b)).collect();
        Plane { width: self.width, height: self.height, data }
    }

    pub fn map(&self, f: impl Fn(f32) -> f32) -> Plane {
        Plane { width: self.width, height: self.height, data: self.data.iter().map(|&v| f(v)).collect() }
    }
}

fn threads() -> usize {
    thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8)
}

/// Gleitender Mittelwert über `2r + 1` Werte von `src` (Abstand `stride`), Rand wiederholt.
fn running_mean(src: &[f32], len: usize, stride: usize, r: usize, out: &mut impl FnMut(usize, f32)) {
    let at = |i: isize| src[i.clamp(0, len as isize - 1) as usize * stride];
    let r = r as isize;
    let mut sum: f32 = (-r..=r).map(at).sum();
    let n = (2 * r + 1) as f32;
    for i in 0..len as isize {
        out(i as usize, sum / n);
        sum += at(i + r + 1) - at(i - r);
    }
}

/// Box-Filter mit Radius `r` (Fenster `2r + 1`), getrennt in Zeilen und Spalten.
pub fn box_blur(p: &Plane, r: usize) -> Plane {
    if r == 0 || p.data.is_empty() {
        return p.clone();
    }
    let (w, h) = (p.width, p.height);
    let mut rows = vec![0f32; w * h];
    let per = h.div_ceil(threads()).max(1);
    thread::scope(|s| {
        for (chunk_index, chunk) in rows.chunks_mut(per * w).enumerate() {
            s.spawn(move || {
                for (dy, row) in chunk.chunks_mut(w).enumerate() {
                    let y = chunk_index * per + dy;
                    running_mean(&p.data[y * w..(y + 1) * w], w, 1, r, &mut |x, v| row[x] = v);
                }
            });
        }
    });

    let mut out = vec![0f32; w * h];
    let cols = w.div_ceil(threads()).max(1);
    let rows = &rows;
    let out_ptr = SharedOut(out.as_mut_ptr());
    thread::scope(|s| {
        for start in (0..w).step_by(cols) {
            let out_ptr = &out_ptr;
            s.spawn(move || {
                for x in start..(start + cols).min(w) {
                    // SAFETY: jeder Thread schreibt nur seine eigenen Spalten `start..start + cols`.
                    running_mean(&rows[x..], h, w, r, &mut |y, v| unsafe { *out_ptr.0.add(y * w + x) = v });
                }
            });
        }
    });
    Plane { width: w, height: h, data: out }
}

/// Zeiger für spaltenweises paralleles Schreiben; die Spaltenbereiche überlappen nicht.
struct SharedOut(*mut f32);
unsafe impl Sync for SharedOut {}

/// Gauß-Näherung über drei Box-Filter (Fehler unter 3 %).
pub fn gaussian(p: &Plane, sigma: f32) -> Plane {
    if sigma < 0.5 {
        return p.clone();
    }
    let r = (((12.0 * sigma * sigma / 3.0 + 1.0).sqrt() - 1.0) / 2.0).round().max(1.0) as usize;
    box_blur(&box_blur(&box_blur(p, r), r), r)
}

/// Guided Filter (He et al.): glättet `src`, erhält aber Kanten, die in `guide` stehen.
/// `eps` steuert, ab welchem Kontrast (Varianz) eine Kante erhalten bleibt.
pub fn guided_filter(guide: &Plane, src: &Plane, r: usize, eps: f32) -> Plane {
    let mean_i = box_blur(guide, r);
    let mean_p = box_blur(src, r);
    let corr_ii = box_blur(&guide.zip(guide, |a, b| a * b), r);
    let corr_ip = box_blur(&guide.zip(src, |a, b| a * b), r);
    let var_i = corr_ii.zip(&mean_i, |c, m| c - m * m);
    let cov_ip = Plane {
        width: src.width,
        height: src.height,
        data: (0..src.data.len()).map(|i| corr_ip.data[i] - mean_i.data[i] * mean_p.data[i]).collect(),
    };
    let a = cov_ip.zip(&var_i, |c, v| c / (v + eps));
    let b = Plane {
        width: src.width,
        height: src.height,
        data: (0..src.data.len()).map(|i| mean_p.data[i] - a.data[i] * mean_i.data[i]).collect(),
    };
    let mean_a = box_blur(&a, r);
    let mean_b = box_blur(&b, r);
    Plane {
        width: src.width,
        height: src.height,
        data: (0..src.data.len()).map(|i| mean_a.data[i] * guide.data[i] + mean_b.data[i]).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn box_blur_keeps_constants_and_averages() {
        let c = Plane::new(37, 23, 0.4);
        assert!(box_blur(&c, 5).data.iter().all(|&v| close(v, 0.4)));

        let mut impulse = Plane::new(31, 31, 0.0);
        impulse.data[15 * 31 + 15] = 1.0;
        let b = box_blur(&impulse, 2);
        assert!(close(b.get(15, 15), 1.0 / 25.0));
        assert!(close(b.get(17, 17), 1.0 / 25.0));
        assert!(close(b.get(18, 15), 0.0));
        assert!(close(b.data.iter().sum::<f32>(), 1.0), "Energie bleibt erhalten");
    }

    #[test]
    fn box_blur_matches_a_naive_version_at_the_edges() {
        let p = Plane::from_fn(9, 7, |x, y| (x * 3 + y * 5) as f32 % 7.0);
        let r = 2;
        let fast = box_blur(&p, r);
        let naive = Plane::from_fn(9, 7, |x, y| {
            let mut sum = 0.0;
            for dy in -(r as isize)..=r as isize {
                for dx in -(r as isize)..=r as isize {
                    let xx = (x as isize + dx).clamp(0, 8) as usize;
                    let yy = (y as isize + dy).clamp(0, 6) as usize;
                    sum += p.get(xx, yy);
                }
            }
            sum / 25.0
        });
        for (a, b) in fast.data.iter().zip(&naive.data) {
            assert!(close(*a, *b), "{a} != {b}");
        }
    }

    #[test]
    fn gaussian_is_symmetric_on_a_step() {
        let step = Plane::from_fn(64, 4, |x, _| if x < 32 { 0.0 } else { 1.0 });
        let g = gaussian(&step, 4.0);
        assert!(g.get(28, 1) > 0.0 && g.get(28, 1) < 0.5);
        assert!(close(g.get(31, 1) + g.get(32, 1), 1.0), "symmetrisch um die Kante");
        assert!(close(g.get(0, 1), 0.0) && close(g.get(63, 1), 1.0));
    }

    #[test]
    fn guided_filter_keeps_edges_and_smooths_flat_noise() {
        // Stufe mit leichtem Rauschen auf beiden Seiten.
        let noisy = Plane::from_fn(64, 64, |x, y| {
            let base = if x < 32 { 0.2 } else { 0.8 };
            base + if (x + y) % 2 == 0 { 0.02 } else { -0.02 }
        });
        let g = guided_filter(&noisy, &noisy, 4, 0.001);
        assert!(g.get(30, 30) < 0.3 && g.get(33, 30) > 0.7, "Kante bleibt");
        assert!((g.get(10, 10) - 0.2).abs() < 0.01 && (g.get(50, 10) - 0.8).abs() < 0.01, "Rauschen geglättet");

        let blurred = box_blur(&noisy, 4);
        assert!(blurred.get(30, 30) > 0.3, "zum Vergleich: der Box-Filter verwischt die Kante");
    }
}
