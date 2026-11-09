//! Bodies of water besides the sea: a Pool fills a box of ground with water
//! (or lava) at its Obj's height, as a lake, a pond or a flooded room; a Dry
//! Area keeps water out of a box, a ball or a capsule, as the inside of a
//! boat's hull, a cave or a diving bell; a Stream runs along its Obj's Path,
//! as a river, a brook or a flow of lava, as wide as the Path is.
//!
//! Each Pool draws its own surface with the sea's smallest waves on it: a
//! small one a grid over its footprint, a large one the sea's rings round
//! the camera, cut off at its sides. Every water surface leaves out what
//! lies inside a Dry Area, pixel by pixel, so a moving boat's hull is dry
//! exactly where it is. A Pool below the sea's level is dry to the sea, so
//! a pit by the shore can hold a pond lower than the sea round it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::Component;
use voxear_scene::Path;

use crate::MeshHandle;
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
    /// The inside of a closed mesh, as a hull's: its own, or the Obj's Mesh
    /// Renderer's.
    Mesh,
}

impl DryShape {
    pub const ALL: [Self; 4] = [Self::Box, Self::Sphere, Self::Capsule, Self::Mesh];

    pub fn name(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Capsule => "Capsule",
            Self::Mesh => "Mesh",
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
    /// Its water's own colour, sRGB, as an Ocean's Deep.
    pub deep: [f32; 3],
    /// The colour light takes through its ripples, sRGB.
    pub scatter: [f32; 3],
    /// What a metre of it lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
    /// Lava's: how many seconds its surface takes to crust over, and how
    /// brightly it glows.
    pub cooling: f32,
    pub glow: f32,
}

impl Component for Pool {}

impl Default for Pool {
    fn default() -> Self {
        Self {
            size: [10.0, 10.0],
            depth: 4.0,
            liquid: Liquid::Water,
            waves: 0.0,
            // A lake's green.
            deep: [0.04, 0.46, 0.33],
            scatter: [0.15, 0.6, 0.5],
            clarity: [0.7, 0.97, 0.93],
            murk: 1.0,
            cooling: 20.0,
            glow: 1.0,
        }
    }
}

/// Keeps water out of its shape: a box `size` across, a ball as wide as
/// `size.x`, or a capsule as wide as `size.x` and as tall as `size.y`,
/// turned with its Obj; or the inside of a closed mesh, placed, turned and
/// scaled with its Obj.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DryArea {
    pub shape: DryShape,
    /// In metres.
    pub size: [f32; 3],
    /// For the Mesh shape: the mesh, or `None` for the Obj's Mesh
    /// Renderer's. Scenes write it as a Mesh Renderer's mesh is written.
    #[serde(skip)]
    pub mesh: Option<MeshHandle>,
    /// For the Mesh shape: the box round its mesh, in the mesh's own space,
    /// which scripts asking about water take as its inside; set where the
    /// mesh is loaded or picked.
    #[serde(skip)]
    pub bounds: Option<[Vec3; 2]>,
    /// How deep the water in it stands when it is level, in metres from its
    /// bottom: a hull's bilge. 0 for none.
    pub bilge: f32,
}

impl Component for DryArea {}

