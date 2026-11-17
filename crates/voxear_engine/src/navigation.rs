//! Navigation in a level: each world of blocks' places, kept up with its
//! blocks (see `voxear_nav`).
//!
//! A world's places are made once something asks for them, as a Block Mob
//! does by walking in it, and from then on follow its blocks as they
//! stream in, stream out and change: on navigation's own threads, handed
//! over whole. A small world's first places are made at once; a world
//! without end makes its own in the background as its ground comes.

use std::{collections::HashMap, sync::Arc};

use voxear_ecs::{Entity, World};
use voxear_nav::{BlockNav, BlockNavBuilder, blocks::walking::WALKERS};
use voxear_terrain::blocks::BlockWorld;

/// Every world of blocks' navigation, by its Obj, for those something has
/// asked for.
#[derive(Default)]
pub struct NavWorlds {
    worlds: HashMap<Entity, BlockNavBuilder>,
}

impl NavWorlds {
    /// World `entity`'s navigation as last made, if it is kept.
    pub fn nav(&self, entity: Entity) -> Option<&Arc<BlockNav>> {
        self.worlds.get(&entity).map(BlockNavBuilder::nav)
    }

    /// Each world kept: its Obj and its builder.
    pub fn worlds(&self) -> impl Iterator<Item = (Entity, &BlockNavBuilder)> {
        self.worlds.iter().map(|(entity, builder)| (*entity, builder))
    }
}

/// Keeps every world's navigation up with its blocks, and lets go of
/// those whose worlds are gone. Cheap where nothing changed: a look at two
/// pointers a world.
pub fn update(world: &mut World) {
    let Some(mut navs) = world.remove_resource::<NavWorlds>() else { return };
    voxear_core::profile_scope!("Navigation");
    navs.worlds.retain(|entity, builder| match world.get::<BlockWorld>(*entity) {
        Some(blocks) => {
            builder.update(&blocks.blocks, &blocks.faces);
            true
        }
        None => false,
    });
    world.insert_resource(navs);
}

/// World `entity`'s navigation, kept from now on: made now where it is
/// small, or begun in the background where it is not.
pub fn nav_of(world: &mut World, entity: Entity) -> Option<Arc<BlockNav>> {
    let blocks = world.get::<BlockWorld>(entity)?;
    let (blocks, faces) = (blocks.blocks.clone(), blocks.faces.clone());
    if world.resource::<NavWorlds>().is_none() {
        world.insert_resource(NavWorlds::default());
    }
    let navs = world.resource_mut::<NavWorlds>()?;
    let builder = navs.worlds.entry(entity).or_insert_with(|| {
        let mut builder = BlockNavBuilder::new(&WALKERS);
        builder.update(&blocks, &faces);
        builder
    });
    Some(builder.nav().clone())
}

/// Waits until every kept world's navigation is made from its blocks as
/// they are now: before play begins, and in tests.
pub fn settle(world: &mut World) {
    let Some(mut navs) = world.remove_resource::<NavWorlds>() else { return };
    for (entity, builder) in &mut navs.worlds {
        if let Some(blocks) = world.get::<BlockWorld>(*entity) {
            builder.settle(&blocks.blocks, &blocks.faces);
        }
    }
    world.insert_resource(navs);
}

/// What each kind of ground costs things finding their way, a block of it
/// at a time, plain ground costing 1: what is cheaper is sought out, what
/// is dearer kept off where there is another way. And the names of the
/// project's own four areas.
///
/// ```toml
/// [navigation]
/// road_cost = 0.6          # ground blocks and Nav Areas mark as a road
/// water_cost = 3.0         # water a block deep, waded
/// swim_cost = 4.0          # water too deep to stand in, swum
/// door_cost = 2.0          # a shut door, opened to go through
/// custom_1_name = "Mud"    # the project's own areas, named and costed
/// custom_1_cost = 2.5
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavigationSettings {
    pub road_cost: f32,
    pub water_cost: f32,
    pub swim_cost: f32,
    pub door_cost: f32,
    pub custom_1_name: String,
    pub custom_1_cost: f32,
    pub custom_2_name: String,
    pub custom_2_cost: f32,
    pub custom_3_name: String,
    pub custom_3_cost: f32,
    pub custom_4_name: String,
    pub custom_4_cost: f32,
}

