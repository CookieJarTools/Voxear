//! One cell of generated ground: where its generator's Volume comes from,
//! and the block and fill a cell is from what the Volume says of it.
//!
//! A generator whose Output has a Volume wired in makes the ground in three
//! dimensions itself. For one that has none, the world's own overhangs and
//! caves (`BlockGenerate`'s numbers) are wired into a copy of its graph as
//! the nodes that make them (`generate::Graph::add_voxels`), so ground is
//! made one way whichever keeps the numbers.

use std::sync::{Arc, Mutex};

use super::{BlockGenerate, columns::Column};
use crate::{
    blocks::{
        chunk::{AIR, BlockId, Fill, fill_of},
        flow::{Water, surface_fill},
    },
    generate::{
        NodeKind, OwnVoxels, WorldGen,
        nodes::{Output, Rivers},
    },
};

/// How many blocks under the surface a world's own caves keep clear of, so
/// they open onto it only here and there.
pub const CAVE_ROOF: f32 = 3.0;

/// How short the slope of how far in the ground blocks are is taken to be
/// at least, in blocks a block, where a distance is measured against it. A
/// height made solid changes by a block for every block down, and faster
/// on a slope, where the number overstates how far the surface is. Where
/// pushing or a cave makes it change more slowly, the number is left as it
/// is: a block where it runs level is not taken to be far from a surface,
/// and blocks side by side are never measured so unlike that the deeper
/// reads as the nearer.
const LEAST_SLOPE: f32 = 1.0;

/// The generator a world of blocks' ground is made by.
pub(super) enum Source<'a> {
    /// The world's generator, whose graph has a Volume.
    Own(&'a WorldGen),
    /// The world's generator with the world's own overhangs and caves wired
    /// into its graph, and rivers where the world has them.
    Moved(Arc<WorldGen>),
    /// The world's generator, whose graph has a Volume, with rivers run down
    /// its ground; or one shaped by its biomes' land, with whatever it adds.
    /// Its Volume is read where the world lies in the generator's world.
    Rivered(Arc<WorldGen>),
}

impl Source<'_> {
    pub fn world(&self) -> &WorldGen {
        match self {
            Source::Own(world) => world,
            Source::Moved(world) | Source::Rivered(world) => world,
        }
    }

    /// Whether the Volume is the generator's own, read where the world lies
    /// in the generator's world. A world's own overhangs and caves are read
    /// from the world's corner, as they always were.
    pub fn own(&self) -> bool {
        matches!(self, Source::Own(_) | Source::Rivered(_))
    }
}

/// The generator the ground `generate` asks of `world` is made by, and its
/// structures built on: with the world's rivers, its biomes' land and its
/// own overhangs and caves wired in where it has them. What a dungeon's
/// layout must be planned on to be the one its chunks build.
pub fn ground_generator(generate: &BlockGenerate, world: &WorldGen, block_size: f32) -> Arc<WorldGen> {
    match source(generate, world, block_size) {
        Source::Own(world) => Arc::new(world.clone()),
        Source::Moved(world) | Source::Rivered(world) => world,
    }
}

/// The generators made with worlds' own overhangs and caves wired in, by
/// the generator's hash and the numbers' bits: the last few asked for, so
/// each chunk made does not compile its own.
type Moved = Vec<((u64, [u32; 3], bool, u64), Arc<WorldGen>)>;
static MOVED: Mutex<Moved> = Mutex::new(Vec::new());
const MOVED_KEPT: usize = 16;

/// How many hectares drain into a place before a river starts there, where
/// the land lies in levels.
const LEVEL_RIVERS: f32 = 24.0;

/// Where the land lies in levels, how wide and how deep a river is where a
/// square kilometre drains into it, in metres: wide and deep enough that a
/// player swims across all but the smallest.
const LEVEL_RIVER_WIDTH: f32 = 24.0;
const LEVEL_RIVER_DEPTH: f32 = 3.5;

/// Where the land lies in levels, great rivers run first, over points 256 m
/// apart: from where this many hectares drain into a point, carried on as
/// far as Run On says, across the lowland to the sea, so wide and deep
/// where a square kilometre drains into them.
const GREAT_RIVERS: f32 = 400.0;
const GREAT_SPACING: f32 = 256.0;
const GREAT_RUN_ON: f32 = 12_000.0;
const GREAT_WIDTH: f32 = 26.0;
const GREAT_DEPTH: f32 = 4.0;