impl Default for DryArea {
    fn default() -> Self {
        Self { shape: DryShape::Box, size: [2.0, 2.0, 4.0], mesh: None, bounds: None, bilge: 0.0 }
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
            // A mesh's inside is known only from the mesh itself (see `water::DryMesh`).
            DryShape::Mesh => f32::INFINITY,
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
            lava: [self.cooling.max(0.1), self.glow.max(0.0), 0.0, 0.0],
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

    /// The Mesh shape's body: `mesh` placed by the Obj's whole matrix,
    /// scale and all, which the shaders read from where a Pool keeps its
    /// liquid.
    pub(crate) fn gpu_mesh(mesh: MeshHandle, model: Mat4) -> GpuWaterBody {
        GpuWaterBody {
            to_world: cols(model),
            to_local: cols(model.inverse()),
            size: [1.0; 3],
            kind: WATER_DRY,
            shape: DryShape::Mesh as u32,
            liquid: mesh.0,
            ..GpuWaterBody::default()
        }
    }

    /// Whether `point` is inside it, placed at `position`, turned by
    /// `rotation`.
    pub fn contains(&self, position: Vec3, rotation: Quat, point: Vec3) -> bool {
        self.distance(rotation.inverse() * (point - position)) < 0.0
    }

    /// For the Mesh shape, whether `point` is inside the box round its mesh,
    /// placed by `model`, its Obj's whole matrix; never before its mesh's
    /// box is known.
    pub fn mesh_contains(&self, model: Mat4, point: Vec3) -> bool {
        let Some([low, high]) = self.bounds else { return false };
        let local = model.inverse().transform_point3(point);
        local.cmpge(low).all() && local.cmple(high).all()
    }

    /// Its bilge placed by `model` (for a Mesh, the Obj's whole matrix;
    /// else its position and rotation): the box round its water, the
    /// surface's height in the middle of its top, as level as the sea
    /// however the hull rocks, the water staying as high over its middle.
    pub fn bilge(&self, model: Mat4) -> Option<[Vec3; 2]> {
        let [low, high] = match self.shape {
            DryShape::Mesh => self.bounds?,
            DryShape::Box => [Vec3::from(self.size) * -0.5, Vec3::from(self.size) * 0.5],
            DryShape::Sphere => [Vec3::splat(self.size[0] * -0.5), Vec3::splat(self.size[0] * 0.5)],
            DryShape::Capsule => {
                let half = Vec3::new(self.size[0], self.size[1].max(self.size[0]), self.size[0]) * 0.5;
                [-half, half]
            }
        };
        let tall = (high.y - low.y) * model.y_axis.truncate().length();
        if self.bilge <= 0.0 || tall <= 0.0 {
            return None;
        }
        let level = model.transform_point3((low + high) * 0.5).y - tall * 0.5 + self.bilge.min(tall);
        let pick = |on: bool, high: f32, low: f32| if on { high } else { low };
        let corners =
            (0..8).map(|i| Vec3::new(pick(i & 1 != 0, high.x, low.x), pick(i & 2 != 0, high.y, low.y), pick(i & 4 != 0, high.z, low.z)));
        let (bottom, top) = corners
            .map(|corner| model.transform_point3(corner))
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(bottom, top), p| (bottom.min(p), top.max(p)));
        (level > bottom.y).then(|| [bottom, Vec3::new(top.x, level, top.z)])
    }

    /// Its bilge as the shaders draw it: a Pool of murky water over the box
    /// round it, which keeps to the inside of the Dry Area its `shape`
    /// names, plus one, once the bodies are listed.
    pub(crate) fn gpu_bilge(&self, model: Mat4) -> Option<GpuWaterBody> {
        let [bottom, top] = self.bilge(model)?;
        let pool = Pool { size: [top.x - bottom.x, top.z - bottom.z], depth: top.y - bottom.y, murk: 4.0, ..Pool::default() };
        Some(pool.gpu(Vec3::new((bottom.x + top.x) * 0.5, top.y, (bottom.z + top.z) * 0.5), Quat::IDENTITY))
    }

    /// The water in its bilge at `point`, inside it, placed by `model` as
    /// for [`DryArea::bilge`]: the surface's height, when the point is
    /// under it.
    pub fn bilge_over(&self, model: Mat4, point: Vec3) -> Option<f32> {
        self.bilge(model).map(|[_, top]| top.y).filter(|&level| point.y < level)
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
    /// Its water's own colour, sRGB, as an Ocean's Deep.
    pub deep: [f32; 3],
    /// The colour light takes through its ripples, sRGB.
    pub scatter: [f32; 3],
    /// What a metre of it lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
    /// Lava's: how many seconds it runs before it crusts over, and how
    /// brightly it glows.
    pub cooling: f32,
    pub glow: f32,
}

impl Component for Stream {}

impl Default for Stream {
    fn default() -> Self {
        Self {
            liquid: Liquid::Water,
            depth: 1.0,
            speed: 1.0,
            rapids: 0.1,
            deep: [0.05, 0.36, 0.3],
            scatter: [0.2, 0.55, 0.45],
            clarity: [0.9, 0.96, 0.95],
            murk: 1.0,
            cooling: 20.0,
            glow: 1.0,
        }
    }
}

/// How far white water from rapids is carried on downstream, in metres.
const FOAM_CARRIED: f32 = 6.0;

/// A place along a Stream, in the world: where its surface is in the
/// middle, which way it runs, how wide it is, how fast it flows there, how
/// much it foams, and how long what flows there has taken to come from its
/// start, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamPoint {
    pub position: Vec3,
    pub tangent: Vec3,
    pub width: f32,
    pub speed: f32,
    pub foam: f32,
    pub age: f32,
    /// Where it runs into the sea or a Pool at its end, how far into its
    /// mouth this point is, from 0 upstream to 1 at the end, and how high the
    /// water it runs into stands.
    pub mouth: f32,
    pub meets: f32,
}

