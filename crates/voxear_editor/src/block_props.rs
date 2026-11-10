//! A Block World in the Inspector: its tools, **Build** and **Settings**.
//! Under Build, the blocks it lists (see `block_panel`), the brush the
//! Scene view builds with (see `block_tools`), and the picked block: which
//! `.block` it is, with its colour and materials, or for a kind of the
//! world's own its name, colour and materials. Under Settings, the
//! `.blockworld` asset its blocks are, how large a block is, the flat
//! ground its blocks start as, and whether it casts the sun's shadows. A
//! change to the asset's file makes its blocks again at once, or for a
//! large world on other threads, shown as it was until then; the changes
//! built since are kept on them, and kept in the world for saving and undo
//! as a Terrain's are. A change to a block it lists is the block's own: written
//! to its `.block` at once, taken up by every world listing it, and no
//! step to undo.

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    blocks::{BlockWorldAsset, Resolved},
    prefab::BlockWorldParts,
    scene_file::{BlockWorldData, BlockWorldSource},
};
use voxear_render::{BlockLook, Renderer};

use voxear_terrain::{
    blocks::{
        BlockMap, BlockWorld,
        block::{BlockDef, Liquid},
        chunk::{AIR, BlockId},
        edits::{self, BlockEdits, Resize},
        file::{BlockCover, BlockFile, BlockKind, MAX_BLOCKS, MAX_LOOKS},
        generate::BlockGenerate,
        vox::VoxFile,
    },
    generate::WorldGen,
    surfaces::Biome,
};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Where Add Component puts a new world of blocks' asset, and a new Voxel
/// Terrain's; and where blocks made from a world's list go.
const FOLDER: &str = "Blocks";
const SMOOTH_FOLDER: &str = "Voxels";
const BLOCKS_FOLDER: &str = "Blocks";

/// Which of a Block World's tools its Inspector shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlockTool {
    #[default]
    Build,
    Settings,
}

impl BlockTool {
    pub const ALL: [BlockTool; 2] = [BlockTool::Build, BlockTool::Settings];

    pub fn name(self) -> &'static str {
        match self {
            BlockTool::Build => "Build",
            BlockTool::Settings => "Settings",
        }
    }
}

/// What the brush does where it is clicked: puts the picked block against
/// the face under it, takes away the block under it, turns the blocks under
/// it into the picked one, or builds the picked model of blocks standing on
/// the face under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlockBrush {
    #[default]
    Place,
    Break,
    Paint,
    Model,
}

impl BlockBrush {
    pub const ALL: [BlockBrush; 4] = [BlockBrush::Place, BlockBrush::Break, BlockBrush::Paint, BlockBrush::Model];

    pub fn name(self) -> &'static str {
        match self {
            BlockBrush::Place => "Place",
            BlockBrush::Break => "Break",
            BlockBrush::Paint => "Paint",
            BlockBrush::Model => "Model",
        }
    }
}

/// What the editor is doing with worlds of blocks: the tool shown, the
/// block picked, by its place in the world's list (one less than its
/// number), the brush, how many blocks across it reaches and the `.vox`
/// model it builds. Kept in the edit world, not saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockUi {
    pub tool: BlockTool,
    pub block: usize,
    pub brush: BlockBrush,
    pub size: u32,
    pub model: Option<AssetGuid>,
}

impl Default for BlockUi {
    fn default() -> Self {
        Self { tool: BlockTool::Build, block: 0, brush: BlockBrush::Place, size: 1, model: None }
    }
}

/// Worlds' files changed while a number is dragged by its label, kept
/// until it is let go: each world is made again once from the last, not at
/// every step of the drag, as making a large one again takes a while.
#[derive(Default)]
struct HeldFiles {
    holding: bool,
    files: std::collections::HashMap<Entity, BlockFile>,
}

/// The worlds whose generator's graph has a Volume, and so makes their
/// overhangs and caves itself: their own Overhangs and Caves are not shown.
#[derive(Default, PartialEq)]
struct GeneratorVolumes(std::collections::HashSet<Entity>);

/// Whether the world's generator makes its overhangs and caves itself, as
/// last seen (see [`hold`]).
pub fn generator_has_volume(world: &World, entity: Entity) -> bool {
    world.resource::<GeneratorVolumes>().is_some_and(|volumes| volumes.0.contains(&entity))
}

/// Whether the world's own overhangs and caves can move into its
/// generator: its ground is made by a generator that can be read and has
/// no Volume yet.
pub fn can_move_to_generator(game: &Game, entity: Entity) -> bool {
    let world = &game.engine.world;
    let reference = world.get::<BlockWorldAsset>(entity).and_then(|asset| asset.file.generate.as_ref()?.world.clone());
    let guid = reference.and_then(|r| game.content.parse(&r).ok()).and_then(|parsed| parsed.guid());
    guid.is_some_and(|guid| game.content.db().get(guid).is_some()) && !generator_has_volume(world, entity)
}

/// Sees which worlds' generators make their overhangs and caves
/// themselves, for the rows a world shows.
fn see_generators(game: &mut Game) {
    let generated: Vec<(Entity, String)> = game
        .engine
        .world
        .query_ref::<(Entity, &BlockWorldAsset)>()
        .into_iter()
        .filter_map(|(entity, asset)| Some((entity, asset.file.generate.as_ref()?.world.clone()?)))
        .collect();
    let content = &mut game.content;
    let mut own = |reference: &str| content.world_gen(reference).is_some_and(|world| world.volume().is_some());
    let seen = GeneratorVolumes(
        generated.into_iter().filter(|(_, reference)| own(reference)).map(|(entity, _)| entity).collect(),
    );
    if game.engine.world.resource::<GeneratorVolumes>() != Some(&seen) {
        game.engine.world.insert_resource(seen);
    }
}

/// Moves the world's own overhangs and caves into its generator: wired
/// into the generator's graph as the nodes that make them, their amounts
/// shown as rows of the generator's own, and none left on the world. The
/// generator is saved, and every world it makes ground for is made again
/// from it. The ground of a world placed at its generator's own corner is
/// what it was; one placed elsewhere has its caves where the generator's
/// are there.
pub fn move_to_generator(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else { return false };
    let Some(generate) = file.generate.as_mut() else { return false };
    let Some(reference) = generate.world.clone() else { return false };
    let Some(guid) = game.content.parse(&reference).ok().and_then(|parsed| parsed.guid()) else { return false };
    let Some(world) = game.content.world_gen(&reference).filter(|world| world.volume().is_none()) else {
        return false;
    };
    let Some(path) = game.content.db().get(guid).map(|entry| entry.path.clone()) else { return false };
    let own = voxear_terrain::generate::OwnVoxels {
        overhangs: generate.overhangs.max(0.0),
        caves: generate.caves.clamp(0.0, 1.0),
        roof: voxear_terrain::blocks::generate::CAVE_ROOF * file.block_size,
    };
    let mut graph = world.graph().clone();
    if !graph.add_voxels(own, true) {
        return false;
    }
    let text = format!("{}{}", crate::project_files::NEW_WORLD_GEN, world.with_graph(graph).to_text());
    if let Err(error) = std::fs::write(game.project.root.join(&path), text) {
        tracing::error!("could not save {path}: {error}");
        return false;
    }
    (generate.overhangs, generate.caves) = (0.0, 0.0);
    if game.content.forget_world_gen(guid) {
        crate::terrain_props::regenerate(game, guid);
        regenerate(game, guid);
    }
    see_generators(game);
    change_file(game, renderer, entity, file)
}