/// The generator the ground `generate` asks of `world` is made by, for a
/// world of blocks `block_size` metres a side.
pub(super) fn source<'a>(generate: &BlockGenerate, world: &'a WorldGen, block_size: f32) -> Source<'a> {
    // Rivers run down the ground where the world has them and its generator makes no water of its own.
    let graph = world.graph();
    let watered = graph.output().is_some_and(|output| graph.link_into(output.id, Output::WATER).is_some());
    let rivered = generate.rivers && !watered;
    let voxels = world.volume().is_some();
    // The biomes' own land: their hills, mountains, tables and pillars, where any has some.
    let biomes = biomes_of(generate);
    let landed = shapes_land(generate) && !holds_written_land(graph);
    // Nodes written out of a world's land that keep to its regions, given them here where they are not yet.
    let attaching = regions_of(generate).filter(|_| keeps_to_world(graph));
    // Its painted maps, read by its landmass: a generator of its own, known by the maps' handle.
    let painted = generate.painted.0.clone();
    if voxels && !rivered && !landed && attaching.is_none() && painted.is_none() {
        return Source::Own(world);
    }
    let lands = if landed || attaching.is_some() { lands_hash(generate) } else { 0 };
    let lands = lands ^ painted.as_ref().map_or(0, |painted| painted.id.rotate_left(29));
    let own = OwnVoxels {
        overhangs: generate.overhangs.max(0.0),
        caves: generate.caves.clamp(0.0, 1.0),
        roof: CAVE_ROOF * block_size,
    };
    let key = (world.hash(), [own.overhangs, own.caves, own.roof].map(f32::to_bits), rivered, lands);
    // Ground shaped by its biomes' land is new ground: all of it is read where the world lies, pillars and bridges
    // with the rest.
    let made = |found: Arc<WorldGen>| if voxels || landed { Source::Rivered(found) } else { Source::Moved(found) };
    let mut moved = MOVED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, found)) = moved.iter().find(|(kept, _)| *kept == key) {
        return made(found.clone());
    }
    let mut graph = graph.clone();
    if let Some(regions) = attaching {
        attach(&mut graph, regions);
    }
    if let Some(painted) = &painted {
        attach_painted(&mut graph, painted);
    }
    if graph.output().is_none() {
        graph.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
    }
    // Shaped by the land already where the generator came so (see `landed`).
    let shaped = match world.land() {
        _ if !landed => crate::generate::land::Shaped::default(),
        Some((hash, shaped)) if *hash == lands => shaped.clone(),
        _ => crate::generate::land::add_land(&mut graph, biomes, world.sea_level(), clear_of(generate), regions_of(generate), generate.levels.as_ref()),
    };
    if rivered {
        add_rivers(&mut graph, generate);
    }
    if !voxels {
        graph.add_voxels(own, false);
    }
    crate::generate::land::add_land_volume(&mut graph, biomes, &shaped);
    let world = world.with_graph(graph);
    // The world's own overhangs and caves are read from its corner: by its chunks, and by what asks of the ground
    // under what is built on it.
    let world = Arc::new(if voxels || landed { world } else { world.with_volume_at_corner() });
    if moved.len() >= MOVED_KEPT {
        moved.remove(0);
    }
    moved.push((key, world.clone()));
    made(world)
}

/// Gives every landmass of `graph` the world's painted maps to read.
fn attach_painted(graph: &mut crate::generate::Graph, painted: &crate::generate::painted::Painted) {
    for node in graph.nodes.iter_mut() {
        if let NodeKind::Landmass(land) = &mut node.kind {
            land.painted = Some(painted.clone());
        }
    }
}

/// The biomes `generate` lists, as read; none before they are.
fn biomes_of(generate: &BlockGenerate) -> &[crate::generate::BiomeDef] {
    generate.placed.as_ref().map(|placed| placed.biomes.list()).unwrap_or_default()
}

