//! A Block World in the Inspector: the `.blockworld` asset its blocks are,
//! how large a block is, the flat ground its blocks start as, and whether it
//! casts the sun's shadows. A change to the asset's file makes its blocks
//! again at once, kept in the world for saving and undo as a Terrain's are.

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
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Where Add Component puts a new world of blocks' asset.
const FOLDER: &str = "Blocks";

/// A value a Block World shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockField {
    Data,
    BlockSize,
    /// How many blocks its ground reaches along x and z, and how many up.
    GroundSize,
    GroundHeight,
    Shadows,
}

use BlockField as F;

/// The values a Block World shows: its asset's where it has one.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let mut fields = vec![F::Data];
    if world.has::<BlockWorldAsset>(entity) {
        fields.extend([F::BlockSize, F::GroundSize, F::GroundHeight]);
    }
    fields.push(F::Shadows);
    fields.into_iter().map(Prop::BlockWorld).collect()
}

pub fn label(field: BlockField) -> &'static str {
    match field {
        F::Data => "Data",
        F::BlockSize => "Block size",
        F::GroundSize => "Ground size",
        F::GroundHeight => "Ground height",
        F::Shadows => "Cast shadows",
    }
}

/// The name a prefab's locks give the value.
pub fn key(field: BlockField) -> &'static str {
    match field {
        F::Data => "data",
        F::Shadows => "cast_shadows",
        // The rest are the asset's, not the scene's.
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
    field == F::Data
}

pub fn get(game: &Game, entity: Entity, field: BlockField) -> Option<Value> {
    let world = &game.engine.world;
    let file = || world.get::<BlockWorldAsset>(entity).map(|asset| &asset.file);
    Some(match field {
        F::Data => {
            let reference = &world.get::<BlockWorldSource>(entity)?.0.data;
            let guid = game.content.parse(reference).ok().and_then(|reference| reference.guid());
            Value::Asset(guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |e| e.path.clone()))
        }
        F::BlockSize => Value::Number(file()?.block_size),
        F::GroundSize => Value::Vector(file()?.ground.size.map(|v| v as f32).to_vec()),
        F::GroundHeight => Value::Number(file()?.ground.height as f32),
        F::Shadows => Value::Toggle(world.get::<BlockWorld>(entity)?.cast_shadows),
    })
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Whole(n) => Some(*n as f32),
        _ => None,
    }
}

/// Sets a value: whether it casts shadows on the world, the rest on its
/// asset's file, its blocks made again from it.
pub fn set(game: &mut Game, entity: Entity, field: BlockField, value: Value) -> bool {
    let world = &mut game.engine.world;
    if field == F::Shadows {
        let Value::Toggle(on) = value else { return false };
        let Some(blocks) = world.get_mut::<BlockWorld>(entity) else { return false };
        blocks.cast_shadows = on;
        return true;
    }
    let Some(mut file) = world.get::<BlockWorldAsset>(entity).map(|asset| asset.file.clone()) else { return false };
    match (field, &value) {
        (F::BlockSize, v) => file.block_size = number(v).unwrap_or(file.block_size).clamp(0.01, 100.0),
        (F::GroundSize, Value::Vector(v)) if v.len() >= 2 => {
            file.ground.size = [v[0], v[1]].map(|n| n.round().clamp(0.0, 4096.0) as u32);
        }
        (F::GroundHeight, v) => {
            file.ground.height = number(v).unwrap_or(file.ground.height as f32).round().clamp(0.0, 1024.0) as u32;
        }
        _ => return false,
    }
    remake(world, entity, file)
}

/// Puts `file` on the world's asset and makes its blocks again from it,
/// its surfaces as large as a block.
fn remake(world: &mut World, entity: Entity, file: BlockFile) -> bool {
    let Some(asset) = world.get_mut::<BlockWorldAsset>(entity) else { return false };
    if asset.file == file {
        return false;
    }
    asset.file = file.clone();
    let shadows = world.get::<BlockWorld>(entity).is_none_or(|blocks| blocks.cast_shadows);
    let mut blocks = voxear_engine::blocks::make(&file);
    blocks.cast_shadows = shadows;
    world.insert(entity, blocks);
    if let Some(look) = world.get::<BlockLook>(entity).cloned() {
        let surfaces: Vec<_> = look
            .surfaces
            .iter()
            .map(|surface| voxear_render::SurfaceLook { size: file.block_size, ..*surface })
            .collect();
        world.insert(entity, BlockLook { surfaces: surfaces.into(), ..look });
    }
    true
}

/// What the Data picker offers: the project's worlds of blocks.
pub fn choices(game: &Game, field: BlockField) -> Vec<(String, Pick)> {
    match field {
        F::Data => fields::assets_of(game, Some(AssetKind::BlockWorld)),
        _ => Vec::new(),
    }
}

/// Takes another asset for its blocks.
pub fn pick(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    field: BlockField,
    choice: &Pick,
) -> bool {
    let (F::Data, Pick::Asset(path)) = (field, choice) else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference),
            Err(_) => return false,
        },
        None => None,
    };
    let Some(source) = game.engine.world.get::<BlockWorldSource>(entity).cloned() else { return false };
    let data = BlockWorldData { data: reference.map_or_else(String::new, |r| r.to_string()), ..source.0 };
    match voxear_engine::prefab::block_world(&mut game.content, renderer, &data) {
        Ok(parts) => {
            put(&mut game.engine.world, entity, parts);
            true
        }
        Err(error) => {
            tracing::error!("{error:#}");
            false
        }
    }
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