/// Holds changes to worlds' files from now on while `dragging`, a number
/// dragged by its label; once it is not, makes each world held again from
/// its last file. The Inspector calls it each frame before it reads its
/// rows, so a world made again on letting go is one edit.
pub fn hold(game: &mut Game, mut renderer: Option<&mut Renderer>, dragging: bool) {
    see_generators(game);
    let world = &mut game.engine.world;
    if world.resource::<HeldFiles>().is_none() {
        if !dragging {
            return;
        }
        world.insert_resource(HeldFiles::default());
    }
    let held = world.resource_mut::<HeldFiles>().expect("inserted above");
    held.holding = dragging;
    if dragging || held.files.is_empty() {
        return;
    }
    for (entity, file) in std::mem::take(&mut held.files) {
        change_file(game, renderer.as_deref_mut(), entity, file);
    }
}

/// The file a world of blocks shows: the one held for it while a number is
/// dragged, the one it is being made again from, or its asset's.
fn shown_file(world: &World, entity: Entity) -> Option<&BlockFile> {
    let held = world.resource::<HeldFiles>().and_then(|held| held.files.get(&entity));
    held.or_else(|| making(world, entity).map(|remake| &remake.file))
        .or_else(|| world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file))
}

/// The file a change to a world of blocks starts from: the one it is being
/// made again from, or its asset's.
fn file_of(world: &World, entity: Entity) -> Option<BlockFile> {
    let making = making(world, entity).map(|remake| &remake.file);
    making.or_else(|| world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file)).cloned()
}

/// A world of blocks' file as it is drawn, each block it lists with that
/// block's look: the one it is being made again from, or its asset's.
fn drawn_of(world: &World, entity: Entity) -> Option<&BlockFile> {
    let making = making(world, entity).map(|remake| &**remake.drawn);
    making.or_else(|| world.get::<BlockWorldAsset>(entity).map(|asset| &**asset.drawn))
}

/// Worlds of blocks being made again on other threads from a changed file,
/// each too large to make within a frame: until its new blocks are made, a
/// world shows what it was and its Inspector the file it is made from (see
/// [`finish`]).
#[derive(Default)]
struct Making(std::collections::HashMap<Entity, Remake>);

/// A world of blocks being made again: the file it is made from, as it is
/// drawn and at which version of the blocks, what it was made from when it
/// started (its file, its ground and its blocks), and where its new ground
/// and world come once made.
struct Remake {
    file: BlockFile,
    drawn: Arc<Resolved>,
    derived: u64,
    from: (BlockFile, Arc<BlockMap>, Arc<BlockMap>),
    made: Arc<Mutex<Option<(Arc<BlockMap>, BlockWorld)>>>,
}

/// How many columns of blocks a world can have and still be made again
/// within a frame; past it, it is made on other threads (see [`Making`]). A
/// smooth one's count four times, as its faces take longer to work out.
const MADE_AT_ONCE: f32 = 65_536.0;

/// Whether `file`'s world takes too long to make within a frame.
fn made_slowly(file: &BlockFile) -> bool {
    let size = file.block_size.max(0.01);
    let columns = match file.generate.as_ref().and_then(|generate| generate.planet) {
        Some(planet) => 4.0 * std::f32::consts::PI * (planet.radius / size).powi(2),
        None => file.ground.size[0] as f32 * file.ground.size[1] as f32,
    };
    columns * if file.smooth { 4.0 } else { 1.0 } > MADE_AT_ONCE
}

fn making(world: &World, entity: Entity) -> Option<&Remake> {
    world.resource::<Making>().and_then(|making| making.0.get(&entity))
}

/// The worlds of blocks made on other threads, waiting for [`finish`].
pub fn made(world: &World) -> Vec<Entity> {
    let Some(making) = world.resource::<Making>() else { return Vec::new() };
    let done = |remake: &Remake| remake.made.lock().is_ok_and(|made| made.is_some());
    making.0.iter().filter(|(_, remake)| done(remake)).map(|(entity, _)| *entity).collect()
}

/// Puts in world `entity` as it was made on other threads, its asset given
/// the file it was made from; whether it was. One whose blocks, ground or
/// file changed while it was made, as by building, is made again from what
/// it is now instead.
pub fn finish(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    let world = &mut game.engine.world;
    let Some((ground, mut made)) = making(world, entity).and_then(|remake| remake.made.lock().ok()?.take()) else {
        return false;
    };
    let remake = world.resource_mut::<Making>().and_then(|making| making.0.remove(&entity)).expect("found above");
    let (old, base, blocks) = &remake.from;
    let asset =
        world.get::<BlockWorldAsset>(entity).filter(|asset| asset.file == *old && Arc::ptr_eq(&asset.base, base));
    let now = world.get::<BlockWorld>(entity).filter(|now| Arc::ptr_eq(&now.blocks, blocks));
    let (Some(_), Some(now)) = (asset, now) else {
        change_file(game, renderer, entity, remake.file);
        return false;
    };
    made.cast_shadows = now.cast_shadows;
    world.insert(entity, made);
    if let Some(asset) = world.get_mut::<BlockWorldAsset>(entity) {
        (asset.file, asset.base, asset.drawn, asset.derived) =
            (remake.file, ground, remake.drawn.clone(), remake.derived);
    }
    restyle(game, renderer, entity, &remake.drawn);
    true
}

/// Lets go of every world of blocks being made again, each left as it is,
/// as when an edit is undone or done again.
pub fn stop_making(world: &mut World) {
    if let Some(making) = world.resource_mut::<Making>() {
        making.0.clear();
    }
}

/// Starts making world `entity` again from `file`, drawn as `drawn` at
/// blocks' version `derived`, on another thread, from what it is now
/// (`from`: its file, its ground and its world), kept in [`Making`] until
/// [`finish`] puts it in.
fn start_making(
    world: &mut World,
    entity: Entity,
    (file, drawn, derived): (BlockFile, Arc<Resolved>, u64),
    (old, base, blocks): (BlockFile, Arc<BlockMap>, BlockWorld),
    carried: Option<BlockEdits>,
    generator: Option<Arc<WorldGen>>,
) {
    let made = Arc::new(Mutex::new(None));
    let from = (old.clone(), base.clone(), blocks.blocks.clone());
    let (job, making, drawing) = (made.clone(), file.clone(), drawn.clone());
    voxear_terrain::workers::spawn(move || {
        let ground = Arc::new(making.make_ground(generator.as_deref()));
        // Let go of while its ground was made, as when another file took its place: nothing more to make.
        if Arc::strong_count(&job) == 1 {
            return;
        }
        let world = made_on((&making, &drawing), &ground, (&old, &base, &blocks), carried, None);
        *job.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((ground, world));
    });
    if world.resource::<Making>().is_none() {
        world.insert_resource(Making::default());
    }
    let remake = Remake { file, drawn, derived, from, made };
    world.resource_mut::<Making>().expect("inserted above").0.insert(entity, remake);
}

/// The editor's block tools as they are.
pub fn ui(world: &World) -> BlockUi {
    world.resource::<BlockUi>().copied().unwrap_or_default()
}

/// The editor's block tools, to change.
pub fn ui_mut(world: &mut World) -> &mut BlockUi {
    if world.resource::<BlockUi>().is_none() {
        world.insert_resource(BlockUi::default());
    }
    world.resource_mut::<BlockUi>().expect("inserted above")
}

