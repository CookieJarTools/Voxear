//! The sea: an Ocean component on an Obj fills the level with water at its
//! height, with waves made by an FFT over a spectrum of real ocean waves.
//!
//! The spectrum is two JONSWAP spectra together: a long swell from a storm
//! far off, and a shorter sea raised by the wind here, each limited by how
//! far the wind has blown over open water. It is split into six cascades,
//! tiles from 1031 metres down to 1.13 that never line up, each holding only
//! its own band of wavelengths.
//!
//! Each wave's random height comes from a hash of where it is in the
//! spectrum, not from a sequence, so the GPU's 256×256 cascades and the
//! CPU's 64×64 copy of the two longest (every wave longer than about ten
//! metres, which is all a floating boat feels) hold the same waves.
//! [`OceanSampler`] answers how high the sea is anywhere, at any time, the
//! same on every machine, without reading anything back from the GPU.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Component;

/// How many cascades of waves the sea has, from the longest.
pub const CASCADES: usize = 6;
/// Each cascade's tile, in metres: coprime, so their repeats never line up.
/// The last two hold the ripples, down to a centimetre, so the water is
/// crisp however near it is seen.
pub const CASCADE_SIZES: [f32; CASCADES] = [1031.0, 257.0, 61.0, 17.0, 4.3, 1.13];
/// The GPU's cascades are this many waves across.
pub const GPU_RESOLUTION: usize = 256;
/// The CPU's copy, of the two longest cascades, for height queries.
pub const QUERY_RESOLUTION: usize = 64;
pub const QUERY_CASCADES: usize = 2;
/// The sea repeats exactly after this many seconds: each wave's frequency is
/// rounded to a whole number of turns in it, so the GPU's time can wrap
/// round without a jump.
pub const REPEAT_SECONDS: f64 = 1000.0;
const GRAVITY: f64 = 9.81;

/// What the weather makes of the sea.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weather {
    /// A long, low swell under a light breeze.
    Calm,
    /// A steady swell and a fresh breeze, with some whitecaps.
    #[default]
    Normal,
    /// A heavy swell from a storm, metres high, under a strong wind.
    Stormy,
    /// The swell and wind as the Ocean's own settings say.
    Custom,
}

impl Weather {
    pub const ALL: [Self; 4] = [Self::Calm, Self::Normal, Self::Stormy, Self::Custom];

    /// How much more foam it whips up than the Ocean's Foam says: real seas
    /// fold about alike whatever their size, so a storm's whitecaps, as Sea
    /// of Thieves has them, come from the weather.
    pub fn foam(self) -> f32 {
        match self {
            Self::Calm => 0.2,
            Self::Normal | Self::Custom => 1.0,
            Self::Stormy => 2.0,
        }
    }

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Normal => "Normal",
            Self::Stormy => "Stormy",
            Self::Custom => "Custom",
        }
    }
}

/// The sea, filling the level at its Obj's height, its wind blowing along its
/// Obj's +Z. The first one in the level is used.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ocean {
    pub weather: Weather,
    /// For Custom weather: the wind that raised the swell, far off, in metres
    /// a second, and how far it blew over open water, in kilometres.
    pub swell_wind: f32,
    pub swell_fetch: f32,
    /// For Custom weather: the wind here, in metres a second, and how far it
    /// has blown over open water, in kilometres.
    pub wind: f32,
    pub fetch: f32,
    /// How sharp the crests are: 0 for round waves, 1 as real ones.
    pub choppiness: f32,
    /// How much higher the waves are than real ones: 1 as the weather makes
    /// them, 2 twice as high. Their crests fold no
    /// harder, so they break into no more foam.
    pub wave_scale: f32,
    /// The colour of deep water, sRGB.
    pub deep: [f32; 3],
    /// The colour light takes through the thin tops of the waves, sRGB.
    pub scatter: [f32; 3],
    /// How much foam the breaking crests have, from 0 for none.
    pub foam: f32,
    /// What a metre of water lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
}