/// A Stream of water, its points, and where it pours into the sea or a
/// Pool if it does: as [`join_streams`] takes them.
pub type Joining = (bool, Vec<StreamPoint>, Option<usize>);

/// Where Streams of water meet. One ending in another that runs on past it
/// is a tributary: it eases into the river over its mouth, as into the sea,
/// and the river churns white below where it joins. Two ending in each
/// other meet head on: each fades out into the other over its last points,
/// and both churn white where they meet. A Stream already pouring into the
/// sea or a Pool, and lava, are left as they are.
pub fn join_streams(streams: &mut [Joining]) {
    for a in 0..streams.len() {
        let (water, ref points, pours) = streams[a];
        let Some(end) = points.last().copied().filter(|_| water && pours.is_none()) else { continue };
        let mut nearest: Option<(usize, usize, f32)> = None;
        for (b, (other_water, others, _)) in streams.iter().enumerate() {
            if b == a || !other_water {
                continue;
            }
            for (j, p) in others.iter().enumerate() {
                let across = (p.position - end.position).with_y(0.0).length();
                let close = across < p.width * 0.5 + end.width * 0.25 && (p.position.y - end.position.y).abs() < 1.5;
                if close && nearest.is_none_or(|(.., best)| across < best) {
                    nearest = Some((b, j, across));
                }
            }
        }
        let Some((b, j, _)) = nearest else { continue };
        if j + 2 < streams[b].1.len() {
            // Just under the river's surface, which then covers it where they overlap.
            let river = streams[b].1[j];
            streams[a].2 = ease_into(&mut streams[a].1, river.position.y - 0.1, river.width * 0.5 + 2.0);
            churn(&mut streams[b].1, j, 0.7);
        } else {
            let last = streams[a].1.len().saturating_sub(2);
            churn(&mut streams[a].1, last, 0.8);
            churn(&mut streams[b].1, j, 0.8);
        }
    }
}

/// Eases a Stream along `points` into water standing `level` high over its
/// last `length` metres, as a tributary into a river: which point that
/// starts at.
fn ease_into(points: &mut [StreamPoint], level: f32, length: f32) -> Option<usize> {
    let mut left = 0.0;
    let mut from = points.len().checked_sub(1)?;
    while from > 0 && left < length {
        left += points[from].position.distance(points[from - 1].position);
        from -= 1;
    }
    let total = left.max(1e-3);
    let mut along = 0.0;
    for i in from..points.len() {
        if i > from {
            along += points[i].position.distance(points[i - 1].position);
        }
        let t = (along / total).min(1.0);
        points[i].mouth = t * t * (3.0 - 2.0 * t);
        points[i].meets = level;
    }
    Some(from)
}

/// White water from point `from` of a Stream on down it, as `strength`
/// says there and fading as it is carried.
fn churn(points: &mut [StreamPoint], from: usize, strength: f32) {
    let mut along = 0.0;
    for i in from..points.len() {
        if i > from {
            along += points[i].position.distance(points[i - 1].position);
        }
        points[i].foam = points[i].foam.max(strength * (-along / FOAM_CARRIED).exp());
    }
}

/// How long a Stream's mouth is, in metres: how far it takes, from where it
/// reaches the water it runs into, to become that water.
pub const MOUTH_LENGTH: f32 = 10.0;

/// Eases a Stream along `points` into water standing `level` high, from the
/// first point that reaches it on, over [`MOUTH_LENGTH`] metres; which point
/// that is, where it pours in, if any does.
pub fn run_into(points: &mut [StreamPoint], level: f32) -> Option<usize> {
    let reaches = points.iter().position(|p| p.position.y < level + 1.0)?;
    let mut along = 0.0;
    for i in reaches..points.len() {
        if i > reaches {
            along += points[i].position.distance(points[i - 1].position);
        }
        let t = (along / MOUTH_LENGTH).min(1.0);
        points[i].mouth = t * t * (3.0 - 2.0 * t);
        points[i].meets = level;
    }
    Some(reaches)
}