/// A value a Block World shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockField {
    Data,
    BlockSize,
    /// Whether it is drawn smooth: a Voxel Terrain.
    Smooth,
    /// How many blocks its ground reaches along x and z, and how many up.
    GroundSize,
    GroundHeight,
    Shadows,
    /// Whether its ground is flat or made by a world generator, and then
    /// which generator, from where in its world, down to which block, how
    /// far its surface is pushed in and out, and how much is caves.
    Ground,
    Generator,
    /// For a planet, how far its sea is from its middle.
    Radius,
    Offset,
    Lowest,
    Overhangs,
    Caves,
    /// Whether its generated ground goes on without end, made round the
    /// camera, and how far its own chunks and its coarser ones reach.
    Endless,
    ViewDistance,
    FarDistance,
    /// The blocks its ground is made of: its top, the layers under it and
    /// how deep they go, and the rest; and the block its generated ground's
    /// sea is filled with.
    TopBlock,
    UnderBlock,
    UnderDepth,
    BaseBlock,
    SeaBlock,
    /// The brush: what it does, and how many blocks across or the model it
    /// builds.
    Brush,
    BrushSize,
    Model,
    /// The `.block` the picked entry is, and the one a new entry is of.
    Block,
    NewBlock,
    /// The picked block's name where it is a kind of the world's own,
    /// whether it is water, its colour and materials.
    Name,
    Water,
    Color,
    Material,
    Top,
    Bottom,
    /// Where the picked kind covers generated ground: whether it does, by
    /// which biome, between which temperatures, humidities and heights, and
    /// how many blocks deep.
    Cover,
    CoverBiome,
    TemperatureMask,
    Temperatures,
    HumidityMask,
    Humidities,
    HeightMask,
    Heights,
    CoverDepth,
}

use BlockField as F;

/// The values a Block World shows, for the tool showing: its asset's where
/// it has one.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let state = ui(world);
    let file = drawn_of(world, entity);
    let fields = match (state.tool, file) {
        (BlockTool::Build, Some(file)) => {
            let mut fields = vec![F::Brush, if state.brush == BlockBrush::Model { F::Model } else { F::BrushSize }];
            if let Some(kind) = picked(world, file).map(|at| &file.blocks[at]) {
                // A block is named by its file; a kind of the world's own, here.
                fields.push(if kind.block.is_some() { F::Block } else { F::Name });
                fields.extend([F::Water, F::Color]);
                // Water is drawn with its colour alone.
                if !kind.water {
                    fields.extend([F::Material, F::Top, F::Bottom]);
                }
                // Covers lay blocks on generated ground; its sea, water.
                if file.generate.is_some() && !kind.water {
                    fields.push(F::Cover);
                    if let Some(cover) = &kind.cover {
                        fields.extend([F::CoverBiome, F::TemperatureMask]);
                        fields.extend(cover.temperature.map(|_| F::Temperatures));
                        fields.push(F::HumidityMask);
                        fields.extend(cover.humidity.map(|_| F::Humidities));
                        fields.push(F::HeightMask);
                        fields.extend(cover.height.map(|_| F::Heights));
                        fields.push(F::CoverDepth);
                    }
                }
            }
            fields
        }
        (BlockTool::Build, None) => vec![F::Data],
        (BlockTool::Settings, Some(file)) => {
            let mut fields = vec![F::Data, F::BlockSize, F::Smooth, F::Ground];
            // A generator whose graph has a Volume makes the overhangs and caves itself.
            let own_voxels = if generator_has_volume(world, entity) { vec![] } else { vec![F::Overhangs, F::Caves] };
            match file.generate {
                // A planet lies all round its middle, made at once: no size, place, lowest block or end.
                Some(ref generate) if generate.planet.is_some() => {
                    fields.extend([F::Generator, F::Radius]);
                    fields.extend(own_voxels);
                }
                Some(ref generate) => {
                    fields.extend([F::Generator, F::Offset, F::Endless]);
                    match generate.endless {
                        Some(_) => fields.extend([F::ViewDistance, F::FarDistance]),
                        None => fields.push(F::GroundSize),
                    }
                    fields.push(F::Lowest);
                    fields.extend(own_voxels);
                }
                None => fields.extend([F::GroundSize, F::GroundHeight]),
            }
            fields.extend([F::TopBlock, F::UnderBlock, F::UnderDepth, F::BaseBlock]);
            if file.generate.is_some() {
                fields.push(F::SeaBlock);
            }
            fields.push(F::Shadows);
            fields
        }
        (BlockTool::Settings, None) => vec![F::Data, F::Shadows],
    };
    fields.into_iter().map(Prop::BlockWorld).collect()
}

pub fn label(field: BlockField) -> &'static str {
    match field {
        F::Data => "Data",
        F::BlockSize => "Block size",
        F::Smooth => "Smooth",
        F::GroundSize => "Ground size",
        F::GroundHeight => "Ground height",
        F::Shadows => "Cast shadows",
        F::Brush => "Brush",
        F::BrushSize => "Size",
        F::Model => "Model",
        F::Block | F::NewBlock => "Block",
        F::Name => "Name",
        F::Water => "Water",
        F::Color => "Color",
        F::Material => "Material",
        F::Top => "Top",
        F::Bottom => "Bottom",
        F::Ground => "Ground",
        F::Generator => "Generator",
        F::Radius => "Radius",
        F::Offset => "Offset",
        F::Lowest => "Lowest block",
        F::Overhangs => "Overhangs",
        F::Caves => "Caves",
        F::Endless => "Endless",
        F::ViewDistance => "View distance",
        F::FarDistance => "Far distance",
        F::TopBlock => "Top block",
        F::UnderBlock => "Under block",
        F::UnderDepth => "Under depth",
        F::BaseBlock => "Base block",
        F::SeaBlock => "Sea block",
        F::Cover => "Cover",
        F::CoverBiome => "Biome",
        F::TemperatureMask => "Temperature",
        F::Temperatures => "Temperatures",
        F::HumidityMask => "Humidity",
        F::Humidities => "Humidities",
        F::HeightMask => "Height",
        F::Heights => "Heights",
        F::CoverDepth => "Depth",
    }
}

/// The name a prefab's locks give the value.
pub fn key(field: BlockField) -> &'static str {
    match field {
        F::Data => "data",
        F::Shadows => "cast_shadows",
        // The rest are the asset's or the editor's, not the scene's.
        _ => "",
    }
}

pub fn unit(field: BlockField) -> Option<&'static str> {
    match field {
        F::BlockSize | F::Offset | F::Overhangs | F::Heights | F::ViewDistance | F::FarDistance | F::Radius => {
            Some("m")
        }
        F::Temperatures => Some("°C"),
        _ => None,
    }
}

/// The range a value's slider spans, for those shown as sliders.
pub fn range(field: BlockField) -> Option<[f32; 2]> {
    match field {
        F::Caves => Some([0.0, 1.0]),
        _ => None,
    }
}

/// Whether the value is an asset picked from a list.
pub fn is_picked(field: BlockField) -> bool {
    matches!(field, F::Data | F::Material | F::Top | F::Bottom | F::Generator | F::Model | F::Block | F::NewBlock)
}

/// The place in `file`'s list of the block picked, where it is one still:
/// none once it was taken out, or with nothing listed.
pub(crate) fn picked(world: &World, file: &BlockFile) -> Option<usize> {
    let at = ui(world).block;
    file.blocks.get(at).filter(|kind| kind.is_live()).map(|_| at)
}

/// The numbers of the blocks `file` lists, those taken out left out.
fn listed(file: &BlockFile) -> Vec<BlockId> {
    (1..=file.blocks.len()).filter(|&at| file.blocks[at - 1].is_live()).map(|at| at as BlockId).collect()
}

