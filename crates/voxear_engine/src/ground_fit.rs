//! Ground Fit: a building that shapes the ground it stands on. A component
//! on an Obj, so on a prefab's root and every instance of it, that levels
//! the ground of a world of blocks without end under and round the Obj's
//! footprint, as a world layer (see `voxear_terrain::blocks::world_layers`):
//! nothing is stored as blocks, so moving, changing or deleting the Obj makes
//! the ground under its old and new footprint again, and it comes back as
//! the generator made it.
//!
//! ```toml
//! [obj.ground_fit]
//! mode = "flatten"         # foundation, flatten or cut
//! boxes = [{ centre = [0.0, 0.0], size = [8.0, 6.0] }]
//! margin = 1.0             # blocks levelled past the footprint
//! slope_out = 4            # blocks the ground eases back over; 0 for a wall
//! level = "point"          # its own height, or `point` in its own space
//! point = [0.0, 0.0, 3.0]
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::WorldTransform;
use voxear_terrain::blocks::{
    chunk::{AIR, BlockId},
    world_layers::{FitLayer, RoadLayer, WorldLayers},
};

/// How a Ground Fit meets the ground: the same as a generated structure's.
pub type FitMode = voxear_terrain::blocks::structure::GroundFit;

/// What height a Ground Fit levels the ground to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FitLevel {
    /// Its Obj's own height: the Obj stands on the levelled ground.
    #[default]
    Obj,
    /// The height of `point`, in the Obj's own space: its door, so a house
    /// sits level with the street at its door.
    Point,
}

impl FitLevel {
    pub const ALL: [FitLevel; 2] = [FitLevel::Obj, FitLevel::Point];

    pub fn name(self) -> &'static str {
        match self {
            FitLevel::Obj => "Obj",
            FitLevel::Point => "Point",
        }
    }
}

/// A box of a footprint, along x and z in its Obj's own space, in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FitBox {
    pub centre: [f32; 2],
    pub size: [f32; 2],
}

impl Default for FitBox {
    fn default() -> Self {
        Self { centre: [0.0, 0.0], size: [4.0, 4.0] }
    }
}

/// Shapes the ground a world of blocks without end makes under and round its
/// Obj (see the module's own words).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GroundFit {
    pub mode: FitMode,
    /// Its footprint: boxes, and an outline besides, in the Obj's own space,
    /// turned and moved with it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub boxes: Vec<FitBox>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<[f32; 2]>,
    pub level: FitLevel,
    pub point: [f32; 3],
    /// How many blocks into the ground it stands: the ground is levelled
    /// that many blocks over its height.
    pub sink: i32,
    /// How far past the footprint the ground is levelled, in blocks.
    pub margin: f32,
    /// Over how many blocks the ground eases back to the generator's, a
    /// block a step, walkable; 0 for a sheer edge, as a retaining wall.
    pub slope_out: u32,
    /// The blocks made ground is built of and topped with; none for its
    /// biome's own. A block the world does not hold is its biome's own too.
    pub fill: Option<AssetGuid>,
    pub top: Option<AssetGuid>,
    /// Whether generated trees, plants and structures are left out of its
    /// reach.
    pub clear_plants: bool,
    /// Where two fits meet, the higher priority's ground wins; between two
    /// alike, the one that applies last is the same on every machine.
    pub priority: i32,
}

impl voxear_ecs::Component for GroundFit {}

impl Default for GroundFit {
    fn default() -> Self {
        Self {
            mode: FitMode::Flatten,
            boxes: vec![FitBox::default()],
            outline: Vec::new(),
            level: FitLevel::Obj,
            point: [0.0; 3],
            sink: 0,
            margin: 1.0,
            slope_out: 4,
            fill: None,
            top: None,
            clear_plants: true,
            priority: 0,
        }
    }
}

/// Where a world of blocks lies: its corner and how large its blocks are.
#[derive(Clone, Copy, Debug)]
pub struct WorldPlace {
    pub corner: Vec3,
    pub block_size: f32,
}