impl Component for Ocean {}

impl Default for Ocean {
    fn default() -> Self {
        Self {
            weather: Weather::Normal,
            swell_wind: 9.0,
            swell_fetch: 150.0,
            wind: 7.0,
            fetch: 20.0,
            choppiness: 0.9,
            wave_scale: 1.0,
            // Deep, saturated blue, turquoise through the crests.
            deep: [0.0, 0.16, 0.32],
            scatter: [0.1, 0.78, 0.72],
            foam: 0.5,
            clarity: [0.82, 0.97, 0.98],
            murk: 1.0,
        }
    }
}

/// One JONSWAP spectrum: waves raised by a wind of `speed` metres a second
/// over `fetch` metres of open water, spread round its direction as
/// `cos^2s` of half the angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waves {
    pub speed: f64,
    pub fetch: f64,
    /// JONSWAP's peak enhancement: 3.3 for a sea still growing, 1 for one
    /// fully grown.
    pub gamma: f64,
    /// How narrowly they follow the wind: 4 for a wind sea, 20 for a swell.
    pub spread: f64,
}

/// The waves the sea is made of: a swell and a wind sea, their direction
/// across the ground, and how sharp their crests are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaState {
    pub swell: Waves,
    pub wind: Waves,
    /// Unit, across the ground (x, z).
    pub direction: [f64; 2],
    pub choppiness: f32,
    /// How much higher than the spectrum makes them every wave is.
    pub scale: f64,
}

impl Ocean {
    /// The sea its weather makes, its wind along `rotation`'s +Z.
    pub fn sea_state(&self, rotation: Quat) -> SeaState {
        let (swell, swell_fetch, wind, fetch) = match self.weather {
            // Significant wave heights of about 0.8, 2.5 and 6.5 metres, the swell's peak 24, 74 and 173 metres long;
            // a storm's wind sea steep and 3 metres high on top of its swell.
            Weather::Calm => (5.0, 50.0, 4.0, 10.0),
            Weather::Normal => (9.0, 150.0, 7.0, 20.0),
            Weather::Stormy => (16.0, 300.0, 16.0, 80.0),
            Weather::Custom => (self.swell_wind, self.swell_fetch, self.wind, self.fetch),
        };
        let along = rotation * Vec3::Z;
        let flat = Vec3::new(along.x, 0.0, along.z).normalize_or(Vec3::Z);
        let km = |fetch: f32| f64::from(fetch.max(0.1)) * 1000.0;
        let scale = self.wave_scale.clamp(0.0, 4.0);
        SeaState {
            swell: Waves { speed: f64::from(swell.max(0.1)), fetch: km(swell_fetch), gamma: 3.3, spread: 20.0 },
            wind: Waves { speed: f64::from(wind.max(0.1)), fetch: km(fetch), gamma: 3.3, spread: 4.0 },
            direction: [f64::from(flat.x), f64::from(flat.z)],
            // A storm's crests sharper still; taller waves pinched no further sideways, so they fold as before.
            choppiness: (self.choppiness * if self.weather == Weather::Stormy { 1.3 } else { 1.0 }).clamp(0.0, 1.5) / scale.max(1.0),
            scale: f64::from(scale),
        }
    }
}

/// How fast deep water carries a wave of wave number `k`, in radians a
/// second, rounded to a whole number of turns in [`REPEAT_SECONDS`]: by
/// gravity, and for ripples of a few centimetres by surface tension too,
/// which hurries them.
pub fn frequency(k: f64) -> f64 {
    let step = std::f64::consts::TAU / REPEAT_SECONDS;
    ((GRAVITY * k + SURFACE_TENSION * k * k * k).sqrt() / step).floor() * step
}

/// Water's surface tension over its density, in m³/s².
const SURFACE_TENSION: f64 = 7.4e-5;

