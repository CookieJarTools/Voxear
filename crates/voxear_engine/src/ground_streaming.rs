//! Objs that come and go with their ground. A world of blocks without end
//! makes its near columns round where it is seen from and lets them go
//! behind; an Obj with **Streams With Ground** standing on a column that goes
//! goes with it, as its When Unloaded says:
//!
//! - **Keep For Later**: it and everything under it are kept as a scene of
//!   their own, its Obj taken away, and made again exactly as it was when its
//!   column is made again. A house and what is in it.
//! - **Destroy**: it goes and does not come back. A mob a script spawned.
//! - **Stay**: nothing happens. Something that must stay, as a quest's.
//!
//! An Obj with a Ground Fit and no Streams With Ground of its own is kept for
//! later, as it shapes the ground it goes with. Behaviours are told first,
//! with `OnGroundUnloading`, and those of an Obj come back with
//! `OnGroundLoaded`. Only while the game plays: in the editor nothing goes.
//!
//! An Obj goes only once it has stood on made ground: one placed or spawned
//! where the ground is not made yet waits for it. Which column it stands on
//! is where its Obj is.
//!
//! Each frame's columns made and let go are kept as [`GroundMoves`], for
//! scripts and other systems: `vox editor call blocks.columns` reads them.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, DimensionId, Parent, WorldTransform};
use voxear_terrain::blocks::chunk::CHUNK;

use crate::scene_file::SceneFile;

/// What happens to an Obj when the ground under it streams out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhenUnloaded {
    /// Kept as it is, its Obj taken away, and made again when its ground is.
    #[default]
    KeepForLater,
    /// Destroyed, and not made again.
    Destroy,
    /// Left as it is.
    Stay,
}

impl WhenUnloaded {
    pub const ALL: [WhenUnloaded; 3] = [WhenUnloaded::KeepForLater, WhenUnloaded::Destroy, WhenUnloaded::Stay];

    pub fn name(self) -> &'static str {
        match self {
            WhenUnloaded::KeepForLater => "Keep For Later",
            WhenUnloaded::Destroy => "Destroy",
            WhenUnloaded::Stay => "Stay",
        }
    }
}

/// Makes its Obj come and go with the ground under it (see the module's own
/// words).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StreamsWithGround {
    pub when_unloaded: WhenUnloaded,
}

impl voxear_ecs::Component for StreamsWithGround {}

/// A world of blocks without end's columns this frame: where it lies, the
/// near columns made now, and those made and let go since last frame.
#[derive(Clone, Debug)]
pub struct WorldMoves {
    pub world: Entity,
    pub dimension: DimensionId,
    pub corner: Vec3,
    pub block_size: f32,
    pub near: Arc<HashSet<[i32; 2]>>,
    pub came: Vec<[i32; 2]>,
    pub went: Vec<[i32; 2]>,
}

impl WorldMoves {
    /// The chunk column a place in the world lies in.
    pub fn column_of(&self, at: Vec3) -> [i32; 2] {
        let column = CHUNK as f32 * self.block_size.max(0.01);
        [((at.x - self.corner.x) / column).floor() as i32, ((at.z - self.corner.z) / column).floor() as i32]
    }
}

/// Every world of blocks without end streamed this frame, and its columns.
#[derive(Clone, Debug, Default)]
pub struct GroundMoves {
    pub worlds: Vec<WorldMoves>,
}

/// Objs kept for later while their ground is away, each a scene of its own.
#[derive(Default)]
pub struct GroundRecords {
    pub records: Vec<GroundRecord>,
}

pub struct GroundRecord {
    pub world: Entity,
    pub column: [i32; 2],
    pub dimension: DimensionId,
    pub scene: SceneFile,
    /// The Ground Fits of its Objs, where each was placed: they go on shaping
    /// the ground while it is away, so the far view and the map still show
    /// it.
    pub fits: Vec<(crate::ground_fit::GroundFit, voxear_core::math::Mat4)>,
}