impl WorldPlace {
    /// A place in the world, in its blocks along x and z.
    fn blocks_of(&self, at: Vec3) -> [f32; 2] {
        let size = self.block_size.max(0.01);
        [(at.x - self.corner.x) / size, (at.z - self.corner.z) / size]
    }
}

impl GroundFit {
    /// Its footprint's outlines in the world's blocks, for its Obj placed by
    /// `placed`.
    pub fn outlines(&self, placed: &Mat4, place: WorldPlace) -> Vec<Vec<[f32; 2]>> {
        let corner = |[x, z]: [f32; 2]| place.blocks_of(placed.transform_point3(Vec3::new(x, 0.0, z)));
        let boxes = self.boxes.iter().map(|fit_box| {
            let ([cx, cz], [sx, sz]) = (fit_box.centre, fit_box.size.map(|v| v.abs() * 0.5));
            [[cx - sx, cz - sz], [cx + sx, cz - sz], [cx + sx, cz + sz], [cx - sx, cz + sz]].map(corner).to_vec()
        });
        let outline = (self.outline.len() >= 3).then(|| self.outline.iter().copied().map(corner).collect());
        boxes.chain(outline).collect()
    }

    /// The first block of air over the ground it levels, in the world's
    /// blocks, for its Obj placed by `placed`.
    pub fn level_of(&self, placed: &Mat4, place: WorldPlace) -> i32 {
        let at = match self.level {
            FitLevel::Obj => placed.w_axis.truncate(),
            FitLevel::Point => placed.transform_point3(Vec3::from(self.point)),
        };
        ((at.y - place.corner.y) / place.block_size.max(0.01)).round() as i32 + self.sink
    }

    /// It as a world layer of a world lying at `place`, its blocks by
    /// `number`: none where it has no footprint.
    pub fn layer(
        &self,
        placed: &Mat4,
        place: WorldPlace,
        number: &dyn Fn(AssetGuid) -> Option<BlockId>,
    ) -> Option<FitLayer> {
        let outlines = self.outlines(placed, place);
        if outlines.is_empty() {
            return None;
        }
        let block = |guid: Option<AssetGuid>| guid.and_then(number).unwrap_or(AIR);
        Some(FitLayer {
            id: 0,
            outlines,
            level: self.level_of(placed, place),
            mode: self.mode,
            margin: self.margin.max(0.0),
            slope_out: self.slope_out.min(MOST_SLOPE_OUT),
            fill: block(self.fill),
            top: block(self.top),
            clear_plants: self.clear_plants,
            priority: self.priority,
            ..FitLayer::default()
        })
    }
}

/// The most blocks a fit eases out over.
pub const MOST_SLOPE_OUT: u32 = 64;

/// Each world's layers as last made, by its Obj, and a number for the fits
/// they were made from, so they are made again only when one changed.
#[derive(Default)]
struct KeptLayers(std::collections::HashMap<Entity, (u64, Arc<WorldLayers>)>);

/// The layers the world of blocks on `entity` was last streamed with.
pub fn kept(world: &World, entity: Entity) -> Option<Arc<WorldLayers>> {
    world.resource::<KeptLayers>()?.0.get(&entity).map(|(_, layers)| layers.clone())
}

/// The layers every Ground Fit switched on in the dimension of the world of
/// blocks on `entity` puts over its ground, the world lying at `place`, its
/// blocks numbered as `number` says: the same `Arc` while nothing changed.
/// `leave_out` is left out, as a fit dropped onto the ground finds the
/// ground without its own cut.
pub fn layers_of(
    world: &mut World,
    entity: Entity,
    place: WorldPlace,
    number: &dyn Fn(AssetGuid) -> Option<BlockId>,
    leave_out: Option<Entity>,
) -> Option<Arc<WorldLayers>> {
    let (fits, roads) = fits_of(world, entity, place, number, leave_out);
    if fits.is_empty() && roads.is_empty() {
        if leave_out.is_none() {
            world.resource_mut::<KeptLayers>().map(|kept| kept.0.remove(&entity));
        }
        return None;
    }
    let bytes: Vec<u8> = fits.iter().flat_map(FitLayer::bytes).chain(roads.iter().flat_map(RoadLayer::bytes)).collect();
    let made_from = voxear_terrain::layers::stable_hash(&bytes);
    if leave_out.is_none() {
        let kept = world.resource::<KeptLayers>().and_then(|kept| kept.0.get(&entity));
        if let Some((_, layers)) = kept.filter(|(was, _)| *was == made_from) {
            return Some(layers.clone());
        }
    }
    let layers = Arc::new(WorldLayers::of(fits, roads));
    if leave_out.is_none() {
        if world.resource::<KeptLayers>().is_none() {
            world.insert_resource(KeptLayers::default());
        }
        let kept = world.resource_mut::<KeptLayers>().expect("put in above");
        kept.0.insert(entity, (made_from, layers.clone()));
    }
    Some(layers)
}