/// The generators shaped by their worlds' biomes' land, by the generator's
/// hash and the biomes': the last few asked for.
type Landed = Vec<((u64, u64), Arc<WorldGen>)>;
static LANDED: Mutex<Landed> = Mutex::new(Vec::new());

/// `world` with its ground shaped by the land of the biomes `generate`
/// lists (see `generate::land`): the ground's height, climate and rivers as
/// the world of blocks makes them, for whatever asks about its ground
/// (where plants and structures stand, how warm it is, its tints). `world`
/// itself where no biome has land of its own. The ground of blocks made
/// from what it gives is the same as from `world`: its land is not shaped
/// twice.
pub fn landed(generate: &BlockGenerate, world: &Arc<WorldGen>) -> Arc<WorldGen> {
    let biomes = biomes_of(generate);
    if world.land().is_some() {
        return world.clone();
    }
    let attaching = regions_of(generate).filter(|_| keeps_to_world(world.graph()));
    let lays = shapes_land(generate) && !holds_written_land(world.graph());
    if !lays && attaching.is_none() {
        return world.clone();
    }
    let lands = lands_hash(generate);
    let key = (world.hash(), lands);
    let mut landed = LANDED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, found)) = landed.iter().find(|(kept, _)| *kept == key) {
        return found.clone();
    }
    let mut graph = world.graph().clone();
    if let Some(regions) = attaching {
        attach(&mut graph, regions);
    }
    let shaped = match lays {
        true => crate::generate::land::add_land(
            &mut graph,
            biomes,
            world.sea_level(),
            clear_of(generate),
            regions_of(generate),
            generate.levels.as_ref(),
        ),
        false => crate::generate::land::Shaped::default(),
    };
    let made = Arc::new(world.with_graph(graph).with_land(lands, shaped));
    if landed.len() >= MOVED_KEPT {
        landed.remove(0);
    }
    landed.push((key, made.clone()));
    made
}

/// Whether `generate`'s ground is shaped as it is made: by its biomes'
/// land, or by its levels.
fn shapes_land(generate: &BlockGenerate) -> bool {
    !generate.land_written
        && (generate.levels.is_some() || biomes_of(generate).iter().any(|biome| !biome.land.is_keep()))
}

/// Whether `generate` lays land into its generator's graph as it makes
/// ground: its levels or its biomes' land, not yet written into the
/// generator (see [`write_land`]).
pub fn lays_land(generate: &BlockGenerate) -> bool {
    shapes_land(generate)
}

/// Whether `graph` holds land written out of a world's (see [`write_land`]):
/// nodes that keep to the biome regions of the world it makes ground for,
/// given them or not. Land is not laid into it again, as when the world's
/// own step that wrote it is undone and the generator stays as written.
fn holds_written_land(graph: &crate::generate::Graph) -> bool {
    graph.nodes.iter().any(|node| match &node.kind {
        NodeKind::ClimateMask(mask) => mask.world_biome > 0,
        NodeKind::Levels(levels) => levels.world_areas,
        _ => false,
    })
}

/// Whether `graph` has nodes that keep to the biome regions of the world of
/// blocks it makes ground for and are not yet given them: Climate Masks of
/// a world's biome, and Levels whose areas are the world's.
fn keeps_to_world(graph: &crate::generate::Graph) -> bool {
    graph.nodes.iter().any(|node| match &node.kind {
        NodeKind::ClimateMask(mask) => mask.world_biome > 0 && mask.region.is_none(),
        NodeKind::Levels(levels) => levels.world_areas && levels.areas.is_none(),
        _ => false,
    })
}

/// Gives the nodes of `graph` that keep to a world's biome regions those
/// of `regions`: each Climate Mask its biome's regions, each Levels node
/// the regions' areas.
fn attach(graph: &mut crate::generate::Graph, regions: &Arc<crate::generate::Regions>) {
    let count = regions.biome_count();
    for node in &mut graph.nodes {
        match &mut node.kind {
            NodeKind::ClimateMask(mask) if mask.world_biome > 0 && (mask.world_biome as usize) <= count => {
                let biome = mask.world_biome as usize - 1;
                mask.region = Some(crate::generate::nodes::RegionOf { regions: regions.clone(), biome });
            }
            NodeKind::Levels(levels) if levels.world_areas => levels.areas = regions.areas().cloned(),
            _ => {}
        }
    }
}

