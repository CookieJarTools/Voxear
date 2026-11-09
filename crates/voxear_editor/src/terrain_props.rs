//! The Terrain component in the Inspector (the Terrain module).
//!
//! First its `.terrain` asset and what the asset says: how large and how
//! fine the ground is, and the grey picture its heights are made from, with
//! how high its white is. Changing those makes the ground again, from the
//! picture or from the heights as they were, stretched to fit; the asset is
//! written when the level is saved (`voxear_engine::terrain`). Then how it
//! is drawn: its material, how large one picture of it is on the ground,
//! how far its finest detail reaches, and whether it casts shadows.

use std::sync::Arc;

use anyhow::{Context, Result};
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game, MaterialSource,
    scene_file::{MaterialRef, TerrainData, TerrainSource, default_material},
    terrain::TerrainAsset,
};
use voxear_render::{Renderer, TerrainLook};
use voxear_terrain::{Terrain, file::RESOLUTIONS};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Where a new terrain's asset is made, in the project's folder.
const FOLDER: &str = "Terrains";

/// The values a Terrain shows: what its asset says only when it has one,
/// and how high the heightmap's white is only when it has a heightmap.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let mut props = vec![Prop::TerrainData];
    if let Some(asset) = world.get::<TerrainAsset>(entity) {
        props.extend([Prop::TerrainSize, Prop::TerrainResolution, Prop::TerrainHeightmap]);
        if asset.file.heightmap.is_some() {
            props.push(Prop::TerrainHeight);
        }
    }
    props.extend([Prop::TerrainMaterial, Prop::TerrainTextureSize, Prop::TerrainDetail, Prop::TerrainShadows]);
    props
}

pub fn label(prop: &Prop) -> &'static str {
    match prop {
        Prop::TerrainData => "Data",
        Prop::TerrainSize => "Size",
        Prop::TerrainResolution => "Resolution",
        Prop::TerrainHeightmap => "Heightmap",
        Prop::TerrainHeight => "Height",
        Prop::TerrainMaterial => "Material",
        Prop::TerrainTextureSize => "Texture size",
        Prop::TerrainDetail => "Detail distance",
        _ => "Cast shadows",
    }
}

/// The name a prefab's locks and a scene file give the value.
pub fn key(prop: &Prop) -> &'static str {
    match prop {
        Prop::TerrainData => "data",
        Prop::TerrainSize => "size",
        Prop::TerrainResolution => "resolution",
        Prop::TerrainHeightmap => "heightmap",
        Prop::TerrainHeight => "height",
        Prop::TerrainMaterial => "material",
        Prop::TerrainTextureSize => "texture_size",
        Prop::TerrainDetail => "detail_distance",
        _ => "cast_shadows",
    }
}

pub fn unit(prop: &Prop) -> Option<&'static str> {
    match prop {
        Prop::TerrainSize | Prop::TerrainHeight | Prop::TerrainTextureSize | Prop::TerrainDetail => Some("m"),
        _ => None,
    }
}

/// An asset's path, by the reference a file holds, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

pub fn get(game: &Game, entity: Entity, prop: &Prop) -> Option<Value> {
    let world = &game.engine.world;
    let asset = || world.get::<TerrainAsset>(entity);
    Some(match prop {
        Prop::TerrainData => Value::Asset(path_of(game, Some(&world.get::<TerrainSource>(entity)?.0.data))),
        Prop::TerrainSize => Value::Number(asset()?.file.size),
        Prop::TerrainResolution => {
            let resolution = asset()?.file.resolution;
            let at = RESOLUTIONS.iter().position(|&r| r == resolution).unwrap_or_default();
            Value::Choice(at, RESOLUTIONS.iter().map(|r| r.to_string()).collect())
        }
        Prop::TerrainHeightmap => Value::Asset(path_of(game, asset()?.file.heightmap.as_deref())),
        Prop::TerrainHeight => Value::Number(asset()?.file.height),
        Prop::TerrainMaterial => {
            let look = world.get::<TerrainLook>(entity)?;
            Value::Asset(match game.content.material_source(look.material)? {
                MaterialSource::Inline(data) if *data == default_material() => "Default".into(),
                MaterialSource::Asset(reference) => path_of(game, Some(&reference.to_string())),
                MaterialSource::Inline(_) => "Custom".into(),
            })
        }
        Prop::TerrainTextureSize => Value::Number(world.get::<TerrainLook>(entity)?.texture_size),
        Prop::TerrainDetail => Value::Number(world.get::<Terrain>(entity)?.detail_distance),
        Prop::TerrainShadows => Value::Toggle(world.get::<Terrain>(entity)?.cast_shadows),
        _ => return None,
    })
}

/// Sets a number, a toggle or a choice; the asset's settings make the
/// ground again.
pub fn set(game: &mut Game, entity: Entity, prop: &Prop, value: Value) -> bool {
    let world = &mut game.engine.world;
    match (prop, value) {
        (Prop::TerrainTextureSize, Value::Number(size)) => {
            change(world, entity, |look: &mut TerrainLook| look.texture_size = size.max(0.01))
        }
        (Prop::TerrainDetail, Value::Number(distance)) => {
            change(world, entity, |terrain: &mut Terrain| terrain.detail_distance = distance.max(1.0))
        }
        (Prop::TerrainShadows, Value::Toggle(on)) => {
            change(world, entity, |terrain: &mut Terrain| terrain.cast_shadows = on)
        }
        (Prop::TerrainSize, Value::Number(size)) => {
            reshape(game, entity, |file| file.size = size.clamp(1.0, 100_000.0))
        }
        (Prop::TerrainResolution, Value::Choice(at, _)) => {
            let Some(&resolution) = RESOLUTIONS.get(at) else { return false };
            reshape(game, entity, |file| file.resolution = resolution)
        }
        (Prop::TerrainHeight, Value::Number(height)) => reshape(game, entity, |file| file.height = height),
        _ => false,
    }
}