fn fits_of(
    world: &World,
    entity: Entity,
    place: WorldPlace,
    number: &dyn Fn(AssetGuid) -> Option<BlockId>,
    leave_out: Option<Entity>,
) -> (Vec<FitLayer>, Vec<RoadLayer>) {
    let here = voxear_scene::dimension_of(world, entity);
    let hidden = world.resource::<HiddenLayers>().is_some_and(|hidden| hidden.fits);
    if hidden {
        return (Vec::new(), Vec::new());
    }
    let shown = |obj: Entity| {
        Some(obj) != leave_out && voxear_scene::is_active(world, obj) && voxear_scene::dimension_of(world, obj) == here
    };
    let mut fits: Vec<FitLayer> = world
        .query_ref::<(Entity, &GroundFit, &WorldTransform)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(obj, ..)| Some(*obj) != leave_out)
        .filter(|(obj, ..)| voxear_scene::is_active(world, *obj) && voxear_scene::dimension_of(world, *obj) == here)
        .filter_map(|(_, fit, placed)| fit.layer(&placed.0, place, number))
        .collect();
    // Those of Objs kept while their ground is away still shape it.
    let kept = crate::ground_streaming::kept_fits(world, here);
    fits.extend(kept.iter().filter_map(|(fit, placed)| fit.layer(placed, place, number)));
    // Area Fills are fits that lay their tops with a palette (see `roads`).
    let areas = world.query_ref::<(Entity, &crate::roads::AreaFill, &WorldTransform)>();
    let areas = areas.without::<voxear_scene::Destroying>().into_iter().filter(|(obj, ..)| shown(*obj));
    fits.extend(areas.filter_map(|(_, area, placed)| area.layer(&placed.0, place, number)));
    let roads = world.query_ref::<(Entity, &crate::roads::Road, &WorldTransform)>();
    let roads = roads.without::<voxear_scene::Destroying>().into_iter().filter(|(obj, ..)| shown(*obj));
    let roads = roads.filter_map(|(_, road, placed)| road.layer(&placed.0, place, number)).collect();
    (fits, roads)
}

/// World layers of a kind left out of every world's ground, to see the bare
/// ground under them: a view setting, never saved with the scene.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HiddenLayers {
    pub fits: bool,
}

/// The ground the generator and every other layer make at `at`, the first
/// block of air over it, in metres up, for the world of blocks on `entity`
/// lying at `place`, `leave_out`'s own fit left out: where an Obj with a
/// Ground Fit dropped there lands. None where the world has no generator.
pub fn bare_ground(
    world: &mut World,
    content: &mut crate::content::Content,
    entity: Entity,
    at: Vec3,
    leave_out: Option<Entity>,
) -> Option<f32> {
    let asset = world.get::<crate::blocks::BlockWorldAsset>(entity)?.clone();
    let making = crate::blocks::making(content, &asset)?;
    let corner = world_corner(world, entity)?;
    let place = WorldPlace { corner, block_size: asset.file.block_size };
    let drawn = asset.drawn.clone();
    let layers = layers_of(world, entity, place, &|guid| drawn.number_of(guid), leave_out);
    let size = place.block_size.max(0.01);
    let [bx, bz] = place.blocks_of(at);
    let column = [bx.floor() as i32, bz.floor() as i32];
    let generate = &making.generate;
    let middle = |v: i32, axis: usize| generate.position[axis] + (v as f32 + 0.5) * size;
    // The world's own generator: with its rivers, its biomes' land and its painted maps.
    let ground = voxear_terrain::blocks::generate::ground_generator(generate, &making.world, size);
    let height = ground.height_at(middle(column[0], 0), middle(column[1], 1));
    let ground = (height / size - 0.5).ceil() as i32;
    let top = layers.and_then(|layers| layers.shape(column, ground)).map_or(ground, |shaped| shaped.top);
    Some(corner.y + top as f32 * size)
}