/// A choice of the world's blocks, air first, with `block` chosen.
fn block_choice(file: &BlockFile, block: BlockId) -> Value {
    let listed = listed(file);
    let mut names = vec!["Air".to_string()];
    names.extend(listed.iter().map(|&block| file.blocks[usize::from(block) - 1].name.clone()));
    Value::Choice(listed.iter().position(|&known| known == block).map_or(0, |at| at + 1), names)
}

/// The block chosen at `at` of a choice of the world's blocks, air first.
fn chosen_block(file: &BlockFile, at: usize) -> BlockId {
    at.checked_sub(1).and_then(|at| listed(file).get(at).copied()).unwrap_or(AIR)
}

/// An asset's path, by the reference a file holds, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

pub fn get(game: &Game, entity: Entity, field: BlockField) -> Option<Value> {
    let world = &game.engine.world;
    // What its rows show: its numbers from its file, as it is held while one is dragged (see `hold`), and each
    // block it lists with that block's own name and look.
    let file = || shown_file(world, entity);
    let drawn = || drawn_of(world, entity);
    let entry = || file().and_then(|file| Some((picked(world, file)?, file)));
    // A kind of the world's own is all in its file; a block's look is the block's.
    let kind = || {
        let (at, file) = entry()?;
        if file.blocks[at].block.is_some() { drawn()?.blocks.get(at) } else { Some(&file.blocks[at]) }
    };
    let cover = || entry().and_then(|(at, file)| file.blocks[at].cover);
    let state = ui(world);
    Some(match field {
        F::Data => Value::Asset(path_of(game, Some(&world.get::<BlockWorldSource>(entity)?.0.data))),
        F::BlockSize => Value::Number(file()?.block_size),
        F::Smooth => Value::Toggle(file()?.smooth),
        F::GroundSize => Value::Vector(file()?.ground.size.map(|v| v as f32).to_vec()),
        F::GroundHeight => Value::Number(file()?.ground.height as f32),
        F::Shadows => Value::Toggle(world.get::<BlockWorld>(entity)?.cast_shadows),
        F::Brush => {
            let at = BlockBrush::ALL.iter().position(|&b| b == state.brush).unwrap_or_default();
            Value::Choice(at, BlockBrush::ALL.iter().map(|b| b.name().to_string()).collect())
        }
        F::BrushSize => Value::Number(state.size as f32),
        F::Model => Value::Asset(
            state.model.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |e| e.path.clone()),
        ),
        F::Block => Value::Asset(path_of(game, kind()?.block.as_deref())),
        F::NewBlock => Value::Asset(String::new()),
        F::Name => Value::Text(kind()?.name.clone()),
        F::Water => Value::Toggle(kind()?.water),
        F::Color => Value::Color(kind()?.color),
        F::Material => Value::Asset(path_of(game, kind()?.material.as_deref())),
        F::Top => Value::Asset(path_of(game, kind()?.top.as_deref())),
        F::Bottom => Value::Asset(path_of(game, kind()?.bottom.as_deref())),
        F::Ground => {
            let names = ["Flat", "Generated", "Planet"].map(String::from).to_vec();
            let generate = file()?.generate.as_ref();
            Value::Choice(generate.map_or(0, |generate| if generate.planet.is_some() { 2 } else { 1 }), names)
        }
        F::Radius => Value::Number(file()?.generate.as_ref()?.planet?.radius),
        F::Generator => Value::Asset(path_of(game, file()?.generate.as_ref()?.world.as_deref())),
        F::Offset => Value::Vector(file()?.generate.as_ref()?.position.to_vec()),
        F::Lowest => Value::Number(file()?.generate.as_ref()?.bottom as f32),
        F::Overhangs => Value::Number(file()?.generate.as_ref()?.overhangs),
        F::Caves => Value::Number(file()?.generate.as_ref()?.caves),
        F::Endless => Value::Toggle(file()?.generate.as_ref()?.endless.is_some()),
        F::ViewDistance => Value::Number(file()?.generate.as_ref()?.endless?.distance),
        F::FarDistance => Value::Number(file()?.generate.as_ref()?.endless?.far),
        F::TopBlock => block_choice(drawn()?, file()?.ground.top),
        F::UnderBlock => block_choice(drawn()?, file()?.ground.under),
        F::UnderDepth => Value::Number(file()?.ground.under_depth as f32),
        F::BaseBlock => block_choice(drawn()?, file()?.ground.bottom),
        F::SeaBlock => block_choice(drawn()?, file()?.generate.as_ref()?.sea),
        F::Cover => Value::Toggle(entry().map(|_| cover().is_some())?),
        F::CoverBiome => {
            // Any, each biome, and its own ranges where they are none's.
            let cover = cover()?;
            let mut names = vec!["Any".to_string()];
            names.extend(Biome::ALL.map(|biome| biome.name().to_string()));
            let ranges = (cover.temperature, cover.humidity);
            let at = match Biome::ALL.iter().position(|biome| biome.ranges() == ranges) {
                _ if ranges == (None, None) => 0,
                Some(at) => at + 1,
                None => {
                    names.push("Custom".to_string());
                    names.len() - 1
                }
            };
            Value::Choice(at, names)
        }
        F::TemperatureMask => Value::Toggle(cover()?.temperature.is_some()),
        F::Temperatures => Value::Vector(cover()?.temperature?.to_vec()),
        F::HumidityMask => Value::Toggle(cover()?.humidity.is_some()),
        F::Humidities => Value::Vector(cover()?.humidity?.to_vec()),
        F::HeightMask => Value::Toggle(cover()?.height.is_some()),
        F::Heights => Value::Vector(cover()?.height?.to_vec()),
        F::CoverDepth => Value::Number(cover()?.depth as f32),
    })
}

fn pair(value: &Value) -> Option<[f32; 2]> {
    match value {
        Value::Vector(v) if v.len() >= 2 => Some([v[0].min(v[1]), v[0].max(v[1])]),
        _ => None,
    }
}

/// Sets one of a cover's values.
fn set_cover(cover: &mut BlockCover, field: BlockField, value: &Value) {
    match (field, value) {
        (F::CoverBiome, Value::Choice(at, _)) => match at.checked_sub(1).and_then(|at| Biome::ALL.get(at)) {
            Some(biome) => (cover.temperature, cover.humidity) = biome.ranges(),
            None if *at == 0 => (cover.temperature, cover.humidity) = (None, None),
            None => {}
        },
        (F::TemperatureMask, Value::Toggle(on)) => {
            cover.temperature = on.then_some(cover.temperature.unwrap_or([-10.0, 0.0]))
        }
        (F::Temperatures, v) => cover.temperature = pair(v).or(cover.temperature),
        (F::HumidityMask, Value::Toggle(on)) => cover.humidity = on.then_some(cover.humidity.unwrap_or([0.0, 0.5])),
        (F::Humidities, v) => cover.humidity = pair(v).map(|r| r.map(|h| h.clamp(0.0, 1.0))).or(cover.humidity),
        (F::HeightMask, Value::Toggle(on)) => cover.height = on.then_some(cover.height.unwrap_or([0.0, 100.0])),
        (F::Heights, v) => cover.height = pair(v).or(cover.height),
        (F::CoverDepth, v) => cover.depth = number(v).unwrap_or(1.0).round().clamp(1.0, 64.0) as u32,
        _ => {}
    }
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Whole(n) => Some(*n as f32),
        _ => None,
    }
}

