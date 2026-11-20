//! Roads and Area Fills: world layers on Objs (see `ground_fit` and
//! `voxear_terrain::blocks::world_layers`) that lay a world of blocks
//! without end's ground with blocks along a line or over an area, without
//! storing blocks.
//!
//! ```toml
//! [obj.road]
//! points = [{ at = [0.0, 0.0, 0.0], width = 5.0 }, { at = [40.0, 2.0, 10.0], width = 9.0 }]
//! mix = [{ block = "…", weight = 3 }, { block = "…", weight = 1 }]
//! pattern = ["1112111"]     # a kerb each side, a middle strip: entries of the mix by place
//! ground = "grade"          # or "follow"
//! grade = 0.25              # the most it climbs a block along
//!
//! [obj.area_fill]
//! outline = [[-8.0, -8.0], [8.0, -8.0], [8.0, 8.0], [-8.0, 8.0]]
//! level = "follow"          # or "level", at its Obj's height
//! ```

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_terrain::blocks::{
    chunk::{AIR, BlockId},
    structure::GroundFit as FitMode,
    world_layers::{Bridge, FitLayer, Palette, RoadLayer, Steps, Tunnel},
};

use crate::ground_fit::WorldPlace;

/// A point of a road, in its Obj's own space, and how wide the road is there
/// in blocks.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RoadPoint {
    pub at: [f32; 3],
    pub width: f32,
}

impl Default for RoadPoint {
    fn default() -> Self {
        Self { at: [0.0; 3], width: 5.0 }
    }
}

/// A block of a mix, as often as its weight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MixBlock {
    pub block: Option<AssetGuid>,
    pub weight: u32,
}

impl Default for MixBlock {
    fn default() -> Self {
        Self { block: None, weight: 1 }
    }
}

/// How a road meets the ground.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoadGround {
    /// Laid on the ground as it is.
    Follow,
    /// The ground cut and filled to a line between its points, climbing no
    /// more than its grade.
    #[default]
    Grade,
}

impl RoadGround {
    pub const ALL: [RoadGround; 2] = [RoadGround::Follow, RoadGround::Grade];

    pub fn name(self) -> &'static str {
        match self {
            RoadGround::Follow => "Follow",
            RoadGround::Grade => "Grade",
        }
    }
}

/// How a graded road climbs.
pub type RoadSteps = Steps;

/// A road, a path or a canal along points the Obj holds (see the module's
/// own words).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Road {
    pub points: Vec<RoadPoint>,
    /// The blocks it is laid with, each as often as its weight; none for the
    /// ground's own top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mix: Vec<MixBlock>,
    /// A small picture of the mix's entries by place from 1, a row a block
    /// along and a character a block across, repeated; 0 for the mix.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pattern: Vec<String>,
    /// The block along each edge, and how many blocks its kerb stands over
    /// the road.
    pub edge: Option<AssetGuid>,
    pub kerb: u32,
    pub ground: RoadGround,
    /// The most it climbs a block along, in blocks, graded.
    pub grade: f32,
    /// Over how many blocks each side the ground eases back, graded.
    pub slope_out: u32,
    pub steps: RoadSteps,
    /// Over ground or water further under it than `over_water` blocks, a
    /// deck of `deck` on a pier every `pier_every` blocks.
    pub bridge: bool,
    pub deck: Option<AssetGuid>,
    pub over_water: u32,
    pub pier_every: u32,
    /// Through ground more than `cover` blocks over it, cut `tunnel_height`
    /// high with a roof of `roof`.
    pub tunnel: bool,
    pub cover: u32,
    pub tunnel_height: u32,
    pub roof: Option<AssetGuid>,
    /// Water in it, `depth` blocks deep, as a canal.
    pub water: Option<AssetGuid>,
    pub depth: u32,
    pub clear_plants: bool,
    pub priority: i32,
}

impl voxear_ecs::Component for Road {}

impl Default for Road {
    fn default() -> Self {
        Self {
            points: vec![RoadPoint::default(), RoadPoint { at: [0.0, 0.0, 16.0], width: 5.0 }],
            mix: Vec::new(),
            pattern: Vec::new(),
            edge: None,
            kerb: 0,
            ground: RoadGround::Grade,
            grade: 0.25,
            slope_out: 3,
            steps: Steps::Blocks,
            bridge: false,
            deck: None,
            over_water: 3,
            pier_every: 8,
            tunnel: false,
            cover: 4,
            tunnel_height: 4,
            roof: None,
            water: None,
            depth: 2,
            clear_plants: true,
            priority: 0,
        }
    }
}

/// A palette of a mix and a pattern, its blocks numbered as `number` says.
fn palette(mix: &[MixBlock], pattern: &[String], number: &dyn Fn(AssetGuid) -> Option<BlockId>) -> Palette {
    let mix: Vec<(BlockId, u32)> =
        mix.iter().filter_map(|entry| Some((entry.block.and_then(number)?, entry.weight))).collect();
    let pattern = pattern
        .iter()
        .map(|row| row.chars().filter_map(|c| c.to_digit(10)).map(|digit| digit as u8).collect())
        .filter(|row: &Vec<u8>| !row.is_empty())
        .collect();
    Palette { mix, pattern }
}

