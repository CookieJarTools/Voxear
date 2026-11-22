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
    navs.worlds.retain(|entity, _| world.get::<BlockWorld>(*entity).is_some());
    for (entity, builder) in navs.worlds.iter_mut() {
        // Its blocks with its walkable models stamped in (see `walkable_models`).
        let Some(blocks) = crate::walkable_models::for_navigation(world, *entity) else { continue };
        let Some(faces) = world.get::<BlockWorld>(*entity).map(|world| world.faces.clone()) else { continue };
        builder.update(&blocks, &faces);
    }
    world.insert_resource(navs);
}

/// World `entity`'s navigation, kept from now on: made now where it is
/// small, or begun in the background where it is not.
pub fn nav_of(world: &mut World, entity: Entity) -> Option<Arc<BlockNav>> {
    let found = world.get::<BlockWorld>(entity)?;
    let (faces, size) = (found.faces.clone(), found.block_size);
    let blocks = crate::walkable_models::for_navigation(world, entity)?;
    if world.resource::<NavWorlds>().is_none() {
        world.insert_resource(NavWorlds::default());
    }
    let navs = world.resource_mut::<NavWorlds>()?;
    let builder = navs.worlds.entry(entity).or_insert_with(|| {
        let mut builder = BlockNavBuilder::new(&walkers_for(size));
        builder.update(&blocks, &faces);
        builder
    });
    Some(builder.nav().clone())
}

