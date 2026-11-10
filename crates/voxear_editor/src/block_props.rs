//! A Block World in the Inspector: its tools, **Build** and **Settings**.
//! Under Build, its kinds of block (see `block_panel`), the brush the Scene
//! view builds with (see `block_tools`), and the picked kind's name,
//! colour and materials. Under Settings, the `.blockworld` asset its blocks
//! are, how large a block is, the flat ground its blocks start as, and
//! whether it casts the sun's shadows. A change to the asset's file makes
//! its blocks again at once, the changes built since kept on them, kept in
//! the world for saving and undo as a Terrain's are.

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    blocks::BlockWorldAsset,
    prefab::BlockWorldParts,
    scene_file::{BlockWorldData, BlockWorldSource},
};
use voxear_render::{BlockLook, Renderer};
use voxear_terrain::blocks::{
    BlockWorld,
    chunk::{AIR, BlockId},
    edits::BlockEdits,
    file::{BlockFile, BlockKind, MAX_BLOCKS},
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
    /// The brush: what it does and how many blocks across.
    Brush,
    BrushSize,
    /// The picked kind of block's name, colour and materials.
    Name,
    Color,
    Material,
    Top,
    Bottom,
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
            if state.block < file.blocks.len() {
                fields.extend([F::Name, F::Color, F::Material, F::Top, F::Bottom]);
            }
            fields
        }
        (BlockTool::Build, None) => vec![F::Data],
        (BlockTool::Settings, Some(_)) => vec![F::Data, F::BlockSize, F::GroundSize, F::GroundHeight, F::Shadows],
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
        F::BlockSize => Some("m"),
        _ => None,
    }
}

/// Whether the value is an asset picked from a list.
pub fn is_picked(field: BlockField) -> bool {
    matches!(field, F::Data | F::Material | F::Top | F::Bottom)
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
    })
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
    renumber: impl Fn(voxear_terrain::blocks::BlockMap) -> voxear_terrain::blocks::BlockMap,
) -> bool {
    let world = &mut game.engine.world;
    let Some(old) = world.get::<BlockWorldAsset>(entity).map(|asset| asset.file.clone()) else { return false };
    let Some(blocks) = world.get::<BlockWorld>(entity).cloned() else { return false };
    if old == file {
        return false;
    }
    let (_, faces) = file.faces();
    let remade =
        if old.ground != file.ground || old.blocks.len() != file.blocks.len() || old.block_size != file.block_size {
            // The new ground names its blocks by their new numbers already; the old ground and blocks are renumbered.
            let mut map = file.make_ground();
            let renumbered = renumber((*blocks.blocks).clone());
            // The changes, renumbered as the blocks are, on the new ground.
            BlockEdits::between(&renumber(old.make_ground()), &renumbered).apply(&mut map);
            let mut made = BlockWorld::new(map, &faces, file.block_size);
            made.cast_shadows = blocks.cast_shadows;
            made
        } else if *faces != *blocks.faces {
            blocks.with_faces(&faces)
        } else {
            blocks
        };
    world.insert(entity, remade);
    if let Some(asset) = world.get_mut::<BlockWorldAsset>(entity) {
        asset.file = file.clone();
    }
    restyle(game, renderer, entity, &file);
    true
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
        let picked = ui(&game.engine.world).block;
        let Some(kind) = file.blocks.get_mut(picked) else { return false };
        let written = reference.map(|r| r.to_string());
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
