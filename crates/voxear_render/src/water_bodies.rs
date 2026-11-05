//! Bodies of water besides the sea: a Pool fills a box of ground with water
//! (or lava) at its Obj's height, as a lake, a pond or a flooded room; a Dry
//! Area keeps water out of a box, a ball or a capsule, as the inside of a
//! boat's hull, a cave or a diving bell; a Stream runs along its Obj's Path,
//! as a river, a brook or a flow of lava, as wide as the Path is.
//!
//! Each Pool draws its own surface, a grid over its footprint with the sea's
//! smallest waves on it; every water surface leaves out what lies inside a
//! Dry Area, pixel by pixel, so a moving boat's hull is dry exactly where it
//! is. A Pool below the sea's level is dry to the sea, so a pit by the shore
//! can hold a pond lower than the sea round it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::Component;
use voxear_scene::Path;

use crate::gpu::{GpuStreamPoint, GpuWaterBody, cols};

/// The most Pools, Streams and Dry Areas a frame draws, the nearest the
/// camera first.
pub const MAX_WATER_BODIES: usize = 64;
/// The most points along Streams a frame draws, and along any one Stream:
/// a longer one takes them further apart.
pub const MAX_STREAM_POINTS: usize = 8192;
pub const STREAM_POINTS: usize = 1024;
/// How far apart a Stream's points are, in metres, unless it is too long.
const STREAM_SPACING: f32 = 1.0;

/// What fills a body of water.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liquid {
    #[default]
    Water,
    Lava,
}

impl Liquid {
    pub const ALL: [Self; 2] = [Self::Water, Self::Lava];

    pub fn name(self) -> &'static str {
        match self {
            Self::Water => "Water",
            Self::Lava => "Lava",
        }
    }
}

/// A Dry Area's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DryShape {
    #[default]
    Box,
    Sphere,
    Capsule,
}

impl DryShape {
    pub const ALL: [Self; 3] = [Self::Box, Self::Sphere, Self::Capsule];

    pub fn name(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Capsule => "Capsule",
        }
    }
}

/// Water at its Obj's height over a box of ground `size` wide (x) and long
/// (z), turned with its Obj: a lake, a pond or a flooded room.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pool {
    /// Across and along, in metres.
    pub size: [f32; 2],
    /// How deep the water is, in metres: its walls and bottom show from
    /// outside, so a Pool raised over the ground is a block of water.
    pub depth: f32,
    pub liquid: Liquid,
    /// How much of the sea's long swell it takes, from 0 for none to 1 as
    /// the sea's: its ripples and short waves it has always.
    pub waves: f32,
    /// The colour of its deep water, sRGB.
    pub deep: [f32; 3],
    /// The colour light takes through its ripples, sRGB.
    pub scatter: [f32; 3],
    /// What a metre of it lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
}

impl Component for Pool {}

impl Default for Pool {
    fn default() -> Self {
        Self {
            size: [10.0, 10.0],
            depth: 4.0,
            liquid: Liquid::Water,
            waves: 0.0,
            deep: [0.02, 0.18, 0.2],
            scatter: [0.15, 0.6, 0.5],
            clarity: [0.75, 0.93, 0.9],
            murk: 1.0,
        }
    }
}

/// Keeps water out of its shape: a box `size` across, a ball as wide as
/// `size.x`, or a capsule as wide as `size.x` and as tall as `size.y`,
/// turned with its Obj.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DryArea {
    pub shape: DryShape,
    /// In metres.
    pub size: [f32; 3],
}

impl Component for DryArea {}

impl Default for DryArea {
    fn default() -> Self {
        Self { shape: DryShape::Box, size: [2.0, 2.0, 4.0] }
    }
}