/// The walkers a world of blocks `size` metres a side gathers its places
/// into regions for: those a block wide of each height, and two blocks wide
/// for what is that big there, as an ogre in blocks a metre across or a
/// person in blocks half that.
fn walkers_for(size: f32) -> Vec<voxear_nav::Walker> {
    let wide = |tall: u16| voxear_nav::Walker::blocks(tall).wide(2).class();
    let mut walkers = WALKERS.to_vec();
    walkers.push(wide(3));
    if size < 0.7 {
        walkers.extend([wide(4), wide(5)]);
    }
    walkers
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
/// agent_height = 1.8       # what the Default agent type is baked for
/// agent_1_name = "Ogre"    # the project's own agent types, sized and costed
/// agent_1_height = 3.2
/// agent_1_radius = 0.9
/// agent_1_water_cost = 8.0 # 0 for the project's own cost
/// searches_a_step = 64     # how many ways agents begin to look for each step
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
    /// How many searches agents may begin each step, a crowd sent off at
    /// once having its ways found over the next few; how many of those no
    /// thread has begun by the step after may be done in the step itself;
    /// and how many places one search may look at before it gives up.
    pub searches_a_step: u32,
    pub searches_here: u32,
    pub most_looked_at: u32,
    /// What the Default agent type is baked for: how tall, how wide round
    /// its middle, how high a step and how steep a slope, in metres and
    /// degrees.
    pub agent_height: f32,
    pub agent_radius: f32,
    pub agent_step: f32,
    pub agent_slope: f32,
    /// The project's own four agent types: each used once named, sized as
    /// the Default one is, and with its own costs of roads, water and
    /// swimming, 0 for the project's.
    pub agent_1_name: String,
    pub agent_1_height: f32,
    pub agent_1_radius: f32,
    pub agent_1_step: f32,
    pub agent_1_slope: f32,
    pub agent_1_road_cost: f32,
    pub agent_1_water_cost: f32,
    pub agent_1_swim_cost: f32,
    pub agent_2_name: String,
    pub agent_2_height: f32,
    pub agent_2_radius: f32,
    pub agent_2_step: f32,
    pub agent_2_slope: f32,
    pub agent_2_road_cost: f32,
    pub agent_2_water_cost: f32,
    pub agent_2_swim_cost: f32,
    pub agent_3_name: String,
    pub agent_3_height: f32,
    pub agent_3_radius: f32,
    pub agent_3_step: f32,
    pub agent_3_slope: f32,
    pub agent_3_road_cost: f32,
    pub agent_3_water_cost: f32,
    pub agent_3_swim_cost: f32,
    pub agent_4_name: String,
    pub agent_4_height: f32,
    pub agent_4_radius: f32,
    pub agent_4_step: f32,
    pub agent_4_slope: f32,
    pub agent_4_road_cost: f32,
    pub agent_4_water_cost: f32,
    pub agent_4_swim_cost: f32,
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
            searches_a_step: 64,
            searches_here: 8,
            most_looked_at: 60_000,
            agent_height: 1.8,
            agent_radius: 0.35,
            agent_step: 0.45,
            agent_slope: 45.0,
            agent_1_name: String::new(),
            agent_1_height: 1.8,
            agent_1_radius: 0.35,
            agent_1_step: 0.45,
            agent_1_slope: 45.0,
            agent_1_road_cost: 0.0,
            agent_1_water_cost: 0.0,
            agent_1_swim_cost: 0.0,
            agent_2_name: String::new(),
            agent_2_height: 1.8,
            agent_2_radius: 0.35,
            agent_2_step: 0.45,
            agent_2_slope: 45.0,
            agent_2_road_cost: 0.0,
            agent_2_water_cost: 0.0,
            agent_2_swim_cost: 0.0,
            agent_3_name: String::new(),
            agent_3_height: 1.8,
            agent_3_radius: 0.35,
            agent_3_step: 0.45,
            agent_3_slope: 45.0,
            agent_3_road_cost: 0.0,
            agent_3_water_cost: 0.0,
            agent_3_swim_cost: 0.0,
            agent_4_name: String::new(),
            agent_4_height: 1.8,
            agent_4_radius: 0.35,
            agent_4_step: 0.45,
            agent_4_slope: 45.0,
            agent_4_road_cost: 0.0,
            agent_4_water_cost: 0.0,
            agent_4_swim_cost: 0.0,
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
        world.insert_resource(NavAgentTypes(self.agent_types().to_vec()));
        world.insert_resource(self.budgets());
    }

    /// What agents may spend on searches, by these settings.
    pub fn budgets(&self) -> NavBudgets {
        NavBudgets {
            searches_a_step: self.searches_a_step.clamp(1, 100_000) as usize,
            searches_here: self.searches_here.min(100_000) as usize,
            most_looked_at: self.most_looked_at.clamp(100, 10_000_000) as usize,
        }
    }

    /// Each agent type by its slot: the Default one, then the project's
    /// own four, named or not.
    pub fn agent_types(&self) -> [NavAgentType; AGENT_TYPES] {
        let costs = self.costs();
        let size = |[height, radius, step, slope]: [f32; 4]| {
            let kept = |value: f32, low: f32, high: f32, otherwise: f32| match value.is_finite() {
                true => value.clamp(low, high),
                false => otherwise,
            };
            (
                kept(height, 0.1, 100.0, 1.8),
                kept(radius, 0.05, 50.0, 0.35),
                kept(step, 0.0, 10.0, 0.45),
                kept(slope, 0.0, 89.0, 45.0),
            )
        };
        let default_size = size([self.agent_height, self.agent_radius, self.agent_step, self.agent_slope]);
        let mut types = [(); AGENT_TYPES].map(|_| NavAgentType::default());
        types[0] = NavAgentType::of("Default".into(), default_size, costs);
        let own: [(&String, [f32; 4], [f32; 3]); 4] = [
            (
                &self.agent_1_name,
                [self.agent_1_height, self.agent_1_radius, self.agent_1_step, self.agent_1_slope],
                [self.agent_1_road_cost, self.agent_1_water_cost, self.agent_1_swim_cost],
            ),
            (
                &self.agent_2_name,
                [self.agent_2_height, self.agent_2_radius, self.agent_2_step, self.agent_2_slope],
                [self.agent_2_road_cost, self.agent_2_water_cost, self.agent_2_swim_cost],
            ),
            (
                &self.agent_3_name,
                [self.agent_3_height, self.agent_3_radius, self.agent_3_step, self.agent_3_slope],
                [self.agent_3_road_cost, self.agent_3_water_cost, self.agent_3_swim_cost],
            ),
            (
                &self.agent_4_name,
                [self.agent_4_height, self.agent_4_radius, self.agent_4_step, self.agent_4_slope],
                [self.agent_4_road_cost, self.agent_4_water_cost, self.agent_4_swim_cost],
            ),
        ];
        for (slot, (name, sizes, [road, water, swim])) in own.into_iter().enumerate() {
            let mut own_costs = costs;
            for (area, cost) in [(voxear_nav::ROAD, road), (voxear_nav::WATER, water), (voxear_nav::SWIM, swim)] {
                if cost.is_finite() && cost > 0.0 {
                    own_costs.0[usize::from(area)] = cost.clamp(0.05, 1000.0);
                }
            }
            let shown =
                if name.trim().is_empty() { format!("Agent Type {}", slot + 1) } else { name.trim().to_string() };
            types[slot + 1] =
                NavAgentType { named: !name.trim().is_empty(), ..NavAgentType::of(shown, size(sizes), own_costs) };
        }
        types
    }
}

/// What agents may spend on searches (see
/// [`NavigationSettings::searches_a_step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavBudgets {
    pub searches_a_step: usize,
    pub searches_here: usize,
    pub most_looked_at: usize,
}