impl Default for NavigationSettings {
    fn default() -> Self {
        let costs = voxear_nav::AreaCosts::default();
        let cost = |area: u8| costs.of(area);
        Self {
            road_cost: cost(voxear_nav::ROAD),
            water_cost: cost(voxear_nav::WATER),
            swim_cost: cost(voxear_nav::SWIM),
            door_cost: cost(voxear_nav::DOOR),
            custom_1_name: String::new(),
            custom_1_cost: 1.0,
            custom_2_name: String::new(),
            custom_2_cost: 1.0,
            custom_3_name: String::new(),
            custom_3_cost: 1.0,
            custom_4_name: String::new(),
            custom_4_cost: 1.0,
        }
    }
}

/// What each area costs things finding their way, as the project says.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NavCosts(pub voxear_nav::AreaCosts);

impl NavigationSettings {
    /// What each area costs, by these settings.
    pub fn costs(&self) -> voxear_nav::AreaCosts {
        let mut costs = voxear_nav::AreaCosts::default();
        let cost = |cost: f32| if cost.is_finite() { cost.clamp(0.05, 1000.0) } else { 1.0 };
        let set = [
            (voxear_nav::ROAD, self.road_cost),
            (voxear_nav::WATER, self.water_cost),
            (voxear_nav::SWIM, self.swim_cost),
            (voxear_nav::DOOR, self.door_cost),
            (voxear_nav::CUSTOM, self.custom_1_cost),
            (voxear_nav::CUSTOM + 1, self.custom_2_cost),
            (voxear_nav::CUSTOM + 2, self.custom_3_cost),
            (voxear_nav::CUSTOM + 3, self.custom_4_cost),
        ];
        for (area, value) in set {
            costs.0[usize::from(area)] = cost(value);
        }
        costs
    }

    /// Puts these settings in `world`, for what finds its way from now on.
    pub fn put(&self, world: &mut World) {
        world.insert_resource(NavCosts(self.costs()));
    }
}

/// What each area costs in `world`: the project's, or the defaults.
pub fn costs(world: &World) -> voxear_nav::AreaCosts {
    world.resource::<NavCosts>().map(|costs| costs.0).unwrap_or_default()
}

/// A box of the world given an area of ground: a road through a town, a
/// danger round a fire, a field nothing walks on. Centred on its Obj, as
/// wide, high and long as `size`, square to the world. The places whose
/// feet are in it are of its area, whatever they stand on.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavArea {
    pub size: [f32; 3],
    pub area: voxear_terrain::blocks::block::BlockNavArea,
}

impl voxear_ecs::Component for NavArea {}

impl Default for NavArea {
    fn default() -> Self {
        Self {
            size: [4.0, 2.0, 4.0],
            area: voxear_terrain::blocks::block::BlockNavArea::Road,
        }
    }
}

/// Every Nav Area's box, in blocks from the corner of a world of blocks at
/// `corner` whose blocks are `size` metres a side, of dimension
/// `dimension`.
pub fn area_boxes(
    world: &World,
    corner: voxear_core::math::Vec3,
    size: f32,
    dimension: voxear_scene::DimensionId,
) -> Vec<voxear_nav::AreaBox> {
    world
        .query_ref::<(Entity, &NavArea, &voxear_scene::Transform)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(entity, ..)| voxear_scene::dimension_of(world, *entity) == dimension)
        .map(|(entity, area, transform)| {
            let middle = world
                .get::<voxear_scene::WorldTransform>(entity)
                .map_or(transform.position, |placed| placed.0.w_axis.truncate());
            let half = voxear_core::math::Vec3::from_array(area.size).abs() * 0.5;
            let local = |at: voxear_core::math::Vec3| ((at - corner) / size.max(1e-3)).to_array();
            let number = area.area.number().max(1) - 1;
            voxear_nav::AreaBox { min: local(middle - half), max: local(middle + half), area: number }
        })
        .collect()
}
