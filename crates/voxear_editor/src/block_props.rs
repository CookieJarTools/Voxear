//! A Block World in the Inspector: its tools, **Build** and **Settings**.
//! Under Build, its kinds of block (see `block_panel`), the brush the Scene
//! view builds with (see `block_tools`), and the picked kind's name,
//! colour and materials. Under Settings, the `.blockworld` asset its blocks
//! are, how large a block is, the flat ground its blocks start as, and
//! whether it casts the sun's shadows. A change to the asset's file makes
//! its blocks again at once, the changes built since kept on them, kept in
//! the world for saving and undo as a Terrain's are.

use anyhow::{Context, Result};
use std::sync::Arc;
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    blocks::BlockWorldAsset,
    prefab::BlockWorldParts,
    scene_file::{BlockWorldData, BlockWorldSource},
};
use voxear_render::{BlockLook, Renderer};

use voxear_terrain::{
    blocks::{
        BlockMap, BlockWorld,
        chunk::{AIR, BlockId},
        edits::BlockEdits,
        file::{BlockCover, BlockFile, BlockKind, MAX_BLOCKS},
    },
    surfaces::Biome,
};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Where Add Component puts a new world of blocks' asset.
const FOLDER: &str = "Blocks";

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
/// the face under it, takes away the block under it, or turns the blocks
/// under it into the picked one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlockBrush {
    #[default]
    Place,
    Break,
    Paint,
}

impl BlockBrush {
    pub const ALL: [BlockBrush; 3] = [BlockBrush::Place, BlockBrush::Break, BlockBrush::Paint];

    pub fn name(self) -> &'static str {
        match self {
            BlockBrush::Place => "Place",
            BlockBrush::Break => "Break",
            BlockBrush::Paint => "Paint",
        }
    }
}

/// What the editor is doing with worlds of blocks: the tool shown, the
/// kind of block picked, by its place in the list, and the brush and how
/// many blocks across it reaches. Kept in the edit world, not saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockUi {
    pub tool: BlockTool,
    pub block: usize,
    pub brush: BlockBrush,
    pub size: u32,
}