impl Default for NavBudgets {
    fn default() -> Self {
        NavigationSettings::default().budgets()
    }
}

/// What agents may spend on searches in `world`: the project's, or the
/// defaults.
pub fn budgets(world: &World) -> NavBudgets {
    world.resource::<NavBudgets>().copied().unwrap_or_default()
}

/// What each area costs in `world`: the project's, or the defaults.
pub fn costs(world: &World) -> voxear_nav::AreaCosts {
    world.resource::<NavCosts>().map(|costs| costs.0).unwrap_or_default()
}

/// How many agent types a project has: the Default one and four of its own.
pub const AGENT_TYPES: usize = 5;

/// A kind of thing finding its way, as a project sets it: what its level's
/// nav mesh is baked for (how tall, how wide round its middle, how high a
/// step and how steep a slope, in metres and degrees), and what each area
/// costs it. Its name is as shown; `named` where the project named it.
#[derive(Clone, Debug, PartialEq)]
pub struct NavAgentType {
    pub name: String,
    pub named: bool,
    pub height: f32,
    pub radius: f32,
    pub step: f32,
    pub slope: f32,
    pub costs: voxear_nav::AreaCosts,
}

impl Default for NavAgentType {
    fn default() -> Self {
        Self::of("Default".into(), (1.8, 0.35, 0.45, 45.0), voxear_nav::AreaCosts::default())
    }
}

impl NavAgentType {
    fn of(name: String, (height, radius, step, slope): (f32, f32, f32, f32), costs: voxear_nav::AreaCosts) -> Self {
        Self { name, named: true, height, radius, step, slope, costs }
    }

    /// What a level's nav mesh is baked with for it.
    pub fn bake_settings(&self) -> voxear_nav::mesh::BakeSettings {
        voxear_nav::mesh::BakeSettings {
            height: self.height,
            radius: self.radius,
            climb: self.step,
            slope: self.slope,
            ..voxear_nav::mesh::BakeSettings::default()
        }
    }
}

/// The project's agent types, by slot (see [`NavigationSettings::agent_types`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavAgentTypes(pub Vec<NavAgentType>);

/// Agent type `slot` in `world`: the project's, or the Default one where
/// there is none in that slot.
pub fn agent_type(world: &World, slot: u8) -> NavAgentType {
    let types = world.resource::<NavAgentTypes>();
    let found = types.and_then(|types| types.0.get(usize::from(slot)).or_else(|| types.0.first())).cloned();
    found.unwrap_or_else(|| NavAgentType { costs: costs(world), ..NavAgentType::default() })
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
        Self { size: [4.0, 2.0, 4.0], area: voxear_terrain::blocks::block::BlockNavArea::Road }
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

/// How what walks a Nav Link crosses it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavLinkKind {
    /// Straight along it, as through a gap in a fence.
    #[default]
    Walk,
    /// Over it in a leap, as across a gap too wide to walk.
    Jump,
    /// Up or down it slowly, as on a ladder or a rope.
    Climb,
    /// From one end to the other at once, as through a door that takes you elsewhere.
    Teleport,
}

impl NavLinkKind {
    pub const ALL: [Self; 4] = [Self::Walk, Self::Jump, Self::Climb, Self::Teleport];

    pub fn number(self) -> u8 {
        self as u8
    }

    pub fn of_number(number: u8) -> Self {
        Self::ALL.get(usize::from(number)).copied().unwrap_or_default()
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Walk => "Walk",
            Self::Jump => "Jump",
            Self::Climb => "Climb",
            Self::Teleport => "Teleport",
        }
    }
}

/// A way a level's maker joins two places that walking does not: from its
/// Obj to `end` (in the Obj's own space, so it moves and turns with it), by
/// a ladder, a gap, a drop or a door that takes you elsewhere. A search may
/// take it as one step, costing as far as it goes and `cost` more; what
/// walks it crosses it as `kind` says. One way, unless `both_ways`.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavLink {
    pub end: [f32; 3],
    pub kind: NavLinkKind,
    pub both_ways: bool,
    pub cost: f32,
}

impl voxear_ecs::Component for NavLink {}

impl Default for NavLink {
    fn default() -> Self {
        Self { end: [0.0, 0.0, 2.0], kind: NavLinkKind::Climb, both_ways: true, cost: 1.0 }
    }
}

/// Every Nav Link's two ends in the world: from where, to where, and it.
pub fn link_ends(
    world: &World,
    entity: Entity,
    link: &NavLink,
) -> Option<(voxear_core::math::Vec3, voxear_core::math::Vec3)> {
    let placed = match world.get::<voxear_scene::WorldTransform>(entity) {
        Some(placed) => placed.0,
        None => world.get::<voxear_scene::Transform>(entity)?.matrix(),
    };
    let end = voxear_core::math::Vec3::from_array(link.end);
    Some((placed.w_axis.truncate(), placed.transform_point3(end)))
}