impl DryArea {
    /// How far `local` (in the Obj's space, unscaled) is outside the shape,
    /// negative inside.
    pub fn distance(&self, local: Vec3) -> f32 {
        let half = Vec3::from(self.size) * 0.5;
        match self.shape {
            DryShape::Box => {
                let q = local.abs() - half;
                q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
            }
            DryShape::Sphere => local.length() - half.x,
            DryShape::Capsule => {
                let reach = (half.y - half.x).max(0.0);
                let along = Vec3::new(local.x, local.y.clamp(-reach, reach), local.z);
                (local - Vec3::new(0.0, along.y, 0.0)).length() - half.x
            }
        }
    }
}

/// A body placed: where its Obj is and how it is turned.
fn placement(position: Vec3, rotation: Quat) -> (Mat4, Mat4) {
    let to_world = Mat4::from_rotation_translation(rotation, position);
    (to_world, to_world.inverse())
}

impl Pool {
    pub(crate) fn gpu(&self, position: Vec3, rotation: Quat) -> GpuWaterBody {
        let linear = |srgb: [f32; 3]| srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        let (to_world, to_local) = placement(position, rotation);
        GpuWaterBody {
            to_world: cols(to_world),
            to_local: cols(to_local),
            size: [self.size[0].max(0.01), self.depth.max(0.01), self.size[1].max(0.01)],
            kind: WATER_POOL,
            deep: linear(self.deep),
            shape: 0,
            scatter: linear(self.scatter),
            waves: self.waves.max(0.0),
            absorb: linear(self.clarity).map(|t| -t.max(1e-3).ln() * self.murk.max(0.0)),
            liquid: self.liquid as u32,
        }
    }

    /// Its surface's height over `point`, when the point is over its
    /// footprint and not below its bottom.
    pub fn surface_over(&self, position: Vec3, rotation: Quat, point: Vec3) -> Option<f32> {
        let local = rotation.inverse() * (point - position);
        let inside = local.x.abs() <= self.size[0] * 0.5 && local.z.abs() <= self.size[1] * 0.5;
        (inside && local.y >= -self.depth).then_some(position.y)
    }
}

impl DryArea {
    pub(crate) fn gpu(&self, position: Vec3, rotation: Quat) -> GpuWaterBody {
        let (to_world, to_local) = placement(position, rotation);
        GpuWaterBody {
            to_world: cols(to_world),
            to_local: cols(to_local),
            size: self.size.map(|s| s.max(0.01)),
            kind: WATER_DRY,
            shape: self.shape as u32,
            ..GpuWaterBody::default()
        }
    }

    /// Whether `point` is inside it, placed at `position`, turned by
    /// `rotation`.
    pub fn contains(&self, position: Vec3, rotation: Quat, point: Vec3) -> bool {
        self.distance(rotation.inverse() * (point - position)) < 0.0
    }
}

/// Water running along its Obj's Path, as wide as the Path is at each point,
/// and faster where its bed falls more steeply: a river, a brook, or lava
/// running down a slope.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stream {
    pub liquid: Liquid,
    /// How deep it runs, in metres: deeper water runs faster downhill.
    pub depth: f32,
    /// How fast it flows where its bed is level, in metres a second.
    /// Downhill it runs faster, as rivers do (Manning's formula).
    pub speed: f32,
    /// How steep it runs, as a slope, before it breaks white into rapids.
    pub rapids: f32,
    /// The colour of its deep water, sRGB.
    pub deep: [f32; 3],
    /// The colour light takes through its ripples, sRGB.
    pub scatter: [f32; 3],
    /// What a metre of it lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
}

impl Component for Stream {}

impl Default for Stream {
    fn default() -> Self {
        Self {
            liquid: Liquid::Water,
            depth: 1.0,
            speed: 1.0,
            rapids: 0.1,
            deep: [0.04, 0.17, 0.2],
            scatter: [0.2, 0.55, 0.45],
            clarity: [0.9, 0.96, 0.95],
            murk: 1.0,
        }
    }
}

/// How far white water from rapids is carried on downstream, in metres.
const FOAM_CARRIED: f32 = 6.0;

