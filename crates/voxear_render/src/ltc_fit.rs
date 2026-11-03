//! Fits the linearly transformed cosines that area lights shade with (see
//! [`crate::ltc`]), as Heitz, Dupuy, Hill and Neubelt did for "Real-Time
//! Polygonal-Light Shading with Linearly Transformed Cosines" (2016).
//!
//! For each roughness and view angle, a 3×3 matrix bends a clamped cosine
//! into the shape of the GGX lobe times the cosine, found by Nelder-Mead
//! from the fit next to it. The tables ship as `data/ltc.bin`, made by
//! `cargo test -p voxear_render --lib ltc_fit -- --ignored`, so nothing is
//! fitted when the engine builds.

use std::f64::consts::PI;

use voxear_core::math::{DMat3, DVec3};

use crate::ltc::{LTC_SIZE, LtcCell};

/// Samples each way when an error or an average is estimated.
const SAMPLES: usize = 32;
/// The smoothest GGX lobe fitted, for a roughness of 0.
const MIN_ALPHA: f64 = 0.00001;

/// GGX with height-correlated Smith masking, times the cosine: what the LTC
/// stands in for. `view` and `light` are in the frame whose Z is the normal.
/// Gives the value and the density of [`ggx_sample`] picking `light`.
pub(crate) fn ggx_eval(view: DVec3, light: DVec3, alpha: f64) -> (f64, f64) {
    if view.z <= 0.0 {
        return (0.0, 0.0);
    }
    let lambda_view = lambda(alpha, view.z);
    let shadowing = if light.z <= 0.0 { 0.0 } else { 1.0 / (1.0 + lambda_view + lambda(alpha, light.z)) };
    let half = (view + light).normalize();
    let slope = (half.x / half.z).powi(2) + (half.y / half.z).powi(2);
    let d = 1.0 / (1.0 + slope / (alpha * alpha));
    let d = d * d / (PI * alpha * alpha * half.z.powi(4));
    let pdf = (d * half.z / 4.0 / view.dot(half)).abs();
    (d * shadowing / 4.0 / view.z, pdf)
}

fn lambda(alpha: f64, cos_theta: f64) -> f64 {
    if cos_theta >= 1.0 {
        return 0.0;
    }
    let a = 1.0 / alpha / cos_theta.acos().tan();
    0.5 * (-1.0 + (1.0 + 1.0 / (a * a)).sqrt())
}

/// A light direction picked in proportion to GGX's distribution of normals.
pub(crate) fn ggx_sample(view: DVec3, alpha: f64, u1: f64, u2: f64) -> DVec3 {
    let phi = 2.0 * PI * u1;
    let r = alpha * (u2 / (1.0 - u2)).sqrt();
    let normal = DVec3::new(r * phi.cos(), r * phi.sin(), 1.0).normalize();
    -view + 2.0 * normal * normal.dot(view)
}

/// A cosine lobe bent by `m`, scaled to integrate to `magnitude`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ltc {
    pub m11: f64,
    pub m22: f64,
    pub m13: f64,
    pub magnitude: f64,
    pub fresnel: f64,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
    pub m: DMat3,
    pub inverse: DMat3,
    pub determinant: f64,
}

impl Ltc {
    fn new() -> Self {
        let mut ltc = Self {
            m11: 1.0,
            m22: 1.0,
            m13: 0.0,
            magnitude: 1.0,
            fresnel: 1.0,
            x: DVec3::X,
            y: DVec3::Y,
            z: DVec3::Z,
            m: DMat3::IDENTITY,
            inverse: DMat3::IDENTITY,
            determinant: 1.0,
        };
        ltc.update();
        ltc
    }

    /// Its matrix from the frame and the three fitted values.
    fn update(&mut self) {
        let scale = DMat3::from_cols(
            DVec3::new(self.m11, 0.0, 0.0),
            DVec3::new(0.0, self.m22, 0.0),
            DVec3::new(self.m13, 0.0, 1.0),
        );
        self.m = DMat3::from_cols(self.x, self.y, self.z) * scale;
        self.inverse = self.m.inverse();
        self.determinant = self.m.determinant().abs();
    }