/// The nodes of `graph` given a world's regions as it shaped its land, as
/// a file keeps them in their place: each Climate Mask the number of the
/// biome whose regions it keeps to, and each Levels node that its areas are
/// the world's.
fn detach(graph: &mut crate::generate::Graph) {
    for node in &mut graph.nodes {
        match &mut node.kind {
            NodeKind::ClimateMask(mask) => {
                if let Some(region) = mask.region.take() {
                    mask.world_biome = region.biome as u32 + 1;
                }
            }
            NodeKind::Levels(levels) => {
                if levels.areas.take().is_some() {
                    levels.world_areas = true;
                }
            }
            _ => {}
        }
    }
}

/// Runs the rivers `generate` has down `graph`'s ground: in a world of
/// levels, great rivers first and the rest down what they leave.
fn add_rivers(graph: &mut crate::generate::Graph, generate: &BlockGenerate) {
    match great_rivers_of(generate) {
        Some(great) => crate::generate::nodes::add_great_rivers(graph, great, rivers_of(generate)),
        None => crate::generate::nodes::add_rivers(graph, rivers_of(generate)),
    };
}

/// The great rivers `generate` runs down its ground before the rest, where
/// its land lies in levels: few, from far up the uplands, broad and deep,
/// and carried on across the lowland to the sea.
fn great_rivers_of(generate: &BlockGenerate) -> Option<Rivers> {
    generate.levels.as_ref()?;
    Some(Rivers {
        spacing: GREAT_SPACING,
        source: GREAT_RIVERS,
        width: GREAT_WIDTH,
        depth: GREAT_DEPTH,
        valley: 160.0,
        falls: 8.0,
        lakes: 0.0,
        run_on: GREAT_RUN_ON,
        seed: 1,
        ..Rivers::default()
    })
}

/// The rivers `generate` runs down its ground. Land in levels lies on long
/// even slopes, down which water runs side by side: fewer rivers, each from
/// more ground, and wider, read better there.
fn rivers_of(generate: &BlockGenerate) -> Rivers {
    match generate.levels {
        Some(_) => {
            Rivers { source: LEVEL_RIVERS, width: LEVEL_RIVER_WIDTH, depth: LEVEL_RIVER_DEPTH, ..Rivers::default() }
        }
        None => Rivers::default(),
    }
}

/// The graph of `world`, the generator a world of blocks' ground is made
/// by as its file has it, with the land `generate` lays into it as it
/// makes ground written in as nodes of its own: its levels, its biomes'
/// land, the rivers and its own overhangs and caves where it lays them in,
/// and its biomes' pillars, bridges and overhangs. The nodes are those the
/// world lays in, numbered as it numbers them, so the ground is the same
/// once the world lays none in itself (`BlockGenerate::land_written`); they
/// are laid out under the graph's own, each part in a comment saying what
/// it does. `names` are the biomes' names, in the order `generate` lists
/// them. None where the world lays no land in.
pub fn write_land(
    generate: &BlockGenerate,
    world: &WorldGen,
    block_size: f32,
    names: &[String],
) -> Option<crate::generate::Graph> {
    use crate::generate::{land, written_land};
    if !shapes_land(generate) || holds_written_land(world.graph()) {
        return None;
    }
    let biomes = biomes_of(generate);
    let mut graph = world.graph().clone();
    let watered = graph.output().is_some_and(|output| graph.link_into(output.id, Output::WATER).is_some());
    let rivered = generate.rivers && !watered;
    let voxels = world.volume().is_some();
    let own = OwnVoxels {
        overhangs: generate.overhangs.max(0.0),
        caves: generate.caves.clamp(0.0, 1.0),
        roof: CAVE_ROOF * block_size,
    };
    // As `source` lays them in, step by step, noting which nodes each step adds.
    let mut steps = written_land::Steps::default();
    let mark = |graph: &crate::generate::Graph| graph.nodes.iter().map(|node| node.id).max().map_or(0, |id| id.0);
    if graph.output().is_none() {
        graph.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
    }
    let first = mark(&graph);
    let shaped = land::add_land(
        &mut graph,
        biomes,
        world.sea_level(),
        clear_of(generate),
        regions_of(generate),
        generate.levels.as_ref(),
    );
    steps.land = (first, mark(&graph));
    if rivered {
        add_rivers(&mut graph, generate);
    }
    steps.rivers = (steps.land.1, mark(&graph));
    if !voxels {
        graph.add_voxels(own, false);
    }
    steps.voxels = (steps.rivers.1, mark(&graph));
    land::add_land_volume(&mut graph, biomes, &shaped);
    steps.volume = (steps.voxels.1, mark(&graph));
    detach(&mut graph);
    written_land::lay_out(&mut graph, &steps, biomes, names, generate.levels.is_some());
    Some(graph)
}