impl Default for BlockUi {
    fn default() -> Self {
        Self { tool: BlockTool::Build, block: 0, brush: BlockBrush::Place, size: 1 }
    }
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
    /// How many blocks its ground reaches along x and z, and how many up.
    GroundSize,
    GroundHeight,
    Shadows,
    /// Whether its ground is flat or made by a world generator, and then
    /// which generator, from where in its world, down to which block, how
    /// far its surface is pushed in and out, and how much is caves.
    Ground,
    Generator,
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
    /// how deep they go, and the rest.
    TopBlock,
    UnderBlock,
    UnderDepth,
    BaseBlock,
    /// The brush: what it does and how many blocks across.
    Brush,
    BrushSize,
    /// The picked kind of block's name, colour and materials.
    Name,
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
    let file = world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file);
    let fields = match (state.tool, file) {
        (BlockTool::Build, Some(file)) => {
            let mut fields = vec![F::Brush, F::BrushSize];
            if let Some(kind) = file.blocks.get(state.block) {
                fields.extend([F::Name, F::Color, F::Material, F::Top, F::Bottom]);
                // Covers lay blocks on generated ground.
                if file.generate.is_some() {
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
            let mut fields = vec![F::Data, F::BlockSize, F::Ground];
            match file.generate {
                Some(ref generate) => {
                    fields.extend([F::Generator, F::Offset, F::Endless]);
                    match generate.endless {
                        Some(_) => fields.extend([F::ViewDistance, F::FarDistance]),
                        None => fields.push(F::GroundSize),
                    }
                    fields.extend([F::Lowest, F::Overhangs, F::Caves]);
                }
                None => fields.extend([F::GroundSize, F::GroundHeight]),
            }
            fields.extend([F::TopBlock, F::UnderBlock, F::UnderDepth, F::BaseBlock, F::Shadows]);
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
        F::GroundSize => "Ground size",
        F::GroundHeight => "Ground height",
        F::Shadows => "Cast shadows",
        F::Brush => "Brush",
        F::BrushSize => "Size",
        F::Name => "Name",
        F::Color => "Color",
        F::Material => "Material",
        F::Top => "Top",
        F::Bottom => "Bottom",
        F::Ground => "Ground",
        F::Generator => "Generator",
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
        F::BlockSize | F::Offset | F::Overhangs | F::Heights | F::ViewDistance | F::FarDistance => Some("m"),
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
    matches!(field, F::Data | F::Material | F::Top | F::Bottom | F::Generator)
}

/// A choice of the world's kinds of block, air first, with `block` chosen.
fn block_choice(file: &BlockFile, block: BlockId) -> Value {
    let mut names = vec!["Air".to_string()];
    names.extend(file.blocks.iter().map(|kind| kind.name.clone()));
    Value::Choice(usize::from(block).min(names.len() - 1), names)
}

/// An asset's path, by the reference a file holds, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

pub fn get(game: &Game, entity: Entity, field: BlockField) -> Option<Value> {
    let world = &game.engine.world;
    let file = || world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file);
    let kind = || file().and_then(|file| file.blocks.get(ui(world).block));
    let state = ui(world);
    Some(match field {
        F::Data => Value::Asset(path_of(game, Some(&world.get::<BlockWorldSource>(entity)?.0.data))),
        F::BlockSize => Value::Number(file()?.block_size),
        F::GroundSize => Value::Vector(file()?.ground.size.map(|v| v as f32).to_vec()),
        F::GroundHeight => Value::Number(file()?.ground.height as f32),
        F::Shadows => Value::Toggle(world.get::<BlockWorld>(entity)?.cast_shadows),
        F::Brush => {
            let at = BlockBrush::ALL.iter().position(|&b| b == state.brush).unwrap_or_default();
            Value::Choice(at, BlockBrush::ALL.iter().map(|b| b.name().to_string()).collect())
        }
        F::BrushSize => Value::Number(state.size as f32),
        F::Name => Value::Text(kind()?.name.clone()),
        F::Color => Value::Color(kind()?.color),
        F::Material => Value::Asset(path_of(game, kind()?.material.as_deref())),
        F::Top => Value::Asset(path_of(game, kind()?.top.as_deref())),
        F::Bottom => Value::Asset(path_of(game, kind()?.bottom.as_deref())),
        F::Ground => {
            let names = vec!["Flat".to_string(), "Generated".to_string()];
            Value::Choice(usize::from(file()?.generate.is_some()), names)
        }
        F::Generator => Value::Asset(path_of(game, file()?.generate.as_ref()?.world.as_deref())),
        F::Offset => Value::Vector(file()?.generate.as_ref()?.position.to_vec()),
        F::Lowest => Value::Number(file()?.generate.as_ref()?.bottom as f32),
        F::Overhangs => Value::Number(file()?.generate.as_ref()?.overhangs),
        F::Caves => Value::Number(file()?.generate.as_ref()?.caves),
        F::Endless => Value::Toggle(file()?.generate.as_ref()?.endless.is_some()),
        F::ViewDistance => Value::Number(file()?.generate.as_ref()?.endless?.distance),
        F::FarDistance => Value::Number(file()?.generate.as_ref()?.endless?.far),
        F::TopBlock => block_choice(file()?, file()?.ground.top),
        F::UnderBlock => block_choice(file()?, file()?.ground.under),
        F::UnderDepth => Value::Number(file()?.ground.under_depth as f32),
        F::BaseBlock => block_choice(file()?, file()?.ground.bottom),
        F::Cover => Value::Toggle(kind()?.cover.is_some()),
        F::CoverBiome => {
            // Any, each biome, and its own ranges where they are none's.
            let cover = kind()?.cover?;
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
        F::TemperatureMask => Value::Toggle(kind()?.cover?.temperature.is_some()),
        F::Temperatures => Value::Vector(kind()?.cover?.temperature?.to_vec()),
        F::HumidityMask => Value::Toggle(kind()?.cover?.humidity.is_some()),
        F::Humidities => Value::Vector(kind()?.cover?.humidity?.to_vec()),
        F::HeightMask => Value::Toggle(kind()?.cover?.height.is_some()),
        F::Heights => Value::Vector(kind()?.cover?.height?.to_vec()),
        F::CoverDepth => Value::Number(kind()?.cover?.depth as f32),
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
    let Some(mut file) = world.get::<BlockWorldAsset>(entity).map(|asset| asset.file.clone()) else { return false };
    let picked = ui(world).block;
    match (field, &value) {
        (F::BlockSize, v) => file.block_size = number(v).unwrap_or(file.block_size).clamp(0.01, 100.0),
        (F::GroundSize, Value::Vector(v)) if v.len() >= 2 => {
            file.ground.size = [v[0], v[1]].map(|n| n.round().clamp(0.0, 4096.0) as u32);
        }
        (F::GroundHeight, v) => {
            file.ground.height = number(v).unwrap_or(file.ground.height as f32).round().clamp(0.0, 1024.0) as u32;
        }
        (F::Name, Value::Text(name)) if picked < file.blocks.len() => file.blocks[picked].name = name.clone(),
        (F::Color, Value::Color(color)) if picked < file.blocks.len() => file.blocks[picked].color = *color,
        (F::Ground, Value::Choice(at, _)) => {
            file.generate = (*at == 1).then(|| file.generate.clone().unwrap_or_default());
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
            let block = (*at).min(file.blocks.len()) as BlockId;
            match field {
                F::TopBlock => file.ground.top = block,
                F::UnderBlock => file.ground.under = block,
                _ => file.ground.bottom = block,
            }
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
    change_file(game, renderer, entity, file, |blocks| blocks)
}

/// Puts `file` on the world's asset and makes its blocks again from it: its
/// ground anew where that changed, with the changes built on the old ground
/// kept on it, the blocks renumbered by `renumber` where its kinds were
/// taken out or moved, and its look made again where its faces are drawn
/// another way.
fn change_file(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    file: BlockFile,
    renumber: impl Fn(BlockMap) -> BlockMap,
) -> bool {
    let world = &game.engine.world;
    let Some(asset) = world.get::<BlockWorldAsset>(entity).cloned() else { return false };
    let (old, base) = (asset.file.clone(), asset.base.clone());
    let Some(blocks) = world.get::<BlockWorld>(entity).cloned() else { return false };
    if old == file {
        return false;
    }
    // A world without end is made round where it is seen as it goes (see `voxear_engine::blocks::stream`), from its
    // file: only that changes here, and a world just made endless starts with nothing made, its changes kept.
    let endless = |file: &BlockFile| file.generate.as_ref().is_some_and(|g| g.endless.is_some());
    if endless(&file) {
        let world = &mut game.engine.world;
        if !endless(&old) {
            let changes =
                Arc::new(BlockEdits::between(&renumber((*base).clone()), &renumber((*blocks.blocks).clone())));
            let (_, faces) = file.faces();
            let mut empty = BlockWorld::new(BlockMap::default(), &faces, file.block_size);
            empty.cast_shadows = blocks.cast_shadows;
            world.insert(entity, empty);
            world.insert(
                entity,
                BlockWorldAsset { file: file.clone(), base: Arc::default(), pending: changes, ..asset },
            );
        } else if let Some(kept) = world.get_mut::<BlockWorldAsset>(entity) {
            kept.file = file.clone();
        }
        restyle(game, renderer, entity, &file);
        return true;
    }
    // One no longer endless keeps every change it had, near or away.
    let carried =
        endless(&old).then(|| voxear_terrain::blocks::stream::all_changes(&base, &blocks.blocks, &asset.pending));
    let (_, faces) = file.faces();
    let grounded = old.ground != file.ground
        || old.generate != file.generate
        || old.blocks.len() != file.blocks.len()
        || old.block_size != file.block_size
        || old.blocks.iter().map(|k| k.cover).ne(file.blocks.iter().map(|k| k.cover));
    let (base, remade) = if grounded {
        // The new ground names its blocks by their new numbers already; the old ground and blocks are renumbered.
        let ground = Arc::new(voxear_engine::blocks::ground(&mut game.content, &file));
        let changes = carried
            .unwrap_or_else(|| BlockEdits::between(&renumber((*base).clone()), &renumber((*blocks.blocks).clone())));
        let mut made = voxear_engine::blocks::make(&file, &ground, Some(&changes));
        made.cast_shadows = blocks.cast_shadows;
        (ground, made)
    } else if *faces != *blocks.faces {
        (base, blocks.with_faces(&faces))
    } else {
        (base, blocks)
    };
    let world = &mut game.engine.world;
    world.insert(entity, remade);
    if let Some(asset) = world.get_mut::<BlockWorldAsset>(entity) {
        (asset.file, asset.base) = (file.clone(), base);
    }
    restyle(game, renderer, entity, &file);
    true
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
        let mut made = voxear_engine::blocks::make(&asset.file, &ground, Some(&changes));
        made.cast_shadows = blocks.cast_shadows;
        game.engine.world.insert(entity, made);
        game.engine.world.insert(entity, BlockWorldAsset { base: ground, ..asset });
    }
}

/// Gives the world's look its file's surfaces again, each material loaded,
/// where there is a renderer to load them and a look to change.
fn restyle(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, file: &BlockFile) {
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

/// What a picker offers: the project's worlds of blocks, or its materials.
pub fn choices(game: &Game, field: BlockField) -> Vec<(String, Pick)> {
    match field {
        F::Data => fields::assets_of(game, Some(AssetKind::BlockWorld)),
        F::Material | F::Top | F::Bottom => fields::assets_of(game, Some(AssetKind::Material)),
        F::Generator => fields::assets_of(game, Some(AssetKind::WorldGen)),
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
    if field != F::Data {
        let Some(mut file) = game.engine.world.get::<BlockWorldAsset>(entity).map(|a| a.file.clone()) else {
            return false;
        };
        let written = reference.map(|r| r.to_string());
        if field == F::Generator {
            let Some(generate) = &mut file.generate else { return false };
            generate.world = written;
            return change_file(game, renderer, entity, file, |blocks| blocks);
        }
        let picked = ui(&game.engine.world).block;
        let Some(kind) = file.blocks.get_mut(picked) else { return false };
        match field {
            F::Material => kind.material = written,
            F::Top => kind.top = written,
            F::Bottom => kind.bottom = written,
            _ => return false,
        }
        return change_file(game, renderer, entity, file, |blocks| blocks);
    }
    let Some(source) = game.engine.world.get::<BlockWorldSource>(entity).cloned() else { return false };
    let data = BlockWorldData { data: reference.map_or_else(String::new, |r| r.to_string()), ..source.0 };
    match voxear_engine::prefab::block_world(&mut game.content, renderer, &data) {
        Ok(parts) => {
            put(&mut game.engine.world, entity, parts);
            ui_mut(&mut game.engine.world).block = 0;
            true
        }
        Err(error) => {
            tracing::error!("{error:#}");
            false
        }
    }
}

/// Picks kind `at` of block to build with and show.
pub fn pick_block(world: &mut World, at: usize) {
    ui_mut(world).block = at;
}

/// Shows tool `tool`.
pub fn show_tool(world: &mut World, tool: BlockTool) {
    ui_mut(world).tool = tool;
}

/// Adds a kind of block of a colour of its own at the end of the list, and
/// picks it.
pub fn add_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> bool {
    let Some(mut file) = game.engine.world.get::<BlockWorldAsset>(entity).map(|a| a.file.clone()) else {
        return false;
    };
    if file.blocks.len() >= MAX_BLOCKS {
        return false;
    }
    // Colours round the wheel, one after another.
    let hue = file.blocks.len() as f32 * 0.38;
    let color = [0.0, 2.0, 4.0].map(|offset: f32| 0.45 + 0.3 * ((hue + offset / 3.0) * std::f32::consts::TAU).cos());
    file.blocks.push(BlockKind::colored(&format!("Block {}", file.blocks.len() + 1), color));
    ui_mut(&mut game.engine.world).block = file.blocks.len() - 1;
    change_file(game, renderer, entity, file, |blocks| blocks)
}

/// Takes kind `at` of block out of the list: its blocks become air, those
/// after it in the list move up a number, and the ground's layers of it
/// become air too.
pub fn remove_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, at: usize) -> bool {
    let Some(mut file) = game.engine.world.get::<BlockWorldAsset>(entity).map(|a| a.file.clone()) else {
        return false;
    };
    if at >= file.blocks.len() {
        return false;
    }
    file.blocks.remove(at);
    let gone = (at + 1) as BlockId;
    let renumber = move |block: BlockId| match block {
        AIR => AIR,
        b if b == gone => AIR,
        b if b > gone => b - 1,
        b => b,
    };
    let ground = &mut file.ground;
    (ground.top, ground.under, ground.bottom) = (renumber(ground.top), renumber(ground.under), renumber(ground.bottom));
    let state = ui_mut(&mut game.engine.world);
    state.block = state.block.min(file.blocks.len().saturating_sub(1));
    change_file(game, renderer, entity, file, move |blocks| blocks.remapped(&renumber))
}

/// Moves kind `from` of block to place `to` in the list, every block
/// renumbered with it.
pub fn move_block(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, from: usize, to: usize) -> bool {
    let Some(mut file) = game.engine.world.get::<BlockWorldAsset>(entity).map(|a| a.file.clone()) else {
        return false;
    };
    let count = file.blocks.len();
    if from >= count || to >= count || from == to {
        return false;
    }
    let mut order: Vec<usize> = (0..count).collect();
    let moved = order.remove(from);
    order.insert(to, moved);
    file.blocks = order.iter().map(|&old| file.blocks[old].clone()).collect();
    // Each old number's new one.
    let mut new_of = vec![0 as BlockId; count + 1];
    for (new, &old) in order.iter().enumerate() {
        new_of[old + 1] = (new + 1) as BlockId;
    }
    let renumber =
        move |block: BlockId| new_of.get(usize::from(block)).copied().filter(|_| block != AIR).unwrap_or(block);
    let ground = &mut file.ground;
    (ground.top, ground.under, ground.bottom) = (renumber(ground.top), renumber(ground.under), renumber(ground.bottom));
    ui_mut(&mut game.engine.world).block = to;
    change_file(game, renderer, entity, file, move |blocks| blocks.remapped(&renumber))
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
/// new asset of its own, named for its Obj, in the project's Blocks folder.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<()> {
    let name = game.engine.world.get::<voxear_scene::Name>(entity).map_or("Blocks".into(), |n| n.0.clone());
    let root = game.project.root.clone();
    let path = crate::project_files::create_block_world(&root, FOLDER, &name)?;
    game.content.rescan()?;
    let guid = game.content.parse(&path)?.guid().with_context(|| format!("{path} is not an asset"))?;
    let data = BlockWorldData { data: guid.to_string(), cast_shadows: true };
    let parts = voxear_engine::prefab::block_world(&mut game.content, renderer, &data)?;
    put(&mut game.engine.world, entity, parts);
    Ok(())
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<BlockWorld>(entity);
    world.remove::<BlockLook>(entity);
    world.remove::<BlockWorldAsset>(entity);
    world.remove::<BlockWorldSource>(entity);
}