    /// An LTC from a matrix as the tables keep it.
    pub(crate) fn from_inverse(inverse: DMat3, magnitude: f64) -> Self {
        let m = inverse.inverse();
        Self { m, inverse, determinant: m.determinant().abs(), magnitude, ..Self::new() }
    }

    pub(crate) fn eval(&self, light: DVec3) -> f64 {
        let original = (self.inverse * light).normalize();
        let bent = self.m * original;
        let length = bent.length();
        let jacobian = self.determinant / (length * length * length);
        self.magnitude * original.z.max(0.0) / PI / jacobian
    }

    pub(crate) fn sample(&self, u1: f64, u2: f64) -> DVec3 {
        let theta = u1.sqrt().acos();
        let phi = 2.0 * PI * u2;
        let original = DVec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos());
        (self.m * original).normalize()
    }
}

/// How far an LTC is from GGX, by samples of both with multiple
/// importance sampling, as the paper's fit measures it.
pub(crate) fn fit_error(ltc: &Ltc, view: DVec3, alpha: f64) -> f64 {
    let mut error = 0.0;
    let mut add = |light: DVec3| {
        let (brdf, brdf_pdf) = ggx_eval(view, light, alpha);
        let lobe = ltc.eval(light);
        let lobe_pdf = lobe / ltc.magnitude;
        let difference = (brdf - lobe).abs().powi(3);
        error += difference / (lobe_pdf + brdf_pdf);
    };
    for j in 0..SAMPLES {
        for i in 0..SAMPLES {
            let u1 = (i as f64 + 0.5) / SAMPLES as f64;
            let u2 = (j as f64 + 0.5) / SAMPLES as f64;
            add(ltc.sample(u1, u2));
            add(ggx_sample(view, alpha, u1, u2));
        }
    }
    error / (SAMPLES * SAMPLES) as f64
}

/// GGX's directional albedo, the share of it Schlick's Fresnel weighs by
/// (1 - v·h)^5, and its average light direction, flattened into the plane
/// of the view.
pub(crate) fn average_terms(view: DVec3, alpha: f64) -> (f64, f64, DVec3) {
    let (mut norm, mut fresnel, mut direction) = (0.0, 0.0, DVec3::ZERO);
    for j in 0..SAMPLES {
        for i in 0..SAMPLES {
            let u1 = (i as f64 + 0.5) / SAMPLES as f64;
            let u2 = (j as f64 + 0.5) / SAMPLES as f64;
            let light = ggx_sample(view, alpha, u1, u2);
            let (value, pdf) = ggx_eval(view, light, alpha);
            if pdf > 0.0 {
                let weight = value / pdf;
                let half = (view + light).normalize();
                norm += weight;
                fresnel += weight * (1.0 - view.dot(half).max(0.0)).powi(5);
                direction += weight * light;
            }
        }
    }
    let count = (SAMPLES * SAMPLES) as f64;
    direction.y = 0.0;
    (norm / count, fresnel / count, direction.normalize())
}

/// The view at the table's column `t`, which is spaced by sqrt(1 - cos θ),
/// and the GGX alpha of its row `a`, roughness squared.
pub(crate) fn cell_view_and_alpha(a: usize, t: usize) -> (DVec3, f64) {
    let x = t as f64 / (LTC_SIZE - 1) as f64;
    let theta = (1.0 - x * x).acos().min(1.57);
    let roughness = a as f64 / (LTC_SIZE - 1) as f64;
    (DVec3::new(theta.sin(), 0.0, theta.cos()), (roughness * roughness).max(MIN_ALPHA))
}