/// A place along a Stream, in the world: where its surface is in the
/// middle, which way it runs, how wide it is, how fast it flows there and
/// how much it foams.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamPoint {
    pub position: Vec3,
    pub tangent: Vec3,
    pub width: f32,
    pub speed: f32,
    pub foam: f32,
}

/// How rough a river's bed is, in Manning's formula: a natural channel's.
const MANNING_ROUGHNESS: f32 = 0.05;

impl Stream {
    /// Its points along `path`, placed by `to_world` (its Obj's matrix).
    pub fn points(&self, path: &Path, to_world: Mat4) -> Vec<StreamPoint> {
        let rough = path.samples(to_world, STREAM_SPACING);
        let length = rough.last().map_or(0.0, |s| s.along);
        let samples = match rough.len() > STREAM_POINTS {
            true => path.samples(to_world, length / (STREAM_POINTS - 1) as f32 * 1.01),
            false => rough,
        };
        let mut points: Vec<StreamPoint> = samples
            .into_iter()
            .take(STREAM_POINTS)
            .map(|s| {
                let level = Vec3::new(s.tangent.x, 0.0, s.tangent.z).length().max(1e-3);
                let slope = (-s.tangent.y / level).max(0.0);
                // Manning: v = R^(2/3) S^(1/2) / n, the hydraulic radius as deep as a wide river is.
                let downhill = self.depth.max(0.01).powf(2.0 / 3.0) * slope.sqrt() / MANNING_ROUGHNESS;
                let rapids = self.rapids.max(1e-3);
                StreamPoint {
                    position: s.position,
                    tangent: s.tangent,
                    width: s.width.max(0.05),
                    speed: self.speed.max(0.0).max(downhill).min(20.0),
                    foam: ((slope - rapids) / rapids).clamp(0.0, 1.0),
                }
            })
            .collect();
        // White water churns on past the foot of the rapids, fading as it goes.
        for i in 1..points.len() {
            let fade = (-points[i].position.distance(points[i - 1].position) / FOAM_CARRIED).exp();
            points[i].foam = points[i].foam.max(points[i - 1].foam * fade);
        }
        points
    }

    pub(crate) fn gpu(&self, first: u32) -> GpuWaterBody {
        let linear = |srgb: [f32; 3]| srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        GpuWaterBody {
            to_world: cols(Mat4::IDENTITY),
            to_local: cols(Mat4::IDENTITY),
            size: [0.0, self.depth.max(0.01), 0.0],
            kind: WATER_STREAM,
            deep: linear(self.deep),
            shape: first,
            scatter: linear(self.scatter),
            waves: 0.0,
            absorb: linear(self.clarity).map(|t| -t.max(1e-3).ln() * self.murk.max(0.0)),
            liquid: self.liquid as u32,
        }
    }
}

impl StreamPoint {
    pub(crate) fn gpu(&self) -> GpuStreamPoint {
        GpuStreamPoint {
            position: self.position.to_array(),
            width: self.width,
            tangent: self.tangent.to_array(),
            speed: self.speed,
            foam: self.foam,
            _pad: [0.0; 3],
        }
    }
}

/// Where a Stream is over `point`: its surface's height there, and how it
/// flows, slower towards its banks; `None` away from it or below its bed.
pub fn stream_over(points: &[StreamPoint], depth: f32, point: Vec3) -> Option<(f32, Vec3)> {
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let mut best: Option<(f32, f32, Vec3)> = None;
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let run = flat(b.position - a.position);
        let t = (flat(point - a.position).dot(run) / run.length_squared().max(1e-6)).clamp(0.0, 1.0);
        let middle = a.position.lerp(b.position, t);
        let off = flat(point - middle).length();
        let half = (a.width + (b.width - a.width) * t) * 0.5;
        if off > half || best.is_some_and(|(d, ..)| d <= off) {
            continue;
        }
        let across = off / half.max(1e-3);
        // As fast as it runs in the middle, a third of that at its banks.
        let speed = (a.speed + (b.speed - a.speed) * t) * (0.3 + 0.7 * (1.0 - across * across));
        let tangent = a.tangent.lerp(b.tangent, t).normalize_or(Vec3::Z);
        best = Some((off, middle.y, tangent * speed));
    }
    best.filter(|&(_, height, _)| point.y >= height - depth).map(|(_, height, flow)| (height, flow))
}