fn change<T: voxear_ecs::Component>(world: &mut World, entity: Entity, how: impl FnOnce(&mut T)) -> bool {
    let Some(component) = world.get_mut::<T>(entity) else { return false };
    how(component);
    true
}

/// Changes the terrain's asset's file and makes its ground again: from its
/// heightmap, if it has one, or else from its heights stretched to fit.
fn reshape(game: &mut Game, entity: Entity, how: impl FnOnce(&mut voxear_terrain::file::TerrainFile)) -> bool {
    let world = &game.engine.world;
    let (Some(asset), Some(terrain)) = (world.get::<TerrainAsset>(entity), world.get::<Terrain>(entity)) else {
        return false;
    };
    let mut file = asset.file.clone();
    how(&mut file);
    if file == asset.file {
        return false;
    }
    let old = terrain.heights.clone();
    let heights = Arc::new(game.content.terrain_heights(&file, Some(&old)));
    let world = &mut game.engine.world;
    change(world, entity, |terrain: &mut Terrain| terrain.heights = heights)
        && change(world, entity, |asset: &mut TerrainAsset| asset.file = file)
}

/// What a picker shows: the project's terrains, its pictures for a
/// heightmap, or its materials.
pub fn choices(game: &Game, prop: &Prop) -> Vec<(String, Pick)> {
    match prop {
        Prop::TerrainData => fields::assets_of(game, Some(AssetKind::Terrain)),
        Prop::TerrainHeightmap => fields::assets_of(game, Some(AssetKind::Texture)),
        Prop::TerrainMaterial => {
            let mut found = vec![("Default".to_string(), Pick::Asset(None))];
            let mut paths: Vec<String> = (game.content.db().entries())
                .filter(|entry| entry.kind == AssetKind::Material)
                .map(|entry| entry.path.clone())
                .collect();
            paths.sort();
            found.extend(paths.into_iter().map(|path| (path.clone(), Pick::Asset(Some(path)))));
            found
        }
        _ => Vec::new(),
    }
}

/// Takes what was picked: another asset for its ground, a picture for its
/// heights, or a material.
pub fn pick(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity, prop: &Prop, choice: &Pick) -> bool {
    let Pick::Asset(path) = choice else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference),
            Err(_) => return false,
        },
        None => None,
    };
    match prop {
        Prop::TerrainData => {
            let Some(source) = game.engine.world.get::<TerrainSource>(entity).cloned() else { return false };
            let data = TerrainData { data: reference.map_or_else(String::new, |r| r.to_string()), ..source.0 };
            match voxear_engine::prefab::terrain(&mut game.content, renderer, &data) {
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
        Prop::TerrainHeightmap => {
            // Without a picture, the heights stay as the picture made them, to change by hand.
            let picture = reference.map(|r| r.to_string());
            reshape(game, entity, |file| file.heightmap = picture)
        }
        Prop::TerrainMaterial => {
            let Some(renderer) = renderer else { return false };
            let source = match &reference {
                None => MaterialSource::Inline(default_material()),
                Some(reference) => MaterialSource::Asset(reference.clone()),
            };
            let Ok(material) = game.content.material(renderer, &source) else { return false };
            let world = &mut game.engine.world;
            let written = reference.map(|r| MaterialRef::Asset(r.to_string()));
            change(world, entity, |look: &mut TerrainLook| look.material = material)
                && change(world, entity, |source: &mut TerrainSource| source.0.material = written)
        }
        _ => false,
    }
}

/// Puts a Terrain's parts on an Obj, in place of any it had.
fn put(world: &mut World, entity: Entity, parts: voxear_engine::prefab::TerrainParts) {
    world.insert(entity, parts.terrain);
    match parts.look {
        Some(look) => world.insert(entity, look),
        None => world.remove::<TerrainLook>(entity).is_some(),
    };
    match parts.asset {
        Some(asset) => world.insert(entity, asset),
        None => world.remove::<TerrainAsset>(entity).is_some(),
    };
    world.insert(entity, parts.source);
}

/// Add Component's Terrain: flat ground on a new asset of its own, named
/// for its Obj, in the project's Terrains folder.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<()> {
    let name = game.engine.world.get::<voxear_scene::Name>(entity).map_or("Terrain".into(), |n| n.0.clone());
    let root = game.project.root.clone();
    let path = crate::project_files::create_terrain(&root, FOLDER, &name)?;
    game.content.rescan()?;
    let guid = game.content.parse(&path)?.guid().with_context(|| format!("{path} is not an asset"))?;
    let data = TerrainData {
        data: guid.to_string(),
        material: None,
        texture_size: TerrainLook::DEFAULT_TEXTURE_SIZE,
        detail_distance: Terrain::DEFAULT_DETAIL_DISTANCE,
        cast_shadows: true,
    };
    let parts = voxear_engine::prefab::terrain(&mut game.content, renderer, &data)?;
    put(&mut game.engine.world, entity, parts);
    Ok(())
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<Terrain>(entity);
    world.remove::<TerrainLook>(entity);
    world.remove::<TerrainAsset>(entity);
    world.remove::<TerrainSource>(entity);
}