/// The Ground Fits of `root` and every Obj under it, where each is placed.
pub fn fits_under(world: &World, root: Entity) -> Vec<(crate::ground_fit::GroundFit, voxear_core::math::Mat4)> {
    let under = |mut entity: Entity| loop {
        if entity == root {
            return true;
        }
        match world.get::<Parent>(entity) {
            Some(parent) => entity = parent.0,
            None => return false,
        }
    };
    world
        .query_ref::<(Entity, &crate::ground_fit::GroundFit, &WorldTransform)>()
        .into_iter()
        .filter(|(entity, ..)| under(*entity))
        .map(|(_, fit, placed)| (fit.clone(), placed.0))
        .collect()
}

/// The Ground Fits of the Objs kept for later in `dimension`.
pub fn kept_fits(
    world: &World,
    dimension: DimensionId,
) -> Vec<(crate::ground_fit::GroundFit, voxear_core::math::Mat4)> {
    let Some(kept) = world.resource::<GroundRecords>() else { return Vec::new() };
    kept.records
        .iter()
        .filter(|record| record.dimension == dimension)
        .flat_map(|record| record.fits.iter().cloned())
        .collect()
}

/// The Objs seen standing on made ground, which go once it does.
#[derive(Default)]
struct OnGround(HashSet<Entity>);

/// What goes this frame: each Obj, kept for later or not.
pub struct Going {
    pub keep: Vec<(Entity, usize)>,
    pub destroy: Vec<Entity>,
}

/// The Objs whose ground went: those standing on a column of a world in
/// their dimension that is not made now, having stood on made ground. Their
/// world's place in `moves` with each kept for later.
pub fn going(world: &mut World, moves: &GroundMoves) -> Going {
    let mut on_ground = world.remove_resource::<OnGround>().unwrap_or_default();
    let mut going = Going { keep: Vec::new(), destroy: Vec::new() };
    let by_dimension: HashMap<DimensionId, usize> =
        moves.worlds.iter().enumerate().map(|(at, moved)| (moved.dimension, at)).collect();
    let candidates: Vec<(Entity, Vec3, WhenUnloaded)> = world
        .query_ref::<(Entity, &WorldTransform, Option<&StreamsWithGround>, Option<&crate::ground_fit::GroundFit>)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, placed, streams, fit)| {
            let when = match (streams, fit) {
                (Some(streams), _) => streams.when_unloaded,
                (None, Some(_)) if !world.has::<Parent>(entity) => WhenUnloaded::KeepForLater,
                _ => return None,
            };
            Some((entity, placed.0.w_axis.truncate(), when))
        })
        .filter(|(.., when)| *when != WhenUnloaded::Stay)
        .collect();
    for (entity, at, when) in candidates {
        let Some(&which) = by_dimension.get(&voxear_scene::dimension_of(world, entity)) else { continue };
        let moved = &moves.worlds[which];
        if moved.near.contains(&moved.column_of(at)) {
            on_ground.0.insert(entity);
            continue;
        }
        if !on_ground.0.remove(&entity) {
            continue;
        }
        match when {
            WhenUnloaded::KeepForLater => going.keep.push((entity, which)),
            _ => going.destroy.push(entity),
        }
    }
    on_ground.0.retain(|entity| world.is_alive(*entity));
    world.insert_resource(on_ground);
    going
}

/// Notes Objs made again as standing on made ground.
pub fn landed(world: &mut World, objs: &[Entity]) {
    let mut on_ground = world.remove_resource::<OnGround>().unwrap_or_default();
    on_ground.0.extend(objs);
    world.insert_resource(on_ground);
}

/// The records whose column came back this frame, taken out of those kept.
pub fn coming(world: &mut World, moves: &GroundMoves) -> Vec<GroundRecord> {
    let Some(mut kept) = world.remove_resource::<GroundRecords>() else { return Vec::new() };
    let back = |record: &GroundRecord| {
        moves.worlds.iter().any(|moved| moved.world == record.world && moved.near.contains(&record.column))
    };
    let (coming, staying): (Vec<GroundRecord>, Vec<GroundRecord>) = kept.records.drain(..).partition(back);
    kept.records = staying;
    world.insert_resource(kept);
    coming
}

/// How many Objs are kept for later now.
pub fn kept_count(world: &World) -> usize {
    world.resource::<GroundRecords>().map_or(0, |kept| kept.records.len())
}
