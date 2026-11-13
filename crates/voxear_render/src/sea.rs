//! Rough water on a world of blocks' sea: swells that sink its top under and
//! near a storm, drawn as small cubes, as columns of blocks that step up and
//! down, or smooth; and whirlpools that draw it down into a funnel.
//!
//! Nothing of the world's blocks changes. The sea's faces stay where they
//! are, flat at the sea's own height, which is the height of a crest; the
//! water under each place is drawn as far below that as the swell there has
//! sunk it (`blockSwell` in `shaders/water.slang`), and what floats is held
//! at the same height ([`SeaSampler`]). A calm sea is sunk nowhere.
//!
//! The swell is a few long waves along the wind and across it, each a whole
//! number of times round in [`PERIOD`] metres, so they read the same from
//! wherever the world's middle has moved to. How rough the sea is at a place
//! is the roughest of the weather that lies over it
//! ([`crate::weather::WeatherPlaces`]), on top of the level's own. The time
//! is the sea's own, which the engine keeps the same on every machine.

use voxear_core::math::{Vec2, Vec3};

use crate::weather::WeatherPlace;

/// How a rough sea is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeaLook {
    /// Flat, whatever the weather.
    Flat,
    /// Columns a block wide that stand half a block higher or lower.
    Steps,
    /// Small cubes, four to a block's side.
    #[default]
    SmallCubes,
    /// A smooth swell, as a sea that is not of blocks.
    Smooth,
}

impl SeaLook {
    pub const ALL: [Self; 4] = [Self::Flat, Self::Steps, Self::SmallCubes, Self::Smooth];

    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::Steps => "Block Steps",
            Self::SmallCubes => "Small Cubes",
            Self::Smooth => "Smooth",
        }
    }

    /// Its place in [`Self::ALL`], as scripts and shaders name it.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&look| look == self).unwrap_or(0)
    }

    /// How wide a column of it is and how high a step, in blocks; nothing
    /// for smooth.
    fn cubes(self) -> (f32, f32) {
        match self {
            Self::Flat | Self::Smooth => (0.0, 0.0),
            Self::Steps => (1.0, 0.5),
            Self::SmallCubes => (0.25, 0.25),
        }
    }
}

/// How high a world of blocks' sea's top is over the world's corner, in
/// metres, a component beside the world: only its water faces there are
/// drawn rough and sampled so, and ponds and rivers stay calm. Without one,
/// all of a world's still water is its sea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaLevel(pub f32);

impl voxear_ecs::Component for SeaLevel {}

/// What a world with no [`SeaLevel`] gives the shaders for one.
pub const ANY_WATER: f32 = -1.0e9;

/// The most places of weather and whirlpools a frame's sea reads.
pub const MAX_STORMS: usize = 8;
pub const MAX_WHIRLPOOLS: usize = 4;
/// How many waves the swell is made of.
pub const WAVES: usize = 4;
/// How far the swell's pattern goes before it repeats, in metres.
pub const PERIOD: f32 = 4096.0;

/// A whirlpool: its middle along x and z, in the world, how far across its
/// funnel reaches and how deep its middle is drawn down, in metres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Whirlpool {
    pub middle: Vec2,
    pub radius: f32,
    pub depth: f32,
}

/// A world of blocks' sea this frame, a resource: how it is drawn where it
/// is rough, and its whirlpools. Without one the sea is flat.
#[derive(Clone, Debug, PartialEq)]
pub struct RoughSea {
    pub look: SeaLook,
    /// How far the deepest trough of the roughest sea lies under a crest,
    /// in metres.
    pub height: f32,
    /// How rough the sea is everywhere, 0 to 1, before any storm.
    pub everywhere: f32,
    /// Which way the swell runs, as an angle from +X towards +Z, in radians.
    pub heading: f32,
    /// The sea's own time, in seconds: the same on every machine.
    pub time: f64,
    /// The nearest whirlpools, the nearest first.
    pub whirlpools: Vec<Whirlpool>,
    /// How much the water glows where it breaks and is drawn down, as warm
    /// seas do at night: 0 for none.
    pub glow: f32,
}

impl Default for RoughSea {
    fn default() -> Self {
        Self {
            look: SeaLook::default(),
            height: 3.0,
            everywhere: 0.0,
            heading: 0.0,
            time: 0.0,
            whirlpools: Vec::new(),
            glow: 0.0,
        }
    }
}

