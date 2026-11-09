//! Noise for terrain: gradient noise in two dimensions, and the fractal
//! kinds layered from it. Built from integer hashing, additions,
//! multiplications and `floor` alone, so the same seed gives the same
//! ground on every machine.

/// A hash of a lattice point and a seed, well mixed.
fn hash(x: i32, z: i32, seed: u32) -> u32 {
    let mut h =
        seed.wrapping_mul(0x9E37_79B9) ^ (x as u32).wrapping_mul(0x85EB_CA6B) ^ (z as u32).wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// Eight directions, evenly round, for the lattice's gradients.
const GRADIENTS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.707_106_77, 0.707_106_77],
    [0.0, 1.0],
    [-0.707_106_77, 0.707_106_77],
    [-1.0, 0.0],
    [-0.707_106_77, -0.707_106_77],
    [0.0, -1.0],
    [0.707_106_77, -0.707_106_77],
];

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Gradient noise at `(x, z)`, about -1 to 1, changing over a unit.
pub fn gradient(x: f32, z: f32, seed: u32) -> f32 {
    let (fx, fz) = (x.floor(), z.floor());
    let (ix, iz) = (fx as i32, fz as i32);
    let (tx, tz) = (x - fx, z - fz);
    let corner = |dx: i32, dz: i32| {
        let [gx, gz] = GRADIENTS[(hash(ix + dx, iz + dz, seed) & 7) as usize];
        gx * (tx - dx as f32) + gz * (tz - dz as f32)
    };
    let (u, v) = (fade(tx), fade(tz));
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * u;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * u;
    (top + (bottom - top) * v) * std::f32::consts::SQRT_2
}

/// The kinds of fractal noise a terrain layer makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoiseStyle {
    /// Rolling ground: octaves of noise added, each finer and fainter.
    #[default]
    Fractal,
    /// Sharp ridges with soft valleys between, for mountain ranges.
    Ridged,
    /// Rounded heaps with creases between, for hummocks and dunes.
    Billow,
}

impl NoiseStyle {
    pub const ALL: [NoiseStyle; 3] = [NoiseStyle::Fractal, NoiseStyle::Ridged, NoiseStyle::Billow];

    pub fn name(self) -> &'static str {
        match self {
            NoiseStyle::Fractal => "Fractal",
            NoiseStyle::Ridged => "Ridged",
            NoiseStyle::Billow => "Billow",
        }
    }
}

/// Fractal noise of `style` at `(x, z)`, 0 to 1, with `octaves` octaves,
/// each twice as fine and half as strong as the one before.
pub fn fractal(style: NoiseStyle, x: f32, z: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut total, mut amplitude, mut frequency) = (0.0, 0.0, 1.0, 1.0);
    // Ridges are sharpened by the octave before, so they stand out where it is high.
    let mut weight = 1.0f32;
    for octave in 0..octaves.clamp(1, 12) {
        let n = gradient(x * frequency, z * frequency, seed.wrapping_add(octave.wrapping_mul(0x632B_E5AB)));
        let value = match style {
            NoiseStyle::Fractal => n * 0.5 + 0.5,
            NoiseStyle::Billow => n.abs(),
            NoiseStyle::Ridged => {
                let ridge = 1.0 - n.abs();
                let ridge = ridge * ridge * weight;
                weight = (ridge * 2.0).clamp(0.0, 1.0);
                ridge
            }
        };
        sum += value * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (sum / total).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_the_same_for_the_same_seed_and_differs_for_another() {
        let a = fractal(NoiseStyle::Fractal, 12.3, 45.6, 6, 7);
        assert_eq!(a, fractal(NoiseStyle::Fractal, 12.3, 45.6, 6, 7));
        assert_ne!(a, fractal(NoiseStyle::Fractal, 12.3, 45.6, 6, 8));
        // Zero on every lattice point, smooth between.
        assert_eq!(gradient(3.0, -4.0, 1), 0.0);
        let near = (gradient(3.5, 2.5, 1) - gradient(3.501, 2.5, 1)).abs();
        assert!(near < 0.01, "{near}");
    }

    #[test]
    fn every_style_stays_between_nothing_and_one() {
        for style in NoiseStyle::ALL {
            for i in 0..2000 {
                let (x, z) = (i as f32 * 0.137, i as f32 * 0.071 - 40.0);
                let v = fractal(style, x, z, 5, 3);
                assert!((0.0..=1.0).contains(&v), "{style:?} {v}");
            }
        }
    }
}