/// The regions `generate`'s biomes keep to, where they keep to any.
fn regions_of(generate: &BlockGenerate) -> Option<&Arc<crate::generate::Regions>> {
    generate.placed.as_ref()?.regions.as_ref()
}

/// The ground round where `generate`'s world lies that its biomes' land
/// keeps clear of, where it keeps clear of any.
fn clear_of(generate: &BlockGenerate) -> Option<crate::generate::land::Clear> {
    let middle = generate.position;
    (generate.land_from > 0.0).then_some(crate::generate::land::Clear { middle, radius: generate.land_from })
}

/// A hash of how `generate`'s biomes lie and the land each shapes, and of
/// where it keeps clear, for knowing one
/// world's shaped generator from another's.
fn lands_hash(generate: &BlockGenerate) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let clear = clear_of(generate).map(|clear| [clear.middle[0], clear.middle[1], clear.radius].map(f32::to_bits));
    clear.hash(&mut hasher);
    regions_of(generate).map(|regions| regions.id()).hash(&mut hasher);
    generate.levels.as_ref().map(|levels| format!("{levels:?}")).hash(&mut hasher);
    for biome in biomes_of(generate) {
        let land = biome.land;
        let numbers = [land.height, land.size, land.valleys, land.overhangs, land.pillars, land.pillar_height];
        (land.form as u8, numbers.map(f32::to_bits), land.bridges.to_bits()).hash(&mut hasher);
        (biome.temperature.map(f32::to_bits), biome.humidity.map(f32::to_bits)).hash(&mut hasher);
    }
    hasher.finish()
}

/// How deep under the surface a block of ground is, in blocks, for its
/// layer, counted down its column from the open air: `above` is the depth
/// of the block over it (nothing for open air), and `ground` how far into
/// the ground under the caves its own middle is. The first block under the
/// air is as deep as its middle, a block at most, and each under it a
/// block deeper, whatever pushed the ground about on the way.
pub(super) fn counted(above: f32, ground: f32) -> f32 {
    if ground <= 0.0 {
        0.0
    } else if above > 0.0 {
        above + 1.0
    } else if ground < 1.0 {
        ground
    } else {
        1.0
    }
}

/// How far from the surface a block's middle is, in blocks, from how far
/// into the ground the Volume puts it (`inside`) and those either side of
/// it along each axis (`beside`, as pairs before and after): the number
/// over how fast it changes from block to block, which is its distance
/// where the ground is steep or pushed about and the number alone is not.
pub(super) fn distance(inside: f32, beside: [[f32; 2]; 3]) -> f32 {
    let [x, y, z] = beside.map(|[before, after]| (after - before) * 0.5);
    let slope = (x * x + y * y + z * z).sqrt();
    inside / if slope > LEAST_SLOPE { slope } else { LEAST_SLOPE }
}

/// How steep a slope the ground's layers still follow in a world drawn
/// smooth, as how much of the way the ground deepens is downwards: three
/// tenths, for ground up to about 70° from level. Steeper is a wall.
const SKIN_FACING: f32 = 0.3;