/// A body's kind, as `WATER_POOL`, `WATER_DRY` and `WATER_STREAM` in
/// `shaders/water.slang`.
pub const WATER_POOL: u32 = 1;
pub const WATER_DRY: u32 = 2;
pub const WATER_STREAM: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dry_area_keeps_water_out_of_its_shape_turned_with_its_obj() {
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let hull = DryArea { shape: DryShape::Box, size: [2.0, 2.0, 6.0] };
        // Six metres long along its own z, which the turn lays along the world's x.
        assert!(hull.contains(Vec3::ZERO, turned, Vec3::new(2.5, 0.0, 0.0)));
        assert!(!hull.contains(Vec3::ZERO, turned, Vec3::new(0.0, 0.0, 2.5)));
        let ball = DryArea { shape: DryShape::Sphere, size: [4.0, 4.0, 4.0] };
        assert!(ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(11.9, 0.0, 0.0)));
        assert!(!ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(12.1, 0.0, 0.0)));
        let bell = DryArea { shape: DryShape::Capsule, size: [2.0, 6.0, 2.0] };
        assert!(bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(0.0, 2.8, 0.0)) && !bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(1.2, 0.0, 0.0)));
    }

    #[test]
    fn a_stream_runs_faster_downhill_foams_on_steep_ground_and_slower_at_its_banks() {
        use voxear_scene::PathPoint;
        let point = |y: f32, z: f32| PathPoint { position: [0.0, y, z], width: 4.0 };
        let path = Path { points: vec![point(0.0, 0.0), point(0.0, 20.0), point(-1.0, 30.0), point(-5.0, 40.0)], closed: false };
        let points = Stream::default().points(&path, Mat4::IDENTITY);
        let at = |z: f32| points.iter().min_by(|a, b| (a.position.z - z).abs().total_cmp(&(b.position.z - z).abs())).unwrap();
        assert!((at(10.0).speed - 1.0).abs() < 1e-3, "level, its own speed: {}", at(10.0).speed);
        assert!(at(25.0).speed > 2.0 && at(25.0).foam == 0.0, "downhill at 1 in 10: {:?}", at(25.0));
        assert!(at(10.0).foam == 0.0 && at(19.0).foam == 0.0, "no white water above the rapids");
        assert!(at(36.0).speed > at(25.0).speed && at(36.0).foam > 0.5, "steeper, rapids: {:?}", at(36.0));
        let (height, middle) = stream_over(&points, 1.0, Vec3::new(0.0, -0.5, 10.0)).expect("over it");
        assert!(height.abs() < 0.1 && (middle.length() - 1.0).abs() < 0.02 && middle.z > 0.99, "{height} {middle}");
        let (_, bank) = stream_over(&points, 1.0, Vec3::new(1.9, 0.0, 10.0)).unwrap();
        assert!(bank.length() < 0.5, "slower at its bank: {bank}");
        assert!(stream_over(&points, 1.0, Vec3::new(2.5, 0.0, 10.0)).is_none(), "past its bank");
        assert!(stream_over(&points, 1.0, Vec3::new(0.0, -2.0, 10.0)).is_none(), "under its bed");
    }

    #[test]
    fn a_pools_surface_is_over_its_footprint_at_its_height() {
        let pond = Pool { size: [4.0, 10.0], ..Pool::default() };
        let at = Vec3::new(0.0, 2.0, 0.0);
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(1.5, -1.0, 4.5)), Some(2.0));
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(2.5, 0.0, 0.0)), None);
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(0.0, -3.0, 0.0)), None, "under its bottom, 4 m down");
    }
}