/// Sets a value: the brush's on the editor, whether it casts shadows on
/// the world, the rest on its asset's file, its blocks made again from it.
pub fn set(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, field: BlockField, value: Value) -> bool {
    let world = &mut game.engine.world;
    match (field, &value) {
        (F::Shadows, Value::Toggle(on)) => {
            let Some(blocks) = world.get_mut::<BlockWorld>(entity) else { return false };
            blocks.cast_shadows = *on;
            return true;
        }
        (F::Brush, Value::Choice(at, _)) => {
            let brush = BlockBrush::ALL[(*at).min(BlockBrush::ALL.len() - 1)];
            return std::mem::replace(&mut ui_mut(world).brush, brush) != brush;
        }
        (F::BrushSize, v) => {
            let size = number(v).unwrap_or(1.0).round().clamp(1.0, 16.0) as u32;
            return std::mem::replace(&mut ui_mut(world).size, size) != size;
        }
        _ => {}
    }
    let Some(mut file) = file_of(world, entity) else { return false };
    // No entry at all where none is picked: what follows asks for one within the list.
    let picked = picked(world, &file).unwrap_or(usize::MAX);
    // A block's colour and whether it is water are the block's own, written to its file.
    if let Some(block) = file.blocks.get(picked).and_then(|kind| kind.block.clone()) {
        match (field, &value) {
            (F::Color, Value::Color(color)) => return change_block(game, &block, |block| block.color = *color),
            (F::Water, Value::Toggle(on)) => {
                let liquid = if *on { Liquid::Water } else { Liquid::None };
                return change_block(game, &block, |block| block.liquid = liquid);
            }
            _ => {}
        }
    }
    match (field, &value) {
        // Its ground stays as large in metres, counted again in blocks of the new size.
        (F::BlockSize, v) => drop(file.set_block_size(number(v).unwrap_or(file.block_size))),
        (F::Smooth, Value::Toggle(on)) => file.smooth = *on,
        (F::GroundSize, Value::Vector(v)) if v.len() >= 2 => {
            file.ground.size = [v[0], v[1]].map(|n| n.round().clamp(0.0, 4096.0) as u32);
        }
        (F::GroundHeight, v) => {
            file.ground.height = number(v).unwrap_or(file.ground.height as f32).round().clamp(0.0, 1024.0) as u32;
        }
        (F::Name, Value::Text(name)) if picked < file.blocks.len() => file.blocks[picked].name = name.clone(),
        (F::Color, Value::Color(color)) if picked < file.blocks.len() => file.blocks[picked].color = *color,
        (F::Water, Value::Toggle(on)) if picked < file.blocks.len() => file.blocks[picked].water = *on,
        (F::Ground, Value::Choice(at, _)) => {
            let generate = file.generate.clone().unwrap_or_default();
            file.generate = match at {
                1 => Some(BlockGenerate { planet: None, ..generate }),
                // A planet is made all at once, never without end.
                2 => {
                    Some(BlockGenerate { planet: Some(generate.planet.unwrap_or_default()), endless: None, ..generate })
                }
                _ => None,
            };
        }
        (F::Radius, v) => {
            let Some(planet) = file.generate.as_mut().and_then(|g| g.planet.as_mut()) else { return false };
            planet.radius = number(v).unwrap_or(planet.radius).clamp(8.0, 20_000.0);
        }
        (F::Offset, v) => {
            let Some(generate) = &mut file.generate else { return false };
            if let Value::Vector(v) = v
                && v.len() >= 2
            {
                generate.position = [v[0], v[1]];
            }
        }
        (F::Lowest, v) => {
            let Some(generate) = &mut file.generate else { return false };
            generate.bottom = number(v).unwrap_or(generate.bottom as f32).round().clamp(-1024.0, 1024.0) as i32;
        }
        (F::Overhangs, v) => {
            let Some(generate) = &mut file.generate else { return false };
            generate.overhangs = number(v).unwrap_or(generate.overhangs).clamp(0.0, 100.0);
        }
        (F::Caves, v) => {
            let Some(generate) = &mut file.generate else { return false };
            generate.caves = number(v).unwrap_or(generate.caves).clamp(0.0, 1.0);
        }
        (F::Endless, Value::Toggle(on)) => {
            let Some(generate) = &mut file.generate else { return false };
            generate.endless = on.then(|| generate.endless.unwrap_or_default());
        }
        (F::ViewDistance | F::FarDistance, v) => {
            let Some(endless) = file.generate.as_mut().and_then(|g| g.endless.as_mut()) else { return false };
            let distance = number(v).unwrap_or(0.0).clamp(16.0, 100_000.0);
            match field {
                F::ViewDistance => endless.distance = distance,
                _ => endless.far = distance,
            }
        }
        (F::TopBlock | F::UnderBlock | F::BaseBlock, Value::Choice(at, _)) => {
            let block = chosen_block(&file, *at);
            match field {
                F::TopBlock => file.ground.top = block,
                F::UnderBlock => file.ground.under = block,
                _ => file.ground.bottom = block,
            }
        }
        (F::SeaBlock, Value::Choice(at, _)) => {
            let sea = chosen_block(&file, *at);
            let Some(generate) = &mut file.generate else { return false };
            generate.sea = sea;
        }
        (F::UnderDepth, v) => {
            file.ground.under_depth =
                number(v).unwrap_or(file.ground.under_depth as f32).round().clamp(0.0, 256.0) as u32;
        }
        (F::Cover, Value::Toggle(on)) if picked < file.blocks.len() => {
            let kind = &mut file.blocks[picked];
            kind.cover = on.then(|| kind.cover.unwrap_or_default());
        }
        (F::CoverBiome | F::TemperatureMask | F::Temperatures | F::HumidityMask | F::Humidities, _)
        | (F::HeightMask | F::Heights | F::CoverDepth, _) => {
            let Some(cover) = file.blocks.get_mut(picked).and_then(|kind| kind.cover.as_mut()) else { return false };
            set_cover(cover, field, &value);
        }
        _ => return false,
    }
    // While a number is dragged by its label, its world is made again only once it is let go (see `hold`).
    if let Some(held) = game.engine.world.resource_mut::<HeldFiles>().filter(|held| held.holding) {
        return held.files.insert(entity, file.clone()).is_none_or(|was| was != file);
    }
    change_file(game, renderer, entity, file)
}

/// Puts `file` on the world's asset and makes its blocks again from it: its
/// ground anew where that changed, with the changes built on the old ground
/// kept on it, and its look made again where its faces are drawn another
/// way.
fn change_file(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, file: BlockFile) -> bool {
    change(game, renderer, entity, file, None)
}

/// As [`change_file`], for a file one of whose blocks was taken out: every
/// block, the ground and every change kept renumbered by `renumber`, which
/// makes its blocks air.
fn renumber_file(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    file: BlockFile,
    renumber: &dyn Fn(BlockId) -> BlockId,
) -> bool {
    change(game, renderer, entity, file, Some(renumber))
}

