//! Ways over an endless Voxel Volume's ground, for Nav Agents and Block Mobs
//! with no world of blocks or nav mesh under them: a path from here to
//! there for an agent this big. Until voxel ground has ways found over it,
//! [`Straight`] goes straight there; a navigation of voxel ground puts its
//! own [`VoxelWays`] in the world with [`set`], and every agent on a volume
//! walks the ways it finds.

use std::sync::Arc;

use voxear_core::math::Vec3;
use voxear_ecs::World;
use voxear_scene::DimensionId;

/// A way found: its corners from where it starts, and whether it reaches
/// where it was asked to go or only as near as it can; and, where it has
/// them, for each corner whether it is jumped to from the one before, and
/// the kind of Nav Link it is reached by (see `voxear_nav::Link::kind`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelWay {
    pub corners: Vec<Vec3>,
    pub complete: bool,
    pub jumps: Vec<bool>,
    pub links: Vec<Option<u8>>,
}

impl VoxelWay {
    /// A way straight from corner to corner, with no jumps or links.
    pub fn straight(corners: Vec<Vec3>, complete: bool) -> Self {
        Self { corners, complete, ..Self::default() }
    }
}

/// What walks a way: how tall it is and how wide round its middle, in
/// metres; whether it swims across water too deep to stand in, jumps gaps
/// and climbs; its layer, which says which Nav Links it takes; and its
/// agent type, which says what each kind of ground costs it and how high
/// it steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelWalker {
    pub height: f32,
    pub radius: f32,
    pub swims: bool,
    pub jumps: bool,
    pub climbs: bool,
    pub layer: u8,
    pub agent_type: u8,
}

impl VoxelWalker {
    /// Something `height` tall and `radius` round that only walks.
    pub fn of_size(height: f32, radius: f32) -> Self {
        Self { height, radius, swims: false, jumps: false, climbs: false, layer: 0, agent_type: 0 }
    }
}

/// What finds ways over voxel ground.
pub trait VoxelWays: Send + Sync {
    /// A way over the ground of dimension `dimension` from `from` to `to`,
    /// both in the world, for `walker`: none where there is no way, not
    /// even part of one.
    fn way(
        &self,
        world: &World,
        dimension: DimensionId,
        from: Vec3,
        to: Vec3,
        walker: &VoxelWalker,
    ) -> Option<VoxelWay>;
}

/// Straight there, whatever stands in the way.
pub struct Straight;

impl VoxelWays for Straight {
    fn way(&self, _: &World, _: DimensionId, from: Vec3, to: Vec3, _: &VoxelWalker) -> Option<VoxelWay> {
        Some(VoxelWay::straight(vec![from, to], true))
    }
}

/// The world's finder of ways over voxel ground.
#[derive(Clone)]
struct Finder(Arc<dyn VoxelWays>);

/// Makes `ways` what finds ways over voxel ground from now on.
pub fn set(world: &mut World, ways: Arc<dyn VoxelWays>) {
    world.insert_resource(Finder(ways));
}

/// Whether something was set to find ways over voxel ground.
pub fn is_set(world: &World) -> bool {
    world.resource::<Finder>().is_some()
}

/// What finds ways over voxel ground: the one set, or [`Straight`].
pub fn finder(world: &World) -> Arc<dyn VoxelWays> {
    world.resource::<Finder>().map_or_else(|| Arc::new(Straight) as Arc<dyn VoxelWays>, |finder| finder.0.clone())
}