impl Road {
    /// Its points in the world, for its Obj placed by `placed`.
    pub fn world_points(&self, placed: &Mat4) -> Vec<Vec3> {
        self.points.iter().map(|point| placed.transform_point3(Vec3::from(point.at))).collect()
    }

    /// It as a world layer of a world lying at `place`, its blocks by
    /// `number`: none with fewer than two points.
    pub fn layer(
        &self,
        placed: &Mat4,
        place: WorldPlace,
        number: &dyn Fn(AssetGuid) -> Option<BlockId>,
    ) -> Option<RoadLayer> {
        if self.points.len() < 2 {
            return None;
        }
        let size = place.block_size.max(0.01);
        let points = self
            .world_points(placed)
            .into_iter()
            .map(|at| {
                let local = (at - place.corner) / size;
                [local.x, local.y.round(), local.z]
            })
            .collect();
        let block = |guid: Option<AssetGuid>| guid.and_then(number).unwrap_or(AIR);
        Some(RoadLayer {
            id: 0,
            points,
            widths: self.points.iter().map(|point| point.width.clamp(1.0, 256.0)).collect(),
            palette: palette(&self.mix, &self.pattern, number),
            edge: block(self.edge),
            kerb: self.kerb.min(4),
            grade: (self.ground == RoadGround::Grade).then_some(self.grade.clamp(0.01, 4.0)),
            slope_out: self.slope_out.min(crate::ground_fit::MOST_SLOPE_OUT),
            steps: self.steps,
            bridge: self.bridge.then(|| Bridge {
                deck: block(self.deck),
                over_water: self.over_water,
                pier_every: self.pier_every,
            }),
            tunnel: self.tunnel.then(|| Tunnel {
                cover: self.cover,
                height: self.tunnel_height,
                roof: block(self.roof),
            }),
            water: self.water.and_then(number).map(|water| (water, self.depth.clamp(1, 16))),
            clear_plants: self.clear_plants,
            priority: self.priority,
        })
    }
}

/// What height an Area Fill lays its ground at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AreaLevel {
    /// Level, at its Obj's height.
    Level,
    /// On the ground as it is.
    #[default]
    Follow,
}

impl AreaLevel {
    pub const ALL: [AreaLevel; 2] = [AreaLevel::Level, AreaLevel::Follow];

    pub fn name(self) -> &'static str {
        match self {
            AreaLevel::Level => "Level",
            AreaLevel::Follow => "Follow",
        }
    }
}

/// A square, a plaza or a yard: an outline laid with a mix or a pattern of
/// blocks, level or on the ground as it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AreaFill {
    /// Its outline along x and z, in metres in its Obj's own space.
    pub outline: Vec<[f32; 2]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mix: Vec<MixBlock>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pattern: Vec<String>,
    pub level: AreaLevel,
    /// What made ground is built of; none for the biome's own.
    pub fill: Option<AssetGuid>,
    pub slope_out: u32,
    pub clear_plants: bool,
    pub priority: i32,
}

impl voxear_ecs::Component for AreaFill {}

impl Default for AreaFill {
    fn default() -> Self {
        Self {
            outline: vec![[-6.0, -6.0], [6.0, -6.0], [6.0, 6.0], [-6.0, 6.0]],
            mix: Vec::new(),
            pattern: Vec::new(),
            level: AreaLevel::Follow,
            fill: None,
            slope_out: 3,
            clear_plants: true,
            priority: 0,
        }
    }
}

impl AreaFill {
    /// It as a world layer of a world lying at `place`, its blocks by
    /// `number`: a fit levelling or following the ground, laid with its
    /// palette. None with fewer than three corners.
    pub fn layer(
        &self,
        placed: &Mat4,
        place: WorldPlace,
        number: &dyn Fn(AssetGuid) -> Option<BlockId>,
    ) -> Option<FitLayer> {
        if self.outline.len() < 3 {
            return None;
        }
        let size = place.block_size.max(0.01);
        let corner = |[x, z]: [f32; 2]| {
            let at = (placed.transform_point3(Vec3::new(x, 0.0, z)) - place.corner) / size;
            [at.x, at.z]
        };
        let height = ((placed.w_axis.y - place.corner.y) / size).round() as i32;
        Some(FitLayer {
            id: 0,
            outlines: vec![self.outline.iter().copied().map(corner).collect()],
            level: height,
            mode: FitMode::Flatten,
            margin: 0.0,
            slope_out: self.slope_out.min(crate::ground_fit::MOST_SLOPE_OUT),
            fill: self.fill.and_then(number).unwrap_or(AIR),
            top: AIR,
            clear_plants: self.clear_plants,
            priority: self.priority,
            palette: palette(&self.mix, &self.pattern, number),
            follow: self.level == AreaLevel::Follow,
        })
    }
}