/// Water deeper than this, in metres, is deep to every wave.
pub const SHALLOWS_DEPTH: f32 = 40.0;

/// How much of cascade `cascade`'s waves water `depth` metres deep keeps: a
/// wave feels the bottom once the water is shallower than half its length,
/// and dies away towards the shore, the long swell first. As `shallowWeight`
/// in `shaders/water.slang`.
pub fn shallow_weight(cascade: usize, depth: f32) -> f32 {
    let length = CASCADE_SIZES[cascade] * 0.1;
    let felt = (2.0 * depth.max(0.0) / length).min(1.0);
    let felt = felt + (1.0 - felt) * (depth / SHALLOWS_DEPTH).clamp(0.0, 1.0);
    1.0 - 0.9 * (1.0 - felt)
}

/// How much the waves move water sideways where it is `depth` metres deep:
/// not at all at the very edge, so they cannot draw it up the shore. As
/// `sidewaysWeight` in `shaders/water.slang`.
pub fn sideways_weight(depth: f32) -> f32 {
    depth.clamp(0.0, 1.0)
}

/// JONSWAP's energy at angular frequency `omega`.
fn jonswap(omega: f64, waves: &Waves) -> f64 {
    if omega <= 0.0 {
        return 0.0;
    }
    let g = GRAVITY;
    let alpha = 0.076 * (waves.speed * waves.speed / (waves.fetch * g)).powf(0.22);
    let peak = 22.0 * (g * g / (waves.speed * waves.fetch)).cbrt();
    let sigma = if omega <= peak { 0.07 } else { 0.09 };
    let r = (-(omega - peak).powi(2) / (2.0 * sigma * sigma * peak * peak)).exp();
    alpha * g * g / omega.powi(5) * (-1.25 * (peak / omega).powi(4)).exp() * waves.gamma.powf(r)
}

/// `cos^2s` of half the angle from the wind, normalised to 1 over the
/// circle.
fn spreading(theta: f64, s: f64) -> f64 {
    const STEPS: usize = 256;
    let shape = |t: f64| (t * 0.5).cos().abs().powf(2.0 * s);
    let area: f64 =
        (0..STEPS).map(|i| shape(-std::f64::consts::PI + (i as f64 + 0.5) * std::f64::consts::TAU / STEPS as f64)).sum::<f64>()
            * std::f64::consts::TAU
            / STEPS as f64;
    shape(theta) / area
}

/// The lowest wave number cascade `cascade` holds; the next cascade's
/// lowest is its highest. Each resolves its band with at least six waves to
/// its tile.
pub fn band_low(cascade: usize) -> f64 {
    match cascade {
        0 => 0.0,
        _ => std::f64::consts::TAU * 6.0 / f64::from(CASCADE_SIZES[cascade]),
    }
}

pub fn band_high(cascade: usize) -> f64 {
    if cascade + 1 < CASCADES { band_low(cascade + 1) } else { f64::INFINITY }
}

/// A complex number, as the spectrum and the FFT hold them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };

    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    fn mul(self, other: Self) -> Self {
        Self::new(self.re * other.re - self.im * other.im, self.re * other.im + self.im * other.re)
    }

    fn add(self, other: Self) -> Self {
        Self::new(self.re + other.re, self.im + other.im)
    }

    fn sub(self, other: Self) -> Self {
        Self::new(self.re - other.re, self.im - other.im)
    }

    fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    fn turn(angle: f64) -> Self {
        Self::new(angle.cos(), angle.sin())
    }
}