/// Fits one cell, starting from `start`'s values: the fit at the next
/// rougher row for the head-on column, which is aligned with the normal,
/// and the fit at the column before otherwise.
pub(crate) fn fit_cell(a: usize, t: usize, start: &Ltc) -> Ltc {
    let (view, alpha) = cell_view_and_alpha(a, t);
    let (magnitude, fresnel, average) = average_terms(view, alpha);
    let mut ltc = Ltc { magnitude, fresnel, ..*start };
    let isotropic = t == 0;
    if isotropic {
        (ltc.x, ltc.y, ltc.z) = (DVec3::X, DVec3::Y, DVec3::Z);
        ltc.m13 = 0.0;
    } else {
        (ltc.x, ltc.y, ltc.z) = (DVec3::new(average.z, 0.0, -average.x), DVec3::Y, average);
    }
    ltc.update();
    let set = |ltc: &mut Ltc, params: [f64; 3]| {
        let m11 = params[0].max(1e-7);
        let m22 = params[1].max(1e-7);
        (ltc.m11, ltc.m22, ltc.m13) = if isotropic { (m11, m11, 0.0) } else { (m11, m22, params[2]) };
        ltc.update();
    };
    let objective = |params: [f64; 3]| {
        let mut trial = ltc;
        set(&mut trial, params);
        fit_error(&trial, view, alpha)
    };
    let best = nelder_mead([ltc.m11, ltc.m22, ltc.m13], 0.05, 1e-5, 100, objective);
    set(&mut ltc, best);
    ltc
}

/// The table's cell as the shader reads it: the inverse matrix, with the
/// terms that stay zero for an isotropic lobe left out and scaled so its
/// middle term is one, then the magnitude and Fresnel share.
pub(crate) fn cell_of(ltc: &Ltc) -> LtcCell {
    let mut m = ltc.m;
    // Terms that only ever hold rounding error.
    m.x_axis.y = 0.0;
    m.y_axis.x = 0.0;
    m.z_axis.y = 0.0;
    m.y_axis.z = 0.0;
    let inverse = m.inverse();
    let inverse = inverse * (1.0 / inverse.y_axis.y);
    LtcCell {
        inverse: [inverse.x_axis.x, inverse.x_axis.z, inverse.z_axis.x, inverse.z_axis.z].map(|v| v as f32),
        magnitude: ltc.magnitude as f32,
        fresnel: ltc.fresnel as f32,
    }
}