fn change(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    file: BlockFile,
    renumber: Option<&dyn Fn(BlockId) -> BlockId>,
) -> bool {
    // One being made again from an earlier file is let go: it is made from this one now.
    let making = game.engine.world.resource_mut::<Making>();
    let was_making = making.is_some_and(|making| making.0.remove(&entity).is_some());
    let world = &game.engine.world;
    let Some(asset) = world.get::<BlockWorldAsset>(entity).cloned() else { return false };
    let (old, base) = (asset.file.clone(), asset.base.clone());
    let Some(blocks) = world.get::<BlockWorld>(entity).cloned() else { return false };
    if old == file {
        return was_making;
    }
    // Its file as it is drawn, each block it lists with that block's look, kept on its asset with the file.
    let drawn = Arc::new(voxear_engine::blocks::resolved(&mut game.content, &file, &asset.grown));
    let derived = game.content.blocks_version();
    let renumbered_edits = |edits: BlockEdits| match renumber {
        Some(change) => edits.remapped(change),
        None => edits,
    };
    // Blocks of another size: what was built stays where it was in metres, read again in the new blocks.
    let resized = old.block_size != file.block_size;
    let water = |block: BlockId| voxear_terrain::blocks::is_water(&blocks.faces, block);
    let resize = Resize { from: old.block_size, to: file.block_size, smooth: file.smooth, water: &water };
    // A world without end is made round where it is seen as it goes (see `voxear_engine::blocks::stream`), from its
    // file: only that changes here. One just made endless, or renumbered, starts again with nothing made and every
    // change kept.
    let endless = |file: &BlockFile| file.generate.as_ref().is_some_and(|g| g.endless.is_some());
    if endless(&file) {
        let world = &mut game.engine.world;
        if !endless(&old) || renumber.is_some() || resized {
            let changes = match endless(&old) {
                true => voxear_terrain::blocks::stream::all_changes(&base, &blocks.blocks, &asset.pending),
                false => BlockEdits::between(&base, &blocks.blocks),
            };
            let changes = if resized { changes.resized(&resize) } else { changes };
            let (_, faces) = drawn.faces();
            let mut empty = BlockWorld::made(BlockMap::default(), &faces, file.block_size, file.smooth);
            empty.cast_shadows = blocks.cast_shadows;
            world.insert(entity, empty);
            let pending = Arc::new(renumbered_edits(changes));
            let kept = BlockWorldAsset { file, base: Arc::default(), pending, drawn: drawn.clone(), derived, ..asset };
            world.insert(entity, kept);
        } else if let Some(kept) = world.get_mut::<BlockWorldAsset>(entity) {
            (kept.file, kept.drawn, kept.derived) = (file, drawn.clone(), derived);
        }
        restyle(game, renderer, entity, &drawn);
        return true;
    }
    // One no longer endless keeps every change it had, near or away.
    let carried =
        endless(&old).then(|| voxear_terrain::blocks::stream::all_changes(&base, &blocks.blocks, &asset.pending));
    let (_, faces) = drawn.faces();
    // Blocks added at the end leave the ground as it was; one taken out makes its blocks air.
    let grounded = old.ground != file.ground
        || old.generate != file.generate
        || renumber.is_some()
        || old.block_size != file.block_size
        || old.smooth != file.smooth
        || old.blocks.iter().map(|k| k.cover).ne(file.blocks.iter().map(|k| k.cover));
    let (base, remade) = if grounded {
        let generator = voxear_engine::blocks::generator(&mut game.content, &file);
        // A large world is made on other threads, and shows what it was until then.
        if renumber.is_none() && made_slowly(&file) {
            let world = &mut game.engine.world;
            start_making(world, entity, (file, drawn, derived), (old, base, blocks), carried, generator);
            return true;
        }
        let ground = Arc::new(file.make_ground(generator.as_deref()));
        let made = made_on((&file, &drawn), &ground, (&old, &base, &blocks), carried, renumber);
        (ground, made)
    } else if *faces != *blocks.faces {
        (base, blocks.restyled(&faces, file.smooth))
    } else {
        (base, blocks)
    };
    let world = &mut game.engine.world;
    world.insert(entity, remade);
    if let Some(asset) = world.get_mut::<BlockWorldAsset>(entity) {
        (asset.file, asset.base, asset.drawn, asset.derived) = (file, base, drawn.clone(), derived);
    }
    restyle(game, renderer, entity, &drawn);
    true
}

/// The world of blocks `file` makes on `ground`, drawn as `drawn`, with
/// what was built on the world `blocks`, which `old` made on `base`, kept on
/// it: where it was in metres where blocks changed size, renumbered by
/// `renumber` where a block was taken out; or, in their place, `carried`,
/// every change a world without end had.
fn made_on(
    (file, drawn): (&BlockFile, &Resolved),
    ground: &Arc<BlockMap>,
    (old, base, blocks): (&BlockFile, &BlockMap, &BlockWorld),
    carried: Option<BlockEdits>,
    renumber: Option<&dyn Fn(BlockId) -> BlockId>,
) -> BlockWorld {
    let renumbered = |map: &BlockMap| renumber.map_or_else(|| map.clone(), |change| map.remapped(change));
    let resized = old.block_size != file.block_size;
    let water = |block: BlockId| voxear_terrain::blocks::is_water(&blocks.faces, block);
    let resize = Resize { from: old.block_size, to: file.block_size, smooth: file.smooth, water: &water };
    // The new ground names its blocks by their new numbers already; the old ground and blocks are renumbered.
    let changes = match (carried, renumber) {
        (Some(carried), _) if resized => carried.resized(&resize),
        (Some(carried), Some(change)) => carried.remapped(change),
        (Some(carried), None) => carried,
        (None, _) if resized => {
            let mut built = (**ground).clone();
            let cells = edits::changed(base, &blocks.blocks);
            for (at, block, fill) in edits::resized(&cells, Some(&blocks.blocks), &resize) {
                built.set_cell(at, block, fill);
            }
            BlockEdits::between(ground, &built)
        }
        (None, _) => BlockEdits::between(&renumbered(base), &renumbered(&blocks.blocks)),
    };
    let mut made = voxear_engine::blocks::make(drawn, ground, Some(&changes));
    made.cast_shadows = blocks.cast_shadows;
    made
}