/// The sea as the shaders read it (`Sea` in `shaders/common.slang`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuSea {
    /// 0 flat, 1 steps, 2 small cubes, 3 smooth.
    pub look: u32,
    pub storms: u32,
    pub whirlpools: u32,
    /// How rough it is everywhere.
    pub everywhere: f32,
    /// How deep the deepest trough is, in metres, and how wide a column is
    /// and how high a step, in blocks; a column of 0 is smooth.
    pub height: f32,
    pub column: f32,
    pub step: f32,
    pub glow: f32,
    /// How far round a whirlpool's arms have turned, in radians, and how
    /// far the patches of white water have drifted, in patches.
    pub spin: f32,
    pub drift: [f32; 2],
    /// How high the sea's top is, in the world; far below with no sea
    /// level. What grows under it sways with its water.
    pub level: f32,
    /// Each wave: how many radians a metre along x and z, how far on it is
    /// now, in radians, and its share of the swell.
    pub waves: [[f32; 4]; WAVES],
    /// Each place of weather: its middle's x and z, its reach and its eye.
    pub places: [[f32; 4]; MAX_STORMS],
    /// What each adds to the sky: cloud, darkness, what falls from it, and
    /// blown sand.
    pub skies: [[f32; 4]; MAX_STORMS],
    /// How rough each makes the sea.
    pub roughs: [f32; MAX_STORMS],
    /// Each whirlpool: its middle's x and z, its reach and its depth.
    pub pools: [[f32; 4]; MAX_WHIRLPOOLS],
}

/// Each wave of the swell: how long it is, in metres, how far its way is
/// turned from the swell's, in radians, and its share of the swell.
const SWELL: [(f32, f32, f32); WAVES] = [(46.0, 0.0, 0.42), (27.0, 0.45, 0.28), (17.0, -0.6, 0.18), (11.0, 1.1, 0.12)];
/// How fast a whirlpool's arms turn, in radians a second, and how fast the
/// patches of white water drift, in patches a second.
const SPIN: f64 = 1.6;
const DRIFT: [f64; 2] = [0.21, 0.13];
/// How much of a whirlpool's reach its fastest water is from its middle.
const POOL_CORE: f32 = 0.18;

/// The sine of `x`, from sums and products alone: the same bits on every
/// machine. Within a thousandth of the true sine.
fn sine(x: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let x = x - (x / TAU + 0.5).floor() * TAU;
    // A parabola through the sine's noughts and peak, then drawn towards the sine.
    let parabola = (4.0 / PI) * x - (4.0 / (PI * PI)) * x * x.abs();
    0.225 * (parabola * parabola.abs() - parabola) + parabola
}

fn cosine(x: f64) -> f64 {
    sine(x + std::f64::consts::FRAC_PI_2)
}

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One wave of the swell: radians a metre along x and z, and radians a
/// second.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Wave {
    k: [f64; 2],
    speed: f64,
    share: f32,
}

fn waves(heading: f32) -> [Wave; WAVES] {
    SWELL.map(|(length, turned, share)| {
        let angle = heading + turned;
        // A whole number of times round in the period, each way, so the pattern has no seam.
        let around = PERIOD / length;
        let (kx, kz) = ((around * angle.cos()).round(), (around * angle.sin()).round());
        let k = std::f64::consts::TAU / f64::from(PERIOD);
        let (kx, kz) = (f64::from(kx) * k, f64::from(kz) * k);
        // Deep water's waves: the longer, the faster.
        let speed = (9.81 * (kx * kx + kz * kz).sqrt()).sqrt();
        Wave { k: [kx, kz], speed, share }
    })
}

impl RoughSea {
    /// As the shaders read it, with the weather that lies over parts of the
    /// level.
    pub fn gpu(&self, places: &[WeatherPlace]) -> GpuSea {
        let (column, step) = self.look.cubes();
        let tau = std::f64::consts::TAU;
        let mut sea = GpuSea {
            look: self.look.index() as u32,
            storms: places.len().min(MAX_STORMS) as u32,
            whirlpools: self.whirlpools.len().min(MAX_WHIRLPOOLS) as u32,
            everywhere: self.everywhere.clamp(0.0, 1.0),
            height: self.height.max(0.0),
            column,
            step,
            glow: self.glow.max(0.0),
            spin: (self.time * SPIN).rem_euclid(tau) as f32,
            drift: DRIFT.map(|speed| (self.time * speed).rem_euclid(4096.0) as f32),
            level: ANY_WATER,
            waves: waves(self.heading).map(|wave| {
                [wave.k[0] as f32, wave.k[1] as f32, (self.time * wave.speed).rem_euclid(tau) as f32, wave.share]
            }),
            ..GpuSea::default()
        };
        for (index, place) in places.iter().take(MAX_STORMS).enumerate() {
            let adds = place.amounts;
            sea.places[index] = [place.middle.x, place.middle.y, place.radius.max(1.0), place.eye.clamp(0.0, 0.8)];
            sea.skies[index] = [adds.cloud, adds.darkness, adds.rain.max(adds.snow), adds.dust];
            sea.roughs[index] = place.rough.clamp(0.0, 1.0);
        }
        for (to, pool) in sea.pools.iter_mut().zip(&self.whirlpools) {
            *to = [pool.middle.x, pool.middle.y, pool.radius.max(1.0), pool.depth.max(0.0)];
        }
        sea
    }

