//! Rough water on a world of blocks' sea: swells that sink its top under and
//! near a storm, drawn as columns of cubes that step up and down, as small
//! cubes, or smooth.
//!
//! Nothing of the world's blocks changes. The sea's faces stay where they
//! are, flat at the sea's own height, which is the height of a crest; the
//! water under each place is drawn as far below that as the swell there has
//! sunk it (`blockSwell` in `shaders/water.slang`), and what floats is held
//! at the same height ([`RoughSea::sunk_at`]). A calm sea is sunk nowhere.
//!
//! The swell is a few long waves along the wind and across it, each a whole
//! number of times round in [`PERIOD`] metres, so they read the same from
//! wherever the world's middle has moved to. How rough the sea is at a place
//! is the strongest of the storms that reach it, on top of the level's own.

use voxear_core::math::Vec2;

/// How a rough sea is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeaLook {
    /// Flat, whatever the weather.
    #[default]
    Flat,
    /// Columns a block wide that stand a part of a block higher or lower.
    Steps,
    /// Columns of small cubes, several to a block's side.
    SmallCubes,
    /// A smooth swell, as a sea that is not of blocks.
    Smooth,
}

/// The most storms and whirlpools a frame's sea reads.
pub const MAX_STORMS: usize = 8;
pub const MAX_WHIRLPOOLS: usize = 4;
/// How many waves the swell is made of.
pub const WAVES: usize = 4;
/// How far the swell's pattern goes before it repeats, in metres.
pub const PERIOD: f32 = 4096.0;

/// Somewhere the sea is rougher: its middle along x and z, how far it
/// reaches, in metres, and how rough it is at its middle, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoughPlace {
    pub middle: Vec2,
    pub radius: f32,
    pub strength: f32,
}

/// A whirlpool: its middle along x and z, how far across its funnel
/// reaches and how deep its middle is drawn down, in metres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Whirlpool {
    pub middle: Vec2,
    pub radius: f32,
    pub depth: f32,
}

/// The sea's roughness this frame, a resource: without one the sea is flat.
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
    pub storms: Vec<RoughPlace>,
    pub whirlpools: Vec<Whirlpool>,
}

impl Default for RoughSea {
    fn default() -> Self {
        Self {
            look: SeaLook::Flat,
            height: 2.0,
            everywhere: 0.0,
            heading: 0.0,
            storms: Vec::new(),
            whirlpools: Vec::new(),
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
    /// How deep the deepest trough is, how wide a column is and how high a
    /// step, in metres; a column or a step of 0 is smooth.
    pub height: f32,
    pub column: f32,
    pub step: f32,
    pub pad: f32,
    /// Each wave: how many radians a metre along x and z, how many a
    /// second, and its share of the swell.
    pub waves: [[f32; 4]; WAVES],
    /// Each storm: its middle's x and z, its reach and its strength.
    pub places: [[f32; 4]; MAX_STORMS],
    /// Each whirlpool: its middle's x and z, its reach and its depth.
    pub pools: [[f32; 4]; MAX_WHIRLPOOLS],
}

/// Each wave of the swell: how long it is, in metres, how far its way is
/// turned from the swell's, in radians, and its share of the swell.
const SWELL: [(f32, f32, f32); WAVES] = [(46.0, 0.0, 0.42), (27.0, 0.45, 0.28), (17.0, -0.6, 0.18), (11.0, 1.1, 0.12)];

impl RoughSea {
    /// The waves the swell is made of, running along its heading.
    pub fn waves(&self) -> [[f32; 4]; WAVES] {
        SWELL.map(|(length, turned, share)| {
            let angle = self.heading + turned;
            // A whole number of times round in the period, each way, so the pattern has no seam.
            let around = PERIOD / length;
            let (kx, kz) = ((around * angle.cos()).round(), (around * angle.sin()).round());
            let k = std::f32::consts::TAU / PERIOD;
            // Deep water's waves: the longer, the faster.
            let speed = (9.81 * k * (kx * kx + kz * kz).sqrt()).sqrt();
            [kx * k, kz * k, speed, share]
        })
    }

    /// As the shaders read it; `origin` is where the world's middle is from
    /// the level's, which storms and whirlpools are placed from.
    pub fn gpu(&self) -> GpuSea {
        let (column, step) = match self.look {
            SeaLook::Flat | SeaLook::Smooth => (0.0, 0.0),
            SeaLook::Steps => (1.0, 0.5),
            SeaLook::SmallCubes => (0.25, 0.25),
        };
        let mut sea = GpuSea {
            look: self.look as u32,
            storms: self.storms.len().min(MAX_STORMS) as u32,
            whirlpools: self.whirlpools.len().min(MAX_WHIRLPOOLS) as u32,
            everywhere: self.everywhere.clamp(0.0, 1.0),
            height: self.height.max(0.0),
            column,
            step,
            pad: 0.0,
            waves: self.waves(),
            ..GpuSea::default()
        };
        for (to, storm) in sea.places.iter_mut().zip(&self.storms) {
            *to = [storm.middle.x, storm.middle.y, storm.radius.max(1.0), storm.strength.clamp(0.0, 1.0)];
        }
        for (to, pool) in sea.pools.iter_mut().zip(&self.whirlpools) {
            *to = [pool.middle.x, pool.middle.y, pool.radius.max(1.0), pool.depth.max(0.0)];
        }
        sea
    }
}