/// Makes every world of blocks whose ground the world generator `guid`
/// makes again from it, the changes built on its old ground kept, as after
/// the generator changed.
pub fn regenerate(game: &mut Game, guid: voxear_assets::AssetGuid) {
    let world = &game.engine.world;
    let using: Vec<Entity> = world
        .query_ref::<(Entity, &BlockWorldAsset)>()
        .into_iter()
        .filter(|(_, asset)| {
            // One without end is made again from its generator by its streamer, which sees the generator change.
            if asset.file.generate.as_ref().is_some_and(|g| g.endless.is_some()) {
                return false;
            }
            let reference = asset.file.generate.as_ref().and_then(|g| g.world.as_deref());
            reference.and_then(|r| game.content.parse(r).ok()).and_then(|parsed| parsed.guid()) == Some(guid)
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in using {
        let world = &game.engine.world;
        let (Some(asset), Some(blocks)) =
            (world.get::<BlockWorldAsset>(entity).cloned(), world.get::<BlockWorld>(entity).cloned())
        else {
            continue;
        };
        let ground = Arc::new(voxear_engine::blocks::ground(&mut game.content, &asset.file));
        let changes = BlockEdits::between(&asset.base, &blocks.blocks);
        let mut made = voxear_engine::blocks::make(&asset.drawn, &ground, Some(&changes));
        made.cast_shadows = blocks.cast_shadows;
        game.engine.world.insert(entity, made);
        game.engine.world.insert(entity, BlockWorldAsset { base: ground, ..asset });
    }
}

/// Gives the world's look its file's surfaces again, each material loaded,
/// where there is a renderer to load them and a look to change.
fn restyle(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, file: &voxear_engine::blocks::Resolved) {
    if !game.engine.world.has::<BlockLook>(entity) {
        return;
    }
    let Some(renderer) = renderer else {
        // Without a renderer at hand, the looks it has, each as large as a block.
        if let Some(look) = game.engine.world.get::<BlockLook>(entity).cloned() {
            let size = file.block_size;
            let surfaces: Vec<_> =
                look.surfaces.iter().map(|surface| voxear_render::SurfaceLook { size, ..*surface }).collect();
            game.engine.world.insert(entity, BlockLook { surfaces: surfaces.into(), ..look });
        }
        return;
    };
    match voxear_engine::blocks::look(&mut game.content, renderer, file) {
        Ok(look) => drop(game.engine.world.insert(entity, look)),
        Err(error) => tracing::error!("a world of blocks' look: {error:#}"),
    }
}

/// What a picker offers: the project's worlds of blocks, its blocks, or its
/// materials.
pub fn choices(game: &Game, field: BlockField) -> Vec<(String, Pick)> {
    match field {
        F::Data => fields::assets_of(game, Some(AssetKind::BlockWorld)),
        // An entry is always of some block.
        F::Block | F::NewBlock => {
            let mut found = fields::assets_of(game, Some(AssetKind::Block));
            found.retain(|(_, pick)| *pick != Pick::Asset(None));
            found
        }
        F::Material | F::Top | F::Bottom => fields::assets_of(game, Some(AssetKind::Material)),
        F::Generator => fields::assets_of(game, Some(AssetKind::WorldGen)),
        F::Model => fields::assets_of(game, Some(AssetKind::VoxModel)),
        _ => Vec::new(),
    }
}

/// Takes what was picked: another asset for its blocks, or a material for
/// the picked kind of block.
pub fn pick(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    field: BlockField,
    choice: &Pick,
) -> bool {
    let Pick::Asset(path) = choice else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference),
            Err(_) => return false,
        },
        None => None,
    };
    if field == F::Model {
        let model = reference.and_then(|r| r.guid());
        return std::mem::replace(&mut ui_mut(&mut game.engine.world).model, model) != model;
    }
    if field != F::Data {
        let Some(mut file) = file_of(&game.engine.world, entity) else {
            return false;
        };
        let written = reference.map(|r| r.to_string());
        if field == F::Generator {
            let Some(generate) = &mut file.generate else { return false };
            generate.world = written;
            return change_file(game, renderer, entity, file);
        }
        if field == F::NewBlock {
            return written.is_some_and(|block| list_block(game, renderer, entity, &block));
        }
        let Some(picked) = picked(&game.engine.world, &file) else { return false };
        let kind = &mut file.blocks[picked];
        if field == F::Block {
            // Another block in its place: every block of its number is that one now.
            let Some(block) = written else { return false };
            *kind = BlockKind { cover: kind.cover, ..BlockKind::of(&block) };
            return change_file(game, renderer, entity, file);
        }
        // A block's materials are the block's own, written to its file.
        if let Some(block) = kind.block.clone() {
            return change_block(game, &block, |block| match field {
                F::Material => block.material = written,
                F::Top => block.top = written,
                F::Bottom => block.bottom = written,
                _ => {}
            });
        }
        match field {
            F::Material => kind.material = written,
            F::Top => kind.top = written,
            F::Bottom => kind.bottom = written,
            _ => return false,
        }
        return change_file(game, renderer, entity, file);
    }
    let Some(source) = game.engine.world.get::<BlockWorldSource>(entity).cloned() else { return false };
    let data = BlockWorldData { data: reference.map_or_else(String::new, |r| r.to_string()), ..source.0 };
    match voxear_engine::prefab::block_world(&mut game.content, renderer, &data) {
        Ok(parts) => {
            let first = parts.asset.as_ref().and_then(|asset| asset.file.blocks.iter().position(BlockKind::is_live));
            put(&mut game.engine.world, entity, parts);
            ui_mut(&mut game.engine.world).block = first.unwrap_or(0);
            true
        }
        Err(error) => {
            tracing::error!("{error:#}");
            false
        }
    }
}

/// Builds `model` in the world, its lowest blocks at `at` and its middle
/// over it: a kind of the world's own for each of its colours the world
/// has none of, as long as it can draw more.
pub(crate) fn place_model(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    model: &VoxFile,
    at: [i32; 3],
) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    let room = MAX_LOOKS.saturating_sub(looks(game, entity));
    let changes = voxear_terrain::blocks::vox::place(&mut file, model, at, room);
    change_file(game, renderer, entity, file);
    let Some(blocks) = game.engine.world.get_mut::<BlockWorld>(entity) else { return false };
    blocks.set_blocks(changes);
    true
}

/// How many ways of drawing a face the world has now, of the `MAX_LOOKS`
/// it can.
fn looks(game: &Game, entity: Entity) -> usize {
    game.engine.world.get::<BlockWorldAsset>(entity).map_or(0, |asset| asset.drawn.looks())
}

/// Whether the world can list one more block: one drawn as nothing it
/// lists is needs a way of drawing a face or three, and a world has only
/// so many.
fn has_room(game: &Game, entity: Entity, file: &BlockFile) -> bool {
    if file.blocks.len() >= MAX_BLOCKS || looks(game, entity) + 3 > MAX_LOOKS {
        tracing::warn!("A world of blocks draws its faces {MAX_LOOKS} ways at most: this one has no room for more");
        return false;
    }
    true
}

/// Picks the block at `at` of the list to build with and show.
pub fn pick_block(world: &mut World, at: usize) {
    ui_mut(world).block = at;
}

/// Shows tool `tool`.
pub fn show_tool(world: &mut World, tool: BlockTool) {
    ui_mut(world).tool = tool;
}

/// Changes the block `reference` names and writes its file, so every world
/// listing it takes it up (see `voxear_engine::blocks::refresh`). Whether
/// anything changed.
fn change_block(game: &mut Game, reference: &str, change: impl FnOnce(&mut BlockDef)) -> bool {
    let Some((guid, block)) = game.content.block(reference) else { return false };
    let mut changed = (*block).clone();
    change(&mut changed);
    changed != *block && write_block(game, guid, &changed)
}

/// Writes `block` as the `.block` at `guid`, read again by whatever lists
/// it.
pub(crate) fn write_block(game: &mut Game, guid: AssetGuid, block: &BlockDef) -> bool {
    let Some(path) = game.content.db().get(guid).map(|entry| entry.path.clone()) else { return false };
    let file = game.content.db().file(&path);
    if let Err(error) = std::fs::write(&file, crate::project_files::block_text(block)) {
        tracing::error!("Could not save {path}: {error}");
        return false;
    }
    game.content.forget_block(guid);
    true
}

/// Whether a data asset of the project places the block at `guid`: an item
/// it is carried as.
pub(crate) fn placed_by(game: &Game, guid: AssetGuid) -> bool {
    let db = game.content.db();
    db.entries().filter(|entry| entry.kind == AssetKind::Data).any(|entry| {
        let read = std::fs::read_to_string(db.file(&entry.path)).ok();
        let Some(table) = read.and_then(|text| toml::from_str::<toml::Table>(&text).ok()) else { return false };
        let places = table.get("fields").and_then(|fields| fields.get("places")).and_then(toml::Value::as_str);
        places.and_then(|block| game.content.parse(block).ok()).and_then(|block| block.guid()) == Some(guid)
    })
}