/// The kind of link a Portal is to a search: in at its front, out of the
/// one it leads to; and what going through costs more than walking as far,
/// in metres.
pub const PORTAL_LINK: u8 = 4;
const PORTAL_COST: f32 = 0.5;

/// Every Nav Link of dimension `dimension`, and every Portal within it, in
/// blocks from the corner of a world of blocks at `corner` whose blocks are
/// `size` metres a side, each with the layers whose Objs go along it, a
/// bit a layer: every layer for a Nav Link, those a Portal lets through.
pub fn links(
    world: &World,
    corner: voxear_core::math::Vec3,
    size: f32,
    dimension: voxear_scene::DimensionId,
) -> Vec<(voxear_nav::Link, u32)> {
    let local = |at: voxear_core::math::Vec3| ((at - corner) / size.max(1e-3)).to_array();
    let portals =
        crate::portals::nav_portals(world).into_iter().filter(|way| way.from == dimension && way.into == dimension);
    let portals: Vec<(voxear_nav::Link, u32)> = portals
        .map(|way| {
            let link = voxear_nav::Link {
                from: local(way.approach),
                to: local(way.exit),
                both_ways: false,
                cost: PORTAL_COST / size.max(1e-3),
                kind: PORTAL_LINK,
            };
            (link, way.passes)
        })
        .collect();
    let mut links: Vec<(voxear_nav::Link, u32)> = world
        .query_ref::<(Entity, &NavLink)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(entity, _)| voxear_scene::dimension_of(world, *entity) == dimension)
        .filter_map(|(entity, link)| {
            let (from, to) = link_ends(world, entity, link)?;
            let link = voxear_nav::Link {
                from: local(from),
                to: local(to),
                both_ways: link.both_ways,
                cost: link.cost.max(0.0) / size.max(1e-3),
                kind: link.kind.number(),
            };
            Some((link, u32::MAX))
        })
        .collect();
    links.extend(portals);
    links
}

/// Those of `links` an Obj of `layer` goes along.
pub fn heeded(links: &[(voxear_nav::Link, u32)], layer: u8) -> Vec<voxear_nav::Link> {
    links.iter().filter(|(_, passes)| passes & (1 << (layer & 31)) != 0).map(|(link, _)| *link).collect()
}

/// Every Nav Link of dimension `dimension`, and every Portal within it, as
/// a level's nav mesh takes them for an Obj of `layer`: each way it goes,
/// in metres from the mesh's origin, `offset` from the world's; and the
/// kind of each.
pub fn mesh_links(
    world: &World,
    offset: voxear_core::math::Vec3,
    dimension: voxear_scene::DimensionId,
    layer: u8,
) -> (Vec<voxear_nav::mesh::MeshLink>, Vec<u8>) {
    let mut links = Vec::new();
    let mut kinds = Vec::new();
    for link in heeded(&self::links(world, voxear_core::math::Vec3::ZERO, 1.0, dimension), layer) {
        let (from, to) = (voxear_core::math::Vec3::from_array(link.from), voxear_core::math::Vec3::from_array(link.to));
        let ways: &[(voxear_core::math::Vec3, voxear_core::math::Vec3)] =
            if link.both_ways { &[(from, to), (to, from)] } else { &[(from, to)] };
        for (a, b) in ways {
            links.push(voxear_nav::mesh::MeshLink { from: *a + offset, to: *b + offset, cost: link.cost });
            kinds.push(link.kind);
        }
    }
    (links, kinds)
}

/// Where a level's baked nav mesh is kept: beside it, named as it is, as
/// `Levels/Main.level`'s in `Levels/Main.nav`.
pub fn nav_file(level: &str) -> String {
    let stem = level.strip_suffix(".level").unwrap_or(level);
    format!("{stem}.nav")
}

/// The open level's baked nav mesh, for what walks its models rather than
/// blocks, in metres from the level's real middle (see `crate::origin`):
/// the Default agent type's.
#[derive(Clone, Debug)]
pub struct LevelNav(pub Arc<voxear_nav::mesh::NavMesh>);

/// The open level's nav meshes baked for the project's own agent types,
/// each by its slot, beside the Default type's [`LevelNav`].
#[derive(Clone, Debug, Default)]
pub struct TypeNavs(pub Vec<(u8, Arc<voxear_nav::mesh::NavMesh>)>);