    /// The sea as what floats on it asks about it, with the weather that
    /// lies over parts of the level and where the world's middle is from
    /// the level's, along x and z, in metres.
    pub fn sampler(&self, places: &[WeatherPlace], origin: [f64; 2]) -> SeaSampler {
        SeaSampler {
            height: self.height.max(0.0),
            everywhere: self.everywhere.clamp(0.0, 1.0),
            time: self.time,
            waves: waves(self.heading),
            places: places.to_vec(),
            pools: self.whirlpools.clone(),
            origin,
        }
    }
}

/// The swell and the whirlpools of a sea at one moment, on the CPU: the
/// same as are drawn, smooth, whatever its look.
#[derive(Clone, Debug)]
pub struct SeaSampler {
    height: f32,
    everywhere: f32,
    time: f64,
    waves: [Wave; WAVES],
    places: Vec<WeatherPlace>,
    pools: Vec<Whirlpool>,
    origin: [f64; 2],
}

/// The sea at a point: how far under its faces it stands, in metres, how
/// its water moves, in metres a second, and how rough it is, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeaPoint {
    pub sunk: f32,
    pub flow: Vec3,
    pub rough: f32,
    /// How far into a whirlpool it is, from 0 at its rim to 1 at its
    /// middle; 0 outside any.
    pub whirlpool: f32,
}

impl SeaSampler {
    /// Whether the sea is flat everywhere: nothing to work out.
    pub fn is_calm(&self) -> bool {
        self.everywhere <= 0.0 && self.pools.is_empty() && self.places.iter().all(|place| place.rough <= 0.0)
    }

    /// How rough the sea is at a point, from 0 to 1.
    pub fn rough_at(&self, x: f32, z: f32) -> f32 {
        let at = Vec2::new(x, z);
        let storms = self.places.iter().map(|place| place.rough * place.share(place.middle.distance(at)));
        storms.fold(self.everywhere, f32::max).clamp(0.0, 1.0)
    }

    /// How far under its faces the sea stands at a point, in metres, at
    /// `time`; how near a crest it is there, and how rough.
    fn sunk(&self, x: f32, z: f32, time: f64) -> (f32, f32, f32) {
        let mut rough = self.rough_at(x, z);
        let period = f64::from(PERIOD);
        let at = [f64::from(x) + self.origin[0], f64::from(z) + self.origin[1]].map(|v| v.rem_euclid(period));
        let mut swell = 0.0;
        for wave in &self.waves {
            let phase = at[0] * wave.k[0] + at[1] * wave.k[1] - time * wave.speed;
            // Peaked at its crest and round in its trough, as a wave near breaking is.
            let round = 0.5 + 0.5 * cosine(phase);
            let peaked = 1.0 - sine(phase * 0.5).abs();
            swell += wave.share * (round + (peaked - round) * 0.6) as f32;
        }
        let crest = ((swell - 0.15) / 0.7).clamp(0.0, 1.0);
        let mut sunk = self.height * rough * (1.0 - crest);
        for pool in &self.pools {
            let far = Vec2::new(x, z).distance(pool.middle) / pool.radius.max(1.0);
            if far < 1.0 {
                let into = 1.0 - far;
                sunk = sunk * (1.0 - smooth(0.0, 0.5, into)) + pool.depth * into * into * (0.4 + 0.6 * into);
                rough = rough.max(smooth(0.0, 0.35, into));
            }
        }
        (sunk, crest, rough)
    }