/// How rough a river's bed is, in Manning's formula: a natural channel's;
/// and lava's, as thick as it is, running a dozen times slower.
const MANNING_ROUGHNESS: f32 = 0.05;
const LAVA_ROUGHNESS: f32 = 0.6;

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
                let lava = self.liquid == Liquid::Lava;
                let roughness = if lava { LAVA_ROUGHNESS } else { MANNING_ROUGHNESS };
                let downhill = self.depth.max(0.01).powf(2.0 / 3.0) * slope.sqrt() / roughness;
                let rapids = self.rapids.max(1e-3);
                StreamPoint {
                    position: s.position,
                    tangent: s.tangent,
                    width: s.width.max(0.05),
                    speed: self.speed.max(0.0).max(downhill).min(20.0),
                    foam: if lava { 0.0 } else { ((slope - rapids) / rapids).clamp(0.0, 1.0) },
                    age: 0.0,
                    mouth: 0.0,
                    meets: 0.0,
                }
            })
            .collect();
        // How long the middle of it takes to come this far.
        for i in 1..points.len() {
            let run = points[i].position.distance(points[i - 1].position);
            points[i].age = points[i - 1].age + run / ((points[i].speed + points[i - 1].speed) * 0.5).max(0.02);
        }
        // White water churns on past the foot of the rapids, fading as it goes.
        for i in 1..points.len() {
            let fade = (-points[i].position.distance(points[i - 1].position) / FOAM_CARRIED).exp();
            points[i].foam = points[i].foam.max(points[i - 1].foam * fade);
        }
        points
    }

    /// As the shaders read it, its `count` points from point `first`, and
    /// which of them it pours into the sea or a Pool at, if it does.
    pub(crate) fn gpu(&self, first: u32, count: u32, pours: Option<usize>) -> GpuWaterBody {
        let linear = |srgb: [f32; 3]| srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        GpuWaterBody {
            to_world: cols(Mat4::IDENTITY),
            to_local: cols(Mat4::IDENTITY),
            size: [count as f32, self.depth.max(0.01), pours.map_or(-1.0, |at| at as f32)],
            kind: WATER_STREAM,
            deep: linear(self.deep),
            shape: first,
            scatter: linear(self.scatter),
            waves: 0.0,
            absorb: linear(self.clarity).map(|t| -t.max(1e-3).ln() * self.murk.max(0.0)),
            liquid: self.liquid as u32,
            lava: [self.cooling.max(0.1), self.glow.max(0.0), 0.0, 0.0],
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
            age: self.age,
            mouth: self.mouth,
            meets: self.meets,
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

/// A body's kind, as `WATER_POOL`, `WATER_DRY`, `WATER_STREAM` and `WATER_WIND` in
/// `shaders/water.slang`.
pub const WATER_POOL: u32 = 1;
pub const WATER_DRY: u32 = 2;
pub const WATER_STREAM: u32 = 3;
/// A Sphere or Box Wind Zone, among the Dry Areas, making the sea's short
/// waves rougher or calmer inside it (`WATER_WIND`).
pub const WATER_WIND: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bilge_stays_level_as_its_hull_rocks() {
        let hull = DryArea { size: [2.0, 2.0, 6.0], bilge: 0.5, ..DryArea::default() };
        let at = |roll: f32| Mat4::from_rotation_translation(Quat::from_rotation_z(roll), Vec3::new(0.0, 3.0, 0.0));
        let [bottom, top] = hull.bilge(at(0.0)).unwrap();
        assert_eq!((bottom.y, top.y), (2.0, 2.5), "half a metre over its bottom");
        let [bottom, top] = hull.bilge(at(0.3)).unwrap();
        assert_eq!(top.y, 2.5, "as high over its middle, rocked");
        assert!(bottom.y < 2.0 && top.x > 1.0, "the box round it, rocked: {bottom} {top}");
        assert_eq!(hull.bilge_over(at(0.0), Vec3::new(0.0, 2.2, 0.0)), Some(2.5));
        assert_eq!(hull.bilge_over(at(0.0), Vec3::new(0.0, 2.7, 0.0)), None, "over its water");
        assert!(DryArea { bilge: 0.0, ..hull }.bilge(at(0.0)).is_none());
    }

    #[test]
    fn a_stream_eases_into_the_water_it_reaches_over_its_mouth() {
        // Down a slope, a point every 4 m, reaching the sea at its level of 0 on its fourth point.
        let point = |i: usize| StreamPoint {
            position: Vec3::new(0.0, 6.0 - 2.0 * i as f32, 4.0 * i as f32),
            tangent: Vec3::Z,
            width: 4.0,
            speed: 2.0,
            foam: 0.0,
            age: 0.0,
            mouth: 0.0,
            meets: 0.0,
        };
        let mut points: Vec<StreamPoint> = (0..8).map(point).collect();
        assert_eq!(run_into(&mut points, 0.0), Some(3), "where it first comes within a metre of the sea");
        assert!(points[..3].iter().all(|p| p.mouth == 0.0), "upstream, its own");
        assert!(points[4].mouth > 0.0 && points[4].mouth < 1.0 && points[4].meets == 0.0, "{:?}", points[4]);
        assert_eq!(points[7].mouth, 1.0, "the sea's, ten metres on");
        let mut high: Vec<StreamPoint> = (0..2).map(point).collect();
        assert_eq!(run_into(&mut high, 0.0), None, "never reaching it");
    }

    #[test]
    fn a_brook_eases_into_the_river_it_joins_and_streams_meeting_head_on_churn() {
        let point = |x: f32, z: f32| StreamPoint {
            position: Vec3::new(x, 0.0, z),
            tangent: Vec3::Z,
            width: 4.0,
            speed: 2.0,
            foam: 0.0,
            age: 0.0,
            mouth: 0.0,
            meets: 0.0,
        };
        // A river north along x = 0, and a brook from the west ending in its middle.
        let river: Vec<StreamPoint> = (0..10).map(|i| point(0.0, i as f32 * 4.0)).collect();
        let brook: Vec<StreamPoint> = (0..5).map(|i| point(-16.0 + i as f32 * 4.0, 18.0)).collect();
        let mut streams = vec![(true, river, None), (true, brook, None)];
        join_streams(&mut streams);
        let (river, brook) = (&streams[0].1, &streams[1].1);
        assert!(streams[1].2.is_some() && brook[4].mouth == 1.0 && brook[0].mouth == 0.0, "the brook eases in at its end");
        assert!(brook[4].meets < 0.0, "just under the river's surface: {}", brook[4].meets);
        assert!(river[3].foam == 0.0 && river[4].foam > 0.5 && river[9].foam < river[4].foam, "white below the join");
        assert!(streams[0].2.is_none(), "the river runs on");
        // Two streams ending in each other.
        let east: Vec<StreamPoint> = (0..4).map(|i| point(i as f32 * 4.0, 0.0)).collect();
        let west: Vec<StreamPoint> = (0..4).map(|i| point(24.0 - i as f32 * 4.0, 0.0)).collect();
        let mut streams = vec![(true, east, None), (true, west, None)];
        join_streams(&mut streams);
        assert!(streams.iter().all(|(_, points, pours)| pours.is_none() && points[3].foam > 0.5), "both churn where they meet");
    }

    #[test]
    fn a_dry_area_keeps_water_out_of_its_shape_turned_with_its_obj() {
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let hull = DryArea { shape: DryShape::Box, size: [2.0, 2.0, 6.0], ..DryArea::default() };
        // Six metres long along its own z, which the turn lays along the world's x.
        assert!(hull.contains(Vec3::ZERO, turned, Vec3::new(2.5, 0.0, 0.0)));
        assert!(!hull.contains(Vec3::ZERO, turned, Vec3::new(0.0, 0.0, 2.5)));
        let ball = DryArea { shape: DryShape::Sphere, size: [4.0, 4.0, 4.0], ..DryArea::default() };
        assert!(ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(11.9, 0.0, 0.0)));
        assert!(!ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(12.1, 0.0, 0.0)));
        let bell = DryArea { shape: DryShape::Capsule, size: [2.0, 6.0, 2.0], ..DryArea::default() };
        assert!(bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(0.0, 2.8, 0.0)) && !bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(1.2, 0.0, 0.0)));
    }

    #[test]
    fn a_mesh_dry_area_keeps_water_out_of_the_box_round_its_mesh_placed_by_its_obj() {
        let hull = DryArea { shape: DryShape::Mesh, bounds: Some([Vec3::splat(-0.5), Vec3::splat(0.5)]), ..DryArea::default() };
        let model = Mat4::from_scale_rotation_translation(Vec3::new(4.0, 2.0, 10.0), Quat::IDENTITY, Vec3::new(0.0, 1.0, 0.0));
        assert!(hull.mesh_contains(model, Vec3::new(1.5, 0.5, 4.5)));
        assert!(!hull.mesh_contains(model, Vec3::new(2.5, 0.5, 0.0)));
        assert!(!DryArea { bounds: None, ..hull }.mesh_contains(model, Vec3::new(0.0, 1.0, 0.0)));
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
        assert!((at(10.0).age - 10.0).abs() < 0.5, "ten metres at a metre a second: {}", at(10.0).age);
        let lava = Stream { liquid: Liquid::Lava, ..Stream::default() }.points(&path, Mat4::IDENTITY);
        let steep = lava.iter().find(|p| (p.position.z - 36.0).abs() < 0.6).unwrap();
        assert!(steep.speed < at(36.0).speed / 5.0 && steep.foam == 0.0, "lava runs slower, and never white: {steep:?}");
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