/// Fits every cell: the head-on column first, from rough to smooth, as each
/// starts from the one rougher; then each row across the view angles, the
/// rows side by side on as many threads as there are cores.
pub(crate) fn fit_table() -> Vec<LtcCell> {
    let n = LTC_SIZE;
    let mut heads = vec![Ltc::new(); n];
    let mut start = Ltc::new();
    for a in (0..n).rev() {
        heads[a] = fit_cell(a, 0, &start);
        start = Ltc { m11: heads[a].m11, m22: heads[a].m22, ..Ltc::new() };
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let rows: Vec<Vec<Ltc>> = std::thread::scope(|scope| {
        let chunks: Vec<_> = (0..threads)
            .map(|thread| {
                let heads = &heads;
                scope.spawn(move || {
                    (thread..n)
                        .step_by(threads)
                        .map(|a| {
                            let mut row = vec![heads[a]];
                            for t in 1..n {
                                let next = fit_cell(a, t, &row[t - 1]);
                                row.push(next);
                            }
                            (a, row)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut rows = vec![Vec::new(); n];
        for chunk in chunks {
            for (a, row) in chunk.join().expect("a fitting thread panicked") {
                rows[a] = row;
            }
        }
        rows
    });
    // Rows are roughness and columns the view angle, as the texture lays them out.
    let mut cells = Vec::with_capacity(n * n);
    for t in 0..n {
        for row in &rows {
            cells.push(cell_of(&row[t]));
        }
    }
    cells
}

/// Nelder-Mead's downhill simplex, with its usual coefficients: the
/// lowest point it finds from `start`, with first steps of `delta`.
fn nelder_mead(
    start: [f64; 3],
    delta: f64,
    tolerance: f64,
    iterations: usize,
    objective: impl Fn([f64; 3]) -> f64,
) -> [f64; 3] {
    const POINTS: usize = 4;
    let mut s = [start; POINTS];
    for (i, point) in s.iter_mut().enumerate().skip(1) {
        point[i - 1] += delta;
    }
    let mut f = s.map(&objective);
    let mut lo = 0;
    for _ in 0..iterations {
        lo = 0;
        let (mut hi, mut nh) = (0, 0);
        for i in 1..POINTS {
            if f[i] < f[lo] {
                lo = i;
            }
            if f[i] > f[hi] {
                nh = hi;
                hi = i;
            } else if f[i] > f[nh] {
                nh = i;
            }
        }
        let (a, b) = (f[lo].abs(), f[hi].abs());
        if 2.0 * (a - b).abs() < (a + b) * tolerance {
            break;
        }
        let mut centroid = [0.0; 3];
        for (_, point) in s.iter().enumerate().filter(|(i, _)| *i != hi) {
            for d in 0..3 {
                centroid[d] += point[d] / 3.0;
            }
        }
        let towards = |scale: f64| std::array::from_fn(|d| centroid[d] + scale * (centroid[d] - s[hi][d]));
        let reflected: [f64; 3] = towards(1.0);
        let fr = objective(reflected);
        if fr < f[nh] {
            if fr < f[lo] {
                let expanded: [f64; 3] = towards(2.0);
                let fe = objective(expanded);
                if fe < fr {
                    (s[hi], f[hi]) = (expanded, fe);
                    continue;
                }
            }
            (s[hi], f[hi]) = (reflected, fr);
            continue;
        }
        let contracted: [f64; 3] = towards(-0.5);
        let fc = objective(contracted);
        if fc < f[hi] {
            (s[hi], f[hi]) = (contracted, fc);
            continue;
        }
        let best = s[lo];
        for k in (0..POINTS).filter(|&k| k != lo) {
            s[k] = std::array::from_fn(|d| best[d] + 0.5 * (s[k][d] - best[d]));
            f[k] = objective(s[k]);
        }
    }
    s[lo]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltc::{cells, encode};

    /// The inverse matrix a shipped cell holds, as the shader builds it.
    fn inverse_of(cell: &LtcCell) -> DMat3 {
        let [a, b, c, d] = cell.inverse.map(f64::from);
        DMat3::from_cols(DVec3::new(a, 0.0, b), DVec3::Y, DVec3::new(c, 0.0, d))
    }

    /// The shipped cell at row `a` and column `t`, back in the frame its
    /// fit was made in: the normal's, with the view in the XZ plane.
    fn shipped(a: usize, t: usize) -> Ltc {
        let cell = &cells()[a + t * LTC_SIZE];
        Ltc::from_inverse(inverse_of(cell), f64::from(cell.magnitude))
    }

    /// How much of the lobe the LTC gets wrong, as a share of all of it:
    /// half the L1 distance between the two, over the hemisphere.
    fn mismatch(ltc: &Ltc, view: DVec3, alpha: f64) -> f64 {
        let (mut wrong, mut total) = (0.0, 0.0);
        let n = 64;
        for j in 0..n {
            for i in 0..n {
                let u1 = (i as f64 + 0.5) / n as f64;
                let u2 = (j as f64 + 0.5) / n as f64;
                // Both distributions' samples, each weighed by the balance heuristic.
                for light in [ltc.sample(u1, u2), ggx_sample(view, alpha, u1, u2)] {
                    let (brdf, brdf_pdf) = ggx_eval(view, light, alpha);
                    let lobe = ltc.eval(light);
                    let pdf = lobe / ltc.magnitude + brdf_pdf;
                    if pdf > 0.0 {
                        wrong += (brdf - lobe).abs() / pdf;
                        total += brdf / pdf;
                    }
                }
            }
        }
        0.5 * wrong / total
    }

    #[test]
    fn the_tables_fit_ggx() {
        // Rough, middling and smooth, head-on and grazing. LTCs are known to fit worst at grazing angles.
        for (a, t, most) in [(63, 0, 0.03), (40, 20, 0.08), (40, 50, 0.25), (20, 10, 0.03), (63, 60, 0.12)] {
            let (view, alpha) = cell_view_and_alpha(a, t);
            let error = mismatch(&shipped(a, t), view, alpha);
            assert!(error < most, "cell ({a}, {t}) is {:.1}% off GGX", error * 100.0);
        }
    }

    #[test]
    fn the_tables_magnitude_is_ggxs_albedo() {
        // Integrated afresh over a grid of the hemisphere, not by the fit's own sampling.
        for (a, t) in [(63, 0), (32, 32), (10, 5), (50, 62)] {
            let (view, alpha) = cell_view_and_alpha(a, t);
            let n = 400;
            let mut albedo = 0.0;
            for j in 0..n {
                for i in 0..n {
                    let cos = (i as f64 + 0.5) / n as f64;
                    let phi = 2.0 * PI * (j as f64 + 0.5) / n as f64;
                    let sin = (1.0 - cos * cos).sqrt();
                    let light = DVec3::new(sin * phi.cos(), sin * phi.sin(), cos);
                    albedo += ggx_eval(view, light, alpha).0 * 2.0 * PI / (n * n) as f64;
                }
            }
            let magnitude = f64::from(cells()[a + t * LTC_SIZE].magnitude);
            // Smooth lobes are too narrow for the grid; only the rougher are compared closely.
            let tolerance = if alpha > 0.05 { 0.03 } else { 0.15 };
            assert!((magnitude - albedo).abs() < tolerance, "cell ({a}, {t}): {magnitude} against {albedo}");
        }
    }

    #[test]
    fn a_rough_lobe_seen_head_on_is_nearly_a_cosine() {
        let inverse = inverse_of(&cells()[LTC_SIZE - 1]);
        let ratio = inverse.z_axis.z / inverse.x_axis.x;
        assert!((0.8..1.25).contains(&ratio), "{inverse:?}");
        assert!(inverse.x_axis.z.abs() < 1e-3 && inverse.z_axis.x.abs() < 1e-3, "{inverse:?}");
    }

    #[test]
    fn fitting_a_cell_again_finds_the_shipped_fit() {
        // Row 40, column 20, from the shipped fit beside it.
        let (a, t) = (40, 20);
        let before = shipped(a, t - 1);
        let (view, alpha) = cell_view_and_alpha(a, t);
        let refit = fit_cell(a, t, &Ltc { m11: 0.6, m22: 0.6, m13: 0.0, ..before });
        let error = mismatch(&refit, view, alpha);
        let shipped_error = mismatch(&shipped(a, t), view, alpha);
        assert!(error < shipped_error * 1.5 + 0.01, "a fresh fit is {error}, the shipped one {shipped_error}");
        assert!((refit.magnitude - f64::from(cells()[a + t * LTC_SIZE].magnitude)).abs() < 1e-4);
    }

    #[test]
    fn every_value_fits_a_half_float() {
        for cell in cells() {
            for value in cell.inverse.into_iter().chain([cell.magnitude, cell.fresnel]) {
                assert!(value.is_finite() && value.abs() < 60_000.0, "{cell:?}");
            }
        }
        let (matrix, amplitude) = encode();
        assert_eq!(matrix.len(), LTC_SIZE * LTC_SIZE * 4);
        assert_eq!(amplitude.len(), LTC_SIZE * LTC_SIZE * 4);
    }

    /// Writes `data/ltc.bin` afresh. Slow, so only on request:
    /// `cargo test -p voxear_render --lib ltc_fit -- --ignored`.
    #[test]
    #[ignore]
    fn fit_the_ltc_tables() {
        let cells = fit_table();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/ltc.bin");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, crate::ltc::to_bytes(&cells)).unwrap();
    }
}