    /// The sea at a point.
    pub fn at(&self, x: f32, z: f32) -> SeaPoint {
        let (sunk, _, rough) = self.sunk(x, z, self.time);
        // Its water rises and falls with the swell, and runs on with each crest and back in each trough.
        let before = self.sunk(x, z, self.time - 0.05).0;
        let mut flow = Vec3::new(0.0, (before - sunk) / 0.05, 0.0);
        let period = f64::from(PERIOD);
        let at = [f64::from(x) + self.origin[0], f64::from(z) + self.origin[1]].map(|v| v.rem_euclid(period));
        for wave in &self.waves {
            let phase = at[0] * wave.k[0] + at[1] * wave.k[1] - self.time * wave.speed;
            let k = (wave.k[0] * wave.k[0] + wave.k[1] * wave.k[1]).sqrt();
            let along = 0.5 * self.height * rough * wave.share * (wave.speed * cosine(phase)) as f32;
            flow.x += along * (wave.k[0] / k) as f32;
            flow.z += along * (wave.k[1] / k) as f32;
        }
        let mut whirlpool = 0.0f32;
        for pool in &self.pools {
            let from = Vec2::new(x, z) - pool.middle;
            let far = from.length() / pool.radius.max(1.0);
            if far >= 1.0 {
                continue;
            }
            let into = 1.0 - far;
            whirlpool = whirlpool.max(into);
            // Round its middle, fastest a little way out from it, and in towards it; at its middle, down.
            let fastest = (2.0 * 9.81 * pool.depth.max(0.0)).sqrt() * 0.6;
            let round = fastest * (far / POOL_CORE).min(POOL_CORE / far.max(1e-3)).min(1.0) * smooth(0.0, 0.25, into);
            let out = from.normalize_or_zero();
            let turning = Vec2::new(-out.y, out.x);
            let moving = turning * round - out * round * 0.45;
            flow.x += moving.x;
            flow.z += moving.y;
            flow.y -= pool.depth * 0.5 * smooth(1.0 - POOL_CORE * 1.5, 1.0, into);
        }
        SeaPoint { sunk, flow, rough, whirlpool }
    }

    /// The way the sea's surface faces at a point.
    pub fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        let reach = 0.5;
        let here = self.sunk(x, z, self.time).0;
        let dx = self.sunk(x + reach, z, self.time).0 - here;
        let dz = self.sunk(x, z + reach, self.time).0 - here;
        Vec3::new(dx, reach, dz).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::{WeatherAmounts, WeatherKind};

    fn storm(x: f32, z: f32, radius: f32) -> WeatherPlace {
        WeatherPlace { middle: Vec2::new(x, z), radius, amounts: WeatherKind::Storm.amounts(), rough: 1.0, eye: 0.0 }
    }

    #[test]
    fn the_sine_from_sums_and_products_is_the_sine() {
        for step in -2000..2000 {
            let x = f64::from(step) * 0.0173;
            assert!((sine(x) - x.sin()).abs() < 1.2e-3, "{x}: {} and {}", sine(x), x.sin());
            assert!((cosine(x) - x.cos()).abs() < 1.2e-3);
        }
    }