/// How deep under the surface a block of ground is, for its layer, in a
/// world drawn smooth whose ground is more than a height. Counted down its
/// column alone, every step down a slope would show the layer under the
/// top one, in stripes. So where the ground faces up, a block is no deeper
/// than it is from the open air, and the layers follow the slope as a
/// skin; and on a wall, which nothing lies on, it is deeper than every
/// layer, so walls and the undersides of overhangs are bare.
///
/// `counted` is its depth counted down its column, `ground` how far into
/// the ground under the caves it is, `beside` the same for the blocks
/// either side of it along each axis, and `up` the way up.
pub(super) fn skinned(counted: f32, ground: f32, beside: [[f32; 2]; 3], up: [f32; 3]) -> f32 {
    let slope = beside.map(|[before, after]| (after - before) * 0.5);
    let length = (slope[0] * slope[0] + slope[1] * slope[1] + slope[2] * slope[2]).sqrt();
    let downwards = -(slope[0] * up[0] + slope[1] * up[1] + slope[2] * up[2]);
    if downwards <= length * SKIN_FACING {
        return f32::MAX;
    }
    let from_air = ground / if length > LEAST_SLOPE { length } else { LEAST_SLOPE };
    if from_air < counted { from_air } else { counted }
}

/// The sea at one cell, or a river's or a lake's: the block it is, air for
/// none; how many blocks under its top the cell's middle is; for flat
/// ground of cubes, how many eighths of the cell it fills, a coarser level's
/// top block of it only partly, so every level's sea stands as high;
/// whether it falls there, a waterfall's curtain; and, of cubes, which way
/// it flows along a current, a river's, which its blocks keep with how high
/// each stands.
#[derive(Clone, Copy)]
pub(super) struct Sea {
    pub block: BlockId,
    pub over: f32,
    pub eighths: Option<f32>,
    pub falling: bool,
    pub current: Option<[f32; 2]>,
}

impl Sea {
    /// How the sea of `block`, its top `top` blocks up, stands at block `y`:
    /// of cubes, how many eighths of the block it fills, a coarser level's
    /// top block of it only partly.
    pub fn at(block: BlockId, top: f32, y: i32, smooth: bool) -> Sea {
        let eighths = (!smooth).then(|| ((top - y as f32).min(1.0) * 8.0).round());
        Sea { block, over: top - (y as f32 + 0.5), eighths, falling: false, current: None }
    }
}

/// What one cell is: its block, and its fill where it keeps one; none for
/// air that keeps neither.
///
/// `inside` is how far into the ground its middle is, in blocks, and
/// `ground` the same with the caves left whole, so out of both is open
/// air. `deep` is its depth under the surface, for its layer, and `filled`
/// what its fill is made from in a world drawn `smooth`.
pub(super) fn cell(
    column: &Column,
    inside: f32,
    ground: f32,
    deep: f32,
    filled: f32,
    sea: Sea,
    smooth: bool,
) -> Option<(BlockId, Option<Fill>)> {
    let block = if inside > 0.0 { column.block(deep) } else { AIR };
    let open = ground <= 0.0;
    // The sea fills the open air below its level. Drawn smooth, it also takes the ground's top block where the
    // ground's surface lies under the sea and the block over it stands clear of it, which would else leave no sea
    // there: so the sea's surface reaches the shore, and keeps its floor.
    let shore = smooth && block != AIR && sea.over <= 1.0 && ground < sea.over;
    let wet = match sea.eighths {
        Some(eighths) if !smooth => open && eighths >= 1.0,
        _ => sea.over > 0.0 && (open || shore),
    };
    if wet {
        let fill = match (smooth, sea.eighths) {
            // A waterfall's curtain falls, as water poured over a ledge does.
            (false, _) if sea.falling => Some(Water::Falling.fill()),
            // The sea's top blocks keep where its surface lies.
            (true, _) => Some(surface_fill(sea.over)),
            // A river's blocks keep which way it flows and how high each stands.
            (false, Some(eighths)) if sea.current.is_some() => {
                sea.current.map(|flow| crate::blocks::flow::current_fill(flow, eighths as u8))
            }
            // Kept as flowing water keeps how high it stands; a coarse level's blocks never flow.
            (false, Some(eighths)) if eighths < 8.0 => Some(Water::Flowing { level: eighths as u8, up: 2 }.fill()),
            (false, _) => None,
        };
        Some((sea.block, fill))
    } else if smooth && (block != AIR || filled > -1.0) {
        // The air a block from the surface keeps how far it is from it too.
        Some((block, Some(fill_of(filled, block != AIR))))
    } else if block != AIR {
        Some((block, None))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "written_tests.rs"]
mod written_tests;