/// Opens the current level's nav mesh, if it has one baked, in place of
/// any before.
pub fn put_level_nav(levels: &crate::levels::Levels, world: &mut World, content: &mut crate::Content) {
    world.remove_resource::<LevelNav>();
    world.remove_resource::<TypeNavs>();
    let Some(level) = levels.current() else { return };
    let path = nav_file(level);
    let Ok(bytes) = content.read(&path) else { return };
    match voxear_nav::mesh::meshes_from_bytes(&bytes) {
        Some(meshes) => put_meshes(world, meshes),
        None => tracing::warn!("{path} is not a nav mesh this engine reads; bake it again"),
    }
}

/// Puts a level's nav meshes in `world`, each by the slot of the agent type
/// it was baked for, in place of any before.
pub fn put_meshes(world: &mut World, meshes: Vec<(u8, voxear_nav::mesh::NavMesh)>) {
    world.remove_resource::<LevelNav>();
    let mut own = Vec::new();
    for (slot, mesh) in meshes {
        match slot {
            0 => world.insert_resource(LevelNav(Arc::new(mesh))),
            _ => own.push((slot, Arc::new(mesh))),
        }
    }
    world.insert_resource(TypeNavs(own));
}

/// The triangles a level's nav mesh is baked from: every still collider's
/// (neither a trigger nor moved by physics), in metres from the level's
/// real middle. Those of Nav Obstacles that carve are left out: they cut
/// the mesh where they stand while the game runs, and may be moved.
pub fn bake_triangles(world: &World) -> Vec<[voxear_core::math::Vec3; 3]> {
    let mut triangles = Vec::new();
    let obstacle = |entity: Entity| world.get::<crate::nav_obstacle::NavObstacle>(entity).is_some_and(|o| o.carve);
    for wall in crate::acoustics::walls_except(world, obstacle) {
        for [a, b, c] in &wall.triangles {
            let corner = |at: &u32| wall.vertices[*at as usize];
            triangles.push([corner(a), corner(b), corner(c)]);
        }
    }
    triangles
}

/// The ground of every terrain of a set size, as a level's nav mesh is
/// baked over it: its heights and holes, in metres from the level's real
/// middle. A terrain without end has none: its ground comes and goes.
pub fn bake_grounds(world: &World) -> Vec<voxear_nav::mesh::Ground> {
    let offset = voxear_scene::WorldOrigin::of(world).0.as_vec3();
    voxear_terrain::terrains(world)
        .into_iter()
        .filter(|(_, terrain, _)| terrain.origin.is_none())
        .map(|(_, terrain, corner)| {
            let map = &terrain.heights;
            let samples = map.resolution();
            let at = |n: u32| (i64::from(n % samples[0]), i64::from(n / samples[0]));
            let every = 0..samples[0] * samples[1];
            let heights = every.clone().map(|n| map.sample(at(n).0, at(n).1)).collect();
            let holes = match map.has_holes() {
                true => every.map(|n| map.is_hole(at(n).0, at(n).1)).collect(),
                false => Vec::new(),
            };
            voxear_nav::mesh::Ground { corner: corner + offset, spacing: map.spacing(), samples, heights, holes }
        })
        .collect()
}

/// The level's nav mesh and how far the world's origin is from the level's
/// real middle: a point in the world is the mesh's less that.
pub fn level_nav(world: &World) -> Option<(Arc<voxear_nav::mesh::NavMesh>, voxear_core::math::Vec3)> {
    let mesh = world.resource::<LevelNav>()?.0.clone();
    Some((mesh, voxear_scene::WorldOrigin::of(world).0.as_vec3()))
}

/// The level's nav mesh for agent type `slot`: the one baked for it, or the
/// Default type's where none was; and how far the world's origin is from
/// the level's real middle.
pub fn level_nav_for(world: &World, slot: u8) -> Option<(Arc<voxear_nav::mesh::NavMesh>, voxear_core::math::Vec3)> {
    let own = world.resource::<TypeNavs>().and_then(|navs| navs.0.iter().find(|(of, _)| *of == slot && slot > 0));
    match own {
        Some((_, mesh)) => Some((mesh.clone(), voxear_scene::WorldOrigin::of(world).0.as_vec3())),
        None => level_nav(world),
    }
}

/// The slots of the agent types the level's nav meshes are baked for.
pub fn baked_types(world: &World) -> Vec<u8> {
    let default = world.resource::<LevelNav>().map(|_| 0);
    let own = world.resource::<TypeNavs>().map(|navs| navs.0.iter().map(|(slot, _)| *slot).collect::<Vec<_>>());
    default.into_iter().chain(own.into_iter().flatten()).collect()
}