/// Where the Obj `obj`, with a Ground Fit, lands set down at `at`: on the
/// ground the generator and every other fit make there, its own left out,
/// with the point it levels to on that ground. None where it has no fit or
/// no world of blocks without end is in its dimension.
pub fn landing(world: &mut World, content: &mut crate::content::Content, obj: Entity, at: Vec3) -> Option<Vec3> {
    let fit = world.get::<GroundFit>(obj)?.clone();
    let here = voxear_scene::dimension_of(world, obj);
    let blocks = world
        .query_ref::<(Entity, &crate::blocks::BlockWorldAsset)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))
        .map(|(entity, _)| entity)
        .find(|entity| voxear_scene::dimension_of(world, *entity) == here)?;
    let ground = bare_ground(world, content, blocks, at, Some(obj))?;
    let rotation = world.get::<WorldTransform>(obj).map_or(Mat4::IDENTITY, |placed| placed.0);
    let lift = match fit.level {
        FitLevel::Obj => 0.0,
        FitLevel::Point => rotation.transform_vector3(Vec3::from(fit.point)).y,
    };
    let size = world.get::<crate::blocks::BlockWorldAsset>(blocks).map_or(1.0, |asset| asset.file.block_size);
    Some(Vec3::new(at.x, ground - lift - fit.sink as f32 * size, at.z))
}

/// The ground under `at` of the world of blocks without end in the dimension
/// of `obj` (the level's without one), in metres up, as the generator and
/// every layer but `obj`'s own make it.
pub fn ground_under(
    world: &mut World,
    content: &mut crate::content::Content,
    at: Vec3,
    obj: Option<Entity>,
) -> Option<f32> {
    let blocks = match obj {
        Some(obj) => crate::roads::world_of(world, obj)?,
        None => world
            .query_ref::<(Entity, &crate::blocks::BlockWorldAsset)>()
            .into_iter()
            .find(|(_, asset)| asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some()))?
            .0,
    };
    bare_ground(world, content, blocks, at, obj)
}

/// The corner of the world of blocks on `entity`, where its block 0 is.
pub fn world_corner(world: &World, entity: Entity) -> Option<Vec3> {
    let local = world.get::<voxear_scene::Transform>(entity)?;
    Some(voxear_terrain::corner(&local, world.get::<WorldTransform>(entity).as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_footprint_is_laid_in_the_worlds_blocks_and_levelled_at_its_door() {
        let fit = GroundFit {
            boxes: vec![FitBox { centre: [0.0, 0.0], size: [8.0, 4.0] }],
            level: FitLevel::Point,
            point: [0.0, 2.0, 2.0],
            sink: 1,
            ..GroundFit::default()
        };
        let turned = Mat4::from_rotation_translation(
            voxear_core::math::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::new(20.0, 10.0, 20.0),
        );
        let place = WorldPlace { corner: Vec3::ZERO, block_size: 2.0 };
        let layer = fit.layer(&turned, place, &|_| None).unwrap();
        let [low, high] = layer.bounds().unwrap();
        // Turned a quarter, its 8 m runs along z: 4 blocks of 2 m, round block 10.
        let along = |axis: usize| (high[axis] - low[axis]) as f32 - 2.0 * (layer.reach() + 1.0);
        assert!((along(1) - 4.0).abs() <= 1.0 && (along(0) - 2.0).abs() <= 1.0, "{low:?} {high:?}");
        assert_eq!(layer.level, 6 + 1, "12 m up at its door, a block sunk: the ground a block over it");
    }
}