/// Two numbers from 0 to 1, from a cascade and a place in its spectrum: the
/// same for that wave wherever it is computed.
fn uniforms(cascade: usize, n: i32, m: i32) -> (f64, f64) {
    let mut h = (cascade as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (n as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (m as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    let mut next = || {
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        ((h >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    };
    (next(), next())
}

/// The wave at place (`n`, `m`) of cascade `cascade`'s spectrum, as it
/// starts: a random height, normal in each part, scaled by how much energy
/// the sea has at its wave number and direction. Nothing outside the
/// cascade's band.
pub fn initial_wave(sea: &SeaState, cascade: usize, n: i32, m: i32) -> Complex {
    let size = f64::from(CASCADE_SIZES[cascade]);
    let dk = std::f64::consts::TAU / size;
    let (kx, kz) = (f64::from(n) * dk, f64::from(m) * dk);
    let k = (kx * kx + kz * kz).sqrt();
    if k < 1e-6 || k < band_low(cascade) || k >= band_high(cascade) {
        return Complex::ZERO;
    }
    let omega = (GRAVITY * k).sqrt();
    let theta = kz.atan2(kx) - sea.direction[1].atan2(sea.direction[0]);
    let theta = (theta + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    let energy = |waves: &Waves| jonswap(omega, waves) * spreading(theta, waves.spread);
    // From frequency to wave number: dω/dk is g / 2ω in deep water, and a polar cell of the grid is k wide.
    let density = (energy(&sea.swell) + energy(&sea.wind)) * GRAVITY / (2.0 * omega) / k;
    // Ripples shorter than a few centimetres are gone, as the FFT cannot hold them without aliasing.
    let damping = (-k * k * 0.01 * 0.01).exp();
    // A quarter: half for each of the wave's two random parts, and half again as each wave is summed with its
    // mirror, h0(k) and h0*(-k), which would double its energy.
    let amplitude = (density * damping * dk * dk * 0.25).sqrt() * sea.scale;
    let (u1, u2) = uniforms(cascade, n, m);
    // Box–Muller: two normal numbers from two uniform ones.
    let radius = (-2.0 * u1.ln()).sqrt();
    let angle = std::f64::consts::TAU * u2;
    Complex::new(radius * angle.cos() * amplitude, radius * angle.sin() * amplitude)
}

/// A cascade's starting spectrum at `resolution` across: the place
/// (`n`, `m`) at index `(m mod N) * N + (n mod N)`, n and m from -N/2. The
/// Nyquist row and column are left empty, as they have no partner wave.
pub fn initial_spectrum(sea: &SeaState, cascade: usize, resolution: usize) -> Vec<Complex> {
    let half = resolution as i32 / 2;
    let mut out = vec![Complex::ZERO; resolution * resolution];
    for m in -half + 1..half {
        for n in -half + 1..half {
            let at = index(n, resolution) + index(m, resolution) * resolution;
            out[at] = initial_wave(sea, cascade, n, m);
        }
    }
    out
}

fn index(n: i32, resolution: usize) -> usize {
    n.rem_euclid(resolution as i32) as usize
}

/// A cascade's surface at a moment: how far each point of a tile is moved,
/// across (x, z) and up (y), at `resolution` across.
pub struct Displacement {
    pub resolution: usize,
    pub size: f32,
    /// x, y, z for each point, rows along x, at `(z * N + x)`.
    pub moved: Vec<[f32; 3]>,
}

/// The cascade's waves, moved on to `time`, as heights and choppy
/// sideways movement over its tile: `h(k, t) = h0(k) e^{iωt} + h0*(-k)
/// e^{-iωt}`, summed by an inverse FFT.
pub fn displacement(initial: &[Complex], resolution: usize, cascade: usize, time: f64, choppiness: f32) -> Displacement {
    let n = resolution;
    let half = n as i32 / 2;
    let size = f64::from(CASCADE_SIZES[cascade]);
    let dk = std::f64::consts::TAU / size;
    let phase_time = time.rem_euclid(REPEAT_SECONDS);
    // Across and up in one complex FFT: x as the real part, y as the imaginary, both real fields.
    let mut xy = vec![Complex::ZERO; n * n];
    let mut z = vec![Complex::ZERO; n * n];
    for m in -half + 1..half {
        for k_n in -half + 1..half {
            let at = index(k_n, n) + index(m, n) * n;
            let opposite = index(-k_n, n) + index(-m, n) * n;
            let (kx, kz) = (f64::from(k_n) * dk, f64::from(m) * dk);
            let k = (kx * kx + kz * kz).sqrt();
            if k < 1e-9 {
                continue;
            }
            let turn = Complex::turn(frequency(k) * phase_time);
            let h = initial[at].mul(turn).add(initial[opposite].conj().mul(turn.conj()));
            // Moving the water towards each crest: i k/|k| h (a crest at a cosine's peak draws its sides in).
            let sideways = |along: f64| Complex::new(0.0, along / k).mul(h);
            let (dx, dz) = (sideways(kx), sideways(kz));
            // x + i y, as both are real: a real field's spectrum times i is still a real field's, times i.
            xy[at] = dx.add(Complex::new(0.0, 1.0).mul(h));
            z[at] = dz;
        }
    }
    inverse_fft_2d(&mut xy, n);
    inverse_fft_2d(&mut z, n);
    let chop = f64::from(choppiness);
    let moved = xy.iter().zip(&z).map(|(xy, z)| [(xy.re * chop) as f32, xy.im as f32, (z.re * chop) as f32]).collect();
    Displacement { resolution: n, size: size as f32, moved }
}

/// An in-place inverse FFT without the 1/N, of `n` (a power of two) values.
fn inverse_fft(values: &mut [Complex]) {
    let n = values.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            values.swap(i, j);
        }
    }
    let mut length = 2;
    while length <= n {
        let step = Complex::turn(std::f64::consts::TAU / length as f64);
        for start in (0..n).step_by(length) {
            let mut w = Complex::new(1.0, 0.0);
            for i in 0..length / 2 {
                let a = values[start + i];
                let b = values[start + i + length / 2].mul(w);
                values[start + i] = a.add(b);
                values[start + i + length / 2] = a.sub(b);
                w = w.mul(step);
            }
        }
        length <<= 1;
    }
}

/// A 2D inverse FFT of `n` by `n` values, rows then columns.
fn inverse_fft_2d(values: &mut [Complex], n: usize) {
    for row in values.chunks_mut(n) {
        inverse_fft(row);
    }
    let mut column = vec![Complex::ZERO; n];
    for x in 0..n {
        for (z, value) in column.iter_mut().enumerate() {
            *value = values[z * n + x];
        }
        inverse_fft(&mut column);
        for (z, value) in column.iter().enumerate() {
            values[z * n + x] = *value;
        }
    }
}

impl Displacement {
    /// How far the surface is moved at `x`, `z` metres, read between the
    /// tile's points, which repeat.
    pub fn at(&self, x: f32, z: f32) -> [f32; 3] {
        let n = self.resolution;
        let (fx, fz) = (x / self.size * n as f32, z / self.size * n as f32);
        let (x0, z0) = (fx.floor(), fz.floor());
        let (tx, tz) = (fx - x0, fz - z0);
        let wrap = |v: f32| (v as i64).rem_euclid(n as i64) as usize;
        let get = |x: f32, z: f32| self.moved[wrap(z) * n + wrap(x)];
        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        let top = lerp3(get(x0, z0), get(x0 + 1.0, z0), tx);
        let bottom = lerp3(get(x0, z0 + 1.0), get(x0 + 1.0, z0 + 1.0), tx);
        lerp3(top, bottom, tz)
    }
}

/// The sea's long waves on the CPU, for how high it is at a point: the two
/// longest cascades at 64×64, moved on to a moment.
pub struct OceanSampler {
    sea: SeaState,
    level: f32,
    initial: Vec<Vec<Complex>>,
    surfaces: Vec<Displacement>,
    time: f64,
}

impl OceanSampler {
    pub fn new(sea: SeaState, level: f32) -> Self {
        let initial = (0..QUERY_CASCADES).map(|c| initial_spectrum(&sea, c, QUERY_RESOLUTION)).collect();
        let mut sampler = Self { sea, level, initial, surfaces: Vec::new(), time: f64::NAN };
        sampler.set_time(0.0);
        sampler
    }

    pub fn sea(&self) -> &SeaState {
        &self.sea
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    /// Moves the waves on to `time`, in seconds; the same time again does
    /// nothing.
    pub fn set_time(&mut self, time: f64) {
        if time == self.time {
            return;
        }
        self.time = time;
        let chop = self.sea.choppiness;
        self.surfaces = (0..QUERY_CASCADES)
            .map(|c| displacement(&self.initial[c], QUERY_RESOLUTION, c, time, chop))
            .collect();
    }

    /// How far the long waves move the water resting at `x`, `z`, where
    /// the sea is `depth` metres deep.
    fn moved(&self, x: f32, z: f32, depth: f32) -> [f32; 3] {
        let mut total = [0.0; 3];
        for (cascade, surface) in self.surfaces.iter().enumerate() {
            let d = surface.at(x, z);
            let weight = shallow_weight(cascade, depth);
            for i in 0..3 {
                total[i] += d[i] * weight;
            }
        }
        let sideways = sideways_weight(depth);
        [total[0] * sideways, total[1], total[2] * sideways]
    }

    /// How high the sea is above `x`, `z`, in the world, out of sight of
    /// land: the water resting where the waves moved it from is found by
    /// stepping back three times, as the waves move water across as well
    /// as up.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.height_over(x, z, f32::INFINITY)
    }

    /// As `height_at`, where the sea is `depth` metres deep, its waves
    /// dying away in the shallows.
    pub fn height_over(&self, x: f32, z: f32, depth: f32) -> f32 {
        let (mut rx, mut rz) = (x, z);
        for _ in 0..3 {
            let d = self.moved(rx, rz, depth);
            rx = x - d[0];
            rz = z - d[2];
        }
        self.level + self.moved(rx, rz, depth)[1]
    }

    /// The way the sea faces at `x`, `z`, from its height a little either
    /// side.
    pub fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        self.normal_over(x, z, f32::INFINITY)
    }

    /// As `normal_at`, where the sea is `depth` metres deep.
    pub fn normal_over(&self, x: f32, z: f32, depth: f32) -> Vec3 {
        let e = 0.25;
        let dx = self.height_over(x + e, z, depth) - self.height_over(x - e, z, depth);
        let dz = self.height_over(x, z + e, depth) - self.height_over(x, z - e, depth);
        Vec3::new(-dx, 2.0 * e, -dz).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stormy() -> SeaState {
        Ocean { weather: Weather::Stormy, ..Ocean::default() }.sea_state(Quat::IDENTITY)
    }

    #[test]
    fn the_inverse_fft_sums_its_waves_as_a_direct_sum_does() {
        let n = 16;
        let values: Vec<Complex> = (0..n).map(|i| Complex::new((i as f64 * 0.7).sin(), (i as f64 * 1.3).cos())).collect();
        let mut fast = values.clone();
        inverse_fft(&mut fast);
        for (x, got) in fast.iter().enumerate() {
            let direct = values.iter().enumerate().fold(Complex::ZERO, |sum, (k, v)| {
                sum.add(v.mul(Complex::turn(std::f64::consts::TAU * (k * x) as f64 / n as f64)))
            });
            assert!((got.re - direct.re).abs() < 1e-9 && (got.im - direct.im).abs() < 1e-9, "{x}: {got:?} {direct:?}");
        }
    }

    #[test]
    fn a_storms_swell_is_metres_high_and_a_calm_sea_low() {
        let height = |weather| {
            let sea = Ocean { weather, ..Ocean::default() }.sea_state(Quat::IDENTITY);
            let sampler = OceanSampler::new(sea, 0.0);
            let heights: Vec<f32> = sampler.surfaces[0].moved.iter().map(|m| m[1]).collect();
            let mean_square: f32 = heights.iter().map(|h| h * h).sum::<f32>() / heights.len() as f32;
            // The significant wave height, as sailors judge it: four standard deviations.
            4.0 * mean_square.sqrt()
        };
        let (calm, stormy) = (height(Weather::Calm), height(Weather::Stormy));
        assert!((4.0..8.0).contains(&stormy), "a storm's swell: {stormy} m");
        assert!(calm < 1.5, "a calm swell: {calm} m");
        assert!(calm < stormy * 0.5, "calm {calm} m against stormy {stormy} m");
    }

    #[test]
    fn the_waves_are_real_and_repeat_after_their_period() {
        let sea = stormy();
        let initial = initial_spectrum(&sea, 1, QUERY_RESOLUTION);
        let now = displacement(&initial, QUERY_RESOLUTION, 1, 12.5, 1.0);
        let later = displacement(&initial, QUERY_RESOLUTION, 1, 12.5 + REPEAT_SECONDS, 1.0);
        for (a, b) in now.moved.iter().zip(&later.moved) {
            assert!((a[1] - b[1]).abs() < 1e-3, "{a:?} {b:?}");
        }
        assert!(now.moved.iter().any(|m| m[1].abs() > 0.01), "there are waves");
    }

    #[test]
    fn a_wave_is_the_same_wherever_it_is_computed() {
        let sea = stormy();
        let small = initial_spectrum(&sea, 1, 16);
        let large = initial_spectrum(&sea, 1, 64);
        for (n, m) in [(1, 2), (-3, 5), (7, -7)] {
            assert_eq!(small[index(n, 16) + index(m, 16) * 16], large[index(n, 64) + index(m, 64) * 64]);
        }
    }

    #[test]
    fn crests_draw_the_water_in_so_they_sharpen() {
        // One wave along x: its peak at x = 0, so the water just past the peak moves back towards it.
        let mut initial = vec![Complex::ZERO; 64 * 64];
        initial[index(4, 64)] = Complex::new(0.5, 0.0);
        let d = displacement(&initial, 64, 1, 0.0, 1.0);
        let peak = d.at(0.0, 0.0)[1];
        let past = d.at(CASCADE_SIZES[1] / 64.0 * 2.0, 0.0);
        assert!(peak > 0.0 && past[0] < 0.0, "peak {peak}, the water past it moved {}", past[0]);
    }

    #[test]
    fn waves_die_away_in_shallow_water_the_longest_first() {
        assert_eq!(shallow_weight(0, 100.0), 1.0, "deep water keeps them all");
        assert!(shallow_weight(0, 5.0) < 0.3 && shallow_weight(3, 5.0) > 0.99, "five metres stops the swell, not the chop");
        assert!((shallow_weight(2, 0.0) - 0.1).abs() < 1e-6, "a tenth at the water's edge");
        assert_eq!(sideways_weight(0.0), 0.0);
        let sea = OceanSampler::new(stormy(), 0.0);
        let (deep, shallow) = (sea.height_over(30.0, 40.0, 1e4).abs(), sea.height_over(30.0, 40.0, 0.5).abs());
        assert!(shallow < deep * 0.5 + 0.01, "the same place, shallower: {shallow} against {deep}");
    }

    #[test]
    fn the_sea_is_as_high_where_asked_as_its_waves_put_it() {
        let mut sampler = OceanSampler::new(stormy(), 2.0);
        sampler.set_time(30.0);
        // Where the water resting at (40, 10) was moved to, the height is that water's.
        let d = sampler.moved(40.0, 10.0, f32::INFINITY);
        let height = sampler.height_at(40.0 + d[0], 10.0 + d[2]);
        assert!((height - (2.0 + d[1])).abs() < 0.05, "{height} against {}", 2.0 + d[1]);
        let normal = sampler.normal_at(40.0, 10.0);
        assert!(normal.y > 0.5, "{normal}");
    }
}