/// The road nearest `at`, within `reach` metres, of those over the world of
/// blocks on `blocks`: its Obj, the nearest point of its middle in the world
/// and how far along it that is, in metres.
pub fn nearest(
    world: &voxear_ecs::World,
    blocks: voxear_ecs::Entity,
    at: Vec3,
    reach: f32,
    skip: Option<voxear_ecs::Entity>,
) -> Option<(voxear_ecs::Entity, Vec3, f32)> {
    let corner = crate::ground_fit::world_corner(world, blocks)?;
    let size = world.get::<crate::blocks::BlockWorldAsset>(blocks)?.file.block_size.max(0.01);
    let place = WorldPlace { corner, block_size: size };
    let here = voxear_scene::dimension_of(world, blocks);
    let local = (at - corner) / size;
    world
        .query_ref::<(voxear_ecs::Entity, &Road, &voxear_scene::WorldTransform)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(obj, ..)| Some(*obj) != skip && voxear_scene::dimension_of(world, *obj) == here)
        .filter_map(|(obj, road, placed)| {
            let layer = road.layer(&placed.0, place, &|_| None)?;
            let (point, along) = layer.nearest_point([local.x, local.z])?;
            let point = corner + Vec3::from(point) * size;
            Some((obj, point, along * size))
        })
        .filter(|(_, point, _)| Vec3::new(point.x - at.x, 0.0, point.z - at.z).length() <= reach)
        .min_by(|a, b| {
            let away = |p: Vec3| Vec3::new(p.x - at.x, 0.0, p.z - at.z).length();
            away(a.1).total_cmp(&away(b.1))
        })
}

/// The world of blocks without end in the dimension of `obj`.
pub fn world_of(world: &voxear_ecs::World, obj: voxear_ecs::Entity) -> Option<voxear_ecs::Entity> {
    let here = voxear_scene::dimension_of(world, obj);
    world
        .query_ref::<(voxear_ecs::Entity, &crate::blocks::BlockWorldAsset)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
        .map(|(entity, _)| entity)
        .find(|entity| voxear_scene::dimension_of(world, *entity) == here)
}

/// Answers a script's question about the world's roads, as JSON: `{ "ask":
/// "nearest_road", "at": [x, y, z], "reach": metres }`, the road nearest a
/// place in the level's dimension, `{ "obj": its number, "point": [x, y, z],
/// "along": metres }`, or nothing where none is within reach.
pub fn answer(world: &mut voxear_ecs::World, text: &str) -> anyhow::Result<String> {
    let asked: serde_json::Value = serde_json::from_str(text)?;
    anyhow::ensure!(asked["ask"] == "nearest_road", "no such question about the world: {}", asked["ask"]);
    let at = asked["at"].as_array().map(|v| v.iter().filter_map(serde_json::Value::as_f64).collect::<Vec<f64>>());
    let Some([x, y, z]) = at.and_then(|at| <[f64; 3]>::try_from(at).ok()) else { anyhow::bail!("`at` is not a place") };
    let reach = asked["reach"].as_f64().unwrap_or(50.0) as f32;
    let at = Vec3::new(x as f32, y as f32, z as f32);
    let blocks = world
        .query_ref::<(voxear_ecs::Entity, &crate::blocks::BlockWorldAsset)>()
        .into_iter()
        .map(|(entity, _)| entity)
        .find(|entity| voxear_scene::dimension_of(world, *entity) == voxear_scene::DimensionId::LEVEL);
    let found = blocks.and_then(|blocks| nearest(world, blocks, at, reach, None));
    Ok(found.map_or_else(String::new, |(obj, point, along)| {
        serde_json::json!({ "obj": obj.to_bits(), "point": point.to_array(), "along": along }).to_string()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_road_is_laid_in_the_worlds_blocks_and_an_area_fill_follows_or_levels() {
        let place = WorldPlace { corner: Vec3::new(10.0, 0.0, 10.0), block_size: 2.0 };
        let placed = Mat4::from_translation(Vec3::new(20.0, 8.0, 20.0));
        let road = Road { pattern: vec!["12 21".into()], ..Road::default() };
        let layer = road.layer(&placed, place, &|_| Some(3)).unwrap();
        assert_eq!(layer.points[0], [5.0, 4.0, 5.0]);
        assert_eq!(layer.points[1], [5.0, 4.0, 13.0]);
        assert_eq!(layer.palette.pattern, vec![vec![1, 2, 2, 1]]);
        assert_eq!(layer.grade, Some(0.25));
        let area = AreaFill { level: AreaLevel::Level, ..AreaFill::default() };
        let fit = area.layer(&placed, place, &|_| None).unwrap();
        assert_eq!((fit.level, fit.follow, fit.mode), (4, false, FitMode::Flatten));
        assert!(AreaFill::default().layer(&placed, place, &|_| None).unwrap().follow);
    }
}