    #[test]
    fn a_sea_is_flat_in_calm_weather_and_rises_and_falls_under_a_storm() {
        let sea = RoughSea::default();
        let calm = sea.sampler(&[], [0.0; 2]);
        assert!(calm.is_calm());
        assert_eq!(calm.at(10.0, -40.0), SeaPoint::default());
        assert_eq!(calm.normal_at(3.0, 4.0), Vec3::Y);

        let places = [storm(0.0, 0.0, 500.0)];
        let stormy = sea.sampler(&places, [0.0; 2]);
        assert!(!stormy.is_calm());
        let sunk: Vec<f32> = (0..400).map(|i| stormy.at(i as f32 * 0.9 - 180.0, i as f32 * 0.37).sunk).collect();
        let (least, most) = sunk.iter().fold((f32::MAX, f32::MIN), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(least >= 0.0 && least < 0.3, "crests at the sea's own height: {least}");
        assert!(most > sea.height * 0.7 && most <= sea.height, "troughs metres down: {most}");
        // Out of the storm's reach the sea is as flat as ever, and calmer towards its edge.
        assert_eq!(stormy.at(900.0, 0.0).sunk, 0.0);
        assert!(stormy.rough_at(420.0, 0.0) < 0.5 && stormy.rough_at(100.0, 0.0) == 1.0);
        // Its water moves: up and down, and along with the waves.
        let moving = (0..50).map(|i| stormy.at(i as f32 * 3.0, 20.0).flow.length()).fold(0.0, f32::max);
        assert!(moving > 0.5 && moving < 12.0, "{moving} m/s");
        // Weather that roughens nothing, as fog, leaves it flat.
        let fog = WeatherPlace { rough: 0.0, amounts: WeatherAmounts { fog: 1.0, ..Default::default() }, ..places[0] };
        assert!(sea.sampler(&[fog], [0.0; 2]).is_calm());
    }

    #[test]
    fn the_swell_is_the_same_wherever_the_worlds_middle_has_moved_to_and_moves_on_with_time() {
        let places = [storm(0.0, 0.0, 5000.0)];
        let sea = RoughSea { time: 31.5, heading: 0.6, ..RoughSea::default() };
        let here = sea.sampler(&places, [0.0; 2]);
        // The same place in the level, seen from a world whose middle is 8192 m and 1000 m on.
        let moved = sea.sampler(&[storm(-8192.0, -1000.0, 5000.0)], [8192.0, 1000.0]);
        for i in 0..40 {
            let (x, z) = (i as f32 * 7.3, i as f32 * -3.1);
            let (a, b) = (here.at(x, z).sunk, moved.at(x - 8192.0, z - 1000.0).sunk);
            assert!((a - b).abs() < 2e-3, "at {x}, {z}: {a} and {b}");
        }
        let later = RoughSea { time: 33.0, ..sea.clone() }.sampler(&places, [0.0; 2]);
        let changed = (0..40).filter(|&i| (here.at(i as f32 * 5.0, 0.0).sunk - later.at(i as f32 * 5.0, 0.0).sunk).abs() > 0.05);
        assert!(changed.count() > 20, "the waves have run on");
        // And every machine is given the same waves: a whole number of them round the period.
        for wave in sea.gpu(&places).waves {
            let around = wave[0] * PERIOD / std::f32::consts::TAU;
            assert!((around - around.round()).abs() < 1e-3, "{around}");
        }
    }

    #[test]
    fn a_whirlpool_draws_the_sea_down_to_its_middle_and_carries_its_water_round_and_in() {
        let pool = Whirlpool { middle: Vec2::new(100.0, 50.0), radius: 20.0, depth: 6.0 };
        let sea = RoughSea { whirlpools: vec![pool], ..RoughSea::default() }.sampler(&[], [0.0; 2]);
        assert!(!sea.is_calm());
        let middle = sea.at(100.0, 50.0);
        assert!((middle.sunk - 6.0).abs() < 0.01 && middle.whirlpool == 1.0 && middle.flow.y < -1.0, "{middle:?}");
        let (rim, half) = (sea.at(119.0, 50.0), sea.at(110.0, 50.0));
        assert!(rim.sunk < 0.1 && half.sunk > 1.0 && half.sunk < 4.0, "{} and {}", rim.sunk, half.sunk);
        assert_eq!(sea.at(125.0, 50.0), SeaPoint::default(), "nothing beyond its reach");
        // East of its middle the water runs north and in, west; faster nearer the middle.
        assert!(half.flow.z > 1.0 && half.flow.x < -0.3, "{:?}", half.flow);
        let near = sea.at(104.0, 50.0);
        assert!(near.flow.length() > half.flow.length() && half.flow.length() > rim.flow.length());
        // Its sides slope down to its middle.
        assert!(sea.normal_at(110.0, 50.0).x < -0.1, "{:?}", sea.normal_at(110.0, 50.0));
    }

    #[test]
    fn the_shaders_are_given_each_place_its_sky_and_how_rough_it_makes_the_sea() {
        let eyed = WeatherPlace { eye: 0.3, ..storm(40.0, -20.0, 300.0) };
        let sea = RoughSea { look: SeaLook::Steps, glow: 0.5, ..RoughSea::default() };
        let gpu = sea.gpu(&[eyed, WeatherPlace { rough: 0.0, ..storm(0.0, 0.0, 100.0) }]);
        assert_eq!((gpu.look, gpu.storms, gpu.whirlpools), (1, 2, 0));
        assert_eq!((gpu.column, gpu.step, gpu.glow), (1.0, 0.5, 0.5));
        assert_eq!(gpu.places[0], [40.0, -20.0, 300.0, 0.3]);
        assert_eq!((gpu.roughs[0], gpu.roughs[1]), (1.0, 0.0));
        let adds = WeatherKind::Storm.amounts();
        assert_eq!(gpu.skies[0], [adds.cloud, adds.darkness, adds.rain, 0.0]);
        for look in SeaLook::ALL {
            assert_eq!(SeaLook::ALL[look.index()], look);
        }
    }
}