/// Lists the block `reference` names at the end of the world's list, and
/// picks it; one it lists already is only picked.
pub fn list_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, reference: &str) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    let guid_of = |game: &Game, reference: &str| game.content.parse(reference).ok().and_then(|parsed| parsed.guid());
    let Some(guid) = guid_of(game, reference) else { return false };
    let known = |kind: &BlockKind| kind.block.as_deref().and_then(|block| guid_of(game, block)) == Some(guid);
    if let Some(at) = file.blocks.iter().position(|kind| kind.is_live() && known(kind)) {
        return std::mem::replace(&mut ui_mut(&mut game.engine.world).block, at) != at;
    }
    if !has_room(game, entity, &file) {
        return false;
    }
    file.blocks.push(BlockKind::of(&guid.to_string()));
    ui_mut(&mut game.engine.world).block = file.blocks.len() - 1;
    change_file(game, renderer, entity, file)
}

/// A colour of its own for the `number`th block made: round the wheel.
fn fresh_color(number: usize) -> [f32; 3] {
    let hue = number.saturating_sub(1) as f32 * 0.38;
    [0.0, 2.0, 4.0].map(|offset: f32| 0.45 + 0.3 * ((hue + offset / 3.0) * std::f32::consts::TAU).cos())
}

/// Makes a new `.block` of a colour of its own in the project's Blocks
/// folder, lists it at the end of the world's list and picks it.
pub fn new_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    let Some(file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    if !has_room(game, entity, &file) {
        return false;
    }
    let block = BlockDef::colored(fresh_color(file.blocks.len() + 1));
    let root = game.project.root.clone();
    let made = crate::project_files::create_block(&root, BLOCKS_FOLDER, "Block", &block)
        .and_then(|path| game.content.rescan().map(|_| path));
    match made {
        Ok(path) => list_block(game, renderer, entity, &path),
        Err(error) => {
            tracing::error!("{error:#}");
            false
        }
    }
}

/// Adds a kind of the world's own, of a colour of its own, at the end of
/// the list, and picks it.
pub fn add_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    if !has_room(game, entity, &file) {
        return false;
    }
    // Numbered from its place in the list, past any name taken, its colour from its number round the wheel.
    let number = (file.blocks.len() + 1..)
        .find(|n| !file.blocks.iter().any(|kind| kind.name == format!("Color {n}")))
        .expect("names enough");
    file.blocks.push(BlockKind::colored(&format!("Color {number}"), fresh_color(number)));
    ui_mut(&mut game.engine.world).block = file.blocks.len() - 1;
    change_file(game, renderer, entity, file)
}

/// Takes the block at `at` out of the list: its blocks become air, and the
/// ground's layers of it too. Its place stays, empty, so every other block
/// keeps its number.
pub fn remove_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, at: usize) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    if !file.blocks.get(at).is_some_and(BlockKind::is_live) {
        return false;
    }
    file.blocks[at] = BlockKind::gap();
    file.forget_gaps();
    let gone = (at + 1) as BlockId;
    let renumber = move |block: BlockId| if block == gone { AIR } else { block };
    // The block listed after it is picked, or else the one before.
    let live = |at: &usize| file.blocks[*at].is_live();
    let next = (at..file.blocks.len()).find(live).or_else(|| (0..at).rev().find(live));
    ui_mut(&mut game.engine.world).block = next.unwrap_or(0);
    renumber_file(game, renderer, entity, file, &renumber)
}

/// Whether a kind of the world's own is one a model of small blocks brought
/// its colour as, which stays the world's: a colour alone, named as those
/// are.
fn is_model_color(kind: &BlockKind) -> bool {
    let plain = !kind.water && kind.material.is_none() && kind.top.is_none() && kind.bottom.is_none();
    plain && kind.name.strip_prefix("Color ").is_some_and(|number| number.parse::<u32>().is_ok())
}

/// How many kinds of the world's own Make Block Assets would make blocks
/// of: all but the colours models brought.
pub fn kinds_to_make(file: &BlockFile) -> usize {
    file.blocks.iter().filter(|kind| kind.is_inline() && !is_model_color(kind)).count()
}

/// Makes a `.block` in the project's Blocks folder of each kind the world
/// keeps itself, but for the colours models brought, and lists the block in
/// the kind's place: one there already of its name and look is the block.
/// Every block keeps its number, so nothing built changes.
pub fn make_block_assets(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    blocks_for_kinds(game, renderer, entity, true)
}

/// As [`make_block_assets`]; or, not `as_drawn`, with the block there
/// already of a kind's name taken as it is, whatever its look: how a new
/// world comes by the project's own grass, dirt and stone.
fn blocks_for_kinds(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, as_drawn: bool) -> bool {
    let Some(mut file) = file_of(&game.engine.world, entity) else {
        return false;
    };
    let root = game.project.root.clone();
    let mut made: Vec<(usize, String)> = Vec::new();
    for (at, kind) in file.blocks.iter().enumerate() {
        if !kind.is_inline() || is_model_color(kind) {
            continue;
        }
        let block = BlockDef::from_kind(kind);
        let found = match as_drawn {
            true => crate::project_files::block_for(&root, BLOCKS_FOLDER, &kind.name, &block),
            false => crate::project_files::block_named(&root, BLOCKS_FOLDER, &kind.name, &block),
        };
        match found {
            Ok(path) => made.push((at, path)),
            Err(error) => tracing::error!("{error:#}"),
        }
    }
    if made.is_empty() {
        return false;
    }
    if let Err(error) = game.content.rescan() {
        tracing::error!("{error:#}");
        return false;
    }
    for (at, path) in made {
        let Some(guid) = game.content.parse(&path).ok().and_then(|parsed| parsed.guid()) else { continue };
        // Read as it is now, where a block of that name was there already.
        game.content.forget_block(guid);
        file.blocks[at] = BlockKind { cover: file.blocks[at].cover, ..BlockKind::of(&guid.to_string()) };
    }
    change_file(game, renderer, entity, file)
}

fn put(world: &mut World, entity: Entity, parts: BlockWorldParts) {
    world.insert(entity, parts.blocks);
    match parts.look {
        Some(look) => world.insert(entity, look),
        None => world.remove::<BlockLook>(entity).is_some(),
    };
    match parts.asset {
        Some(asset) => world.insert(entity, asset),
        None => world.remove::<BlockWorldAsset>(entity).is_some(),
    };
    world.insert(entity, parts.source);
}

/// Add Component's Block World: flat ground of grass, dirt and stone on a
/// new asset of its own, named for its Obj, in the project's Blocks folder;
/// or, `smooth`, its Voxel Terrain, the same drawn smooth, in its Voxels
/// folder. Its grass, dirt and stone are the project's blocks of those
/// names, made where it has none.
pub fn add(game: &mut Game, mut renderer: Option<&mut Renderer>, entity: Entity, smooth: bool) -> Result<()> {
    let name = game.engine.world.get::<voxear_scene::Name>(entity).map_or("Blocks".into(), |n| n.0.clone());
    let root = game.project.root.clone();
    let folder = if smooth { SMOOTH_FOLDER } else { FOLDER };
    let path = crate::project_files::create_block_world(&root, folder, &name, smooth)?;
    game.content.rescan()?;
    let guid = game.content.parse(&path)?.guid().with_context(|| format!("{path} is not an asset"))?;
    let data = BlockWorldData { data: guid.to_string(), cast_shadows: true };
    let parts = voxear_engine::prefab::block_world(&mut game.content, renderer.as_deref_mut(), &data)?;
    put(&mut game.engine.world, entity, parts);
    blocks_for_kinds(game, renderer, entity, false);
    Ok(())
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<BlockWorld>(entity);
    world.remove::<BlockLook>(entity);
    world.remove::<BlockWorldAsset>(entity);
    world.remove::<BlockWorldSource>(entity);
}
