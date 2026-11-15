//! The dungeons a world of blocks' generator lays out, as JSON in the
//! level's metres: for scripts (`Dungeons` in Voxear.Core) and for
//! `vox editor call dungeon.layouts`.

use anyhow::{Result, bail};
use serde_json::{Value, json};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld,
    dungeon::{DungeonLayout, EntranceKind, LightKind, SpaceKind, WayKind},
    structure::dungeons_near,
};

use crate::{Content, blocks::BlockWorldAsset};

/// The dungeons of the world of blocks on `obj` (0 for each world of blocks
/// of the level, in the order of their Objs) whose layouts reach within
/// `at[3]` metres of the point `at` in the level, as JSON.
pub fn dungeons_json(world: &World, content: &mut Content, obj: u64, at: [f32; 4]) -> Result<String> {
    let worlds: Vec<Entity> = match obj {
        0 => {
            let mut worlds: Vec<Entity> = world
                .query_ref::<(Entity, &BlockWorld)>()
                .without::<Destroying>()
                .into_iter()
                .map(|(e, _)| e)
                .collect();
            worlds.sort_by_key(|entity| entity.to_bits());
            worlds
        }
        obj => vec![Entity::from_bits(obj)],
    };
    let mut dungeons = Vec::new();
    for entity in worlds {
        dungeons.extend(of_world(world, content, entity, at)?);
    }
    Ok(json!({ "dungeons": dungeons }).to_string())
}

fn of_world(world: &World, content: &mut Content, entity: Entity, at: [f32; 4]) -> Result<Vec<Value>> {
    let Some(asset) = world.get::<BlockWorldAsset>(entity) else { return Ok(Vec::new()) };
    let file = asset.file.clone();
    let Some(generate) = file.generate.as_ref() else { return Ok(Vec::new()) };
    let Some(built) = generate.built.as_deref() else { return Ok(Vec::new()) };
    // The generator its chunks are built on, rivers and all, so the layouts are the ones built.
    let Some(landed) = crate::blocks::generator(content, &file) else { return Ok(Vec::new()) };
    let generator = voxear_terrain::blocks::generate::ground_generator(generate, &landed, file.block_size.max(0.01));
    let Some(transform) = world.get::<Transform>(entity) else { bail!("the world of blocks has no Transform") };
    let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
    let size = file.block_size;
    let [px, pz] = generate.position;
    let biomes = generate.placed.as_deref().filter(|placed| !placed.biomes.is_empty());
    let halved = generate.halves.is_some() && generate.slabs && !file.smooth;
    let laid = (size, generate.position, generate.bottom, halved);
    let near = [at[0] - corner.x + px, at[2] - corner.z + pz];
    let found = dungeons_near(built, &generator, biomes, laid, near, at[3].max(0.0));
    let metres = |block: [f32; 3]| (corner + Vec3::new(block[0], block[1], block[2]) * size).to_array();
    let centre = |block: [i32; 3]| metres([block[0] as f32 + 0.5, block[1] as f32 + 1.0, block[2] as f32 + 0.5]);
    Ok(found
        .into_iter()
        .map(|(which, _, layout)| layout_json(&layout, which, entity, &metres, &centre, size))
        .collect())
}

fn layout_json(
    layout: &DungeonLayout,
    which: usize,
    entity: Entity,
    metres: &dyn Fn([f32; 3]) -> [f32; 3],
    centre: &dyn Fn([i32; 3]) -> [f32; 3],
    size: f32,
) -> Value {
    let e = layout.entrance;
    let spaces: Vec<Value> = layout
        .spaces
        .iter()
        .filter(|space| space.height >= 0)
        .map(|space| {
            json!({
                "kind": space_name(space.kind), "floor": space.floor, "at": centre(space.at),
                "size": [(space.half[0] * 2 + 1) as f32 * size, space.height as f32 * size, (space.half[1] * 2 + 1) as f32 * size],
                "upper": metres([0.0, space.upper as f32 + 1.0, 0.0])[1], "below": space.below as f32 * size,
                "main": space.main, "progress": space.progress,
            })
        })
        .collect();
    // Spaces left out of the plan are left out of the list, so ways name spaces by their place in it.
    let live: Vec<usize> = (0..layout.spaces.len()).filter(|&at| layout.spaces[at].height >= 0).collect();
    let index = |space: usize| live.iter().position(|&at| at == space).map_or(-1, |at| at as i64);
    let ways: Vec<Value> = layout
        .ways
        .iter()
        .map(|way| {
            let points: Vec<[f32; 3]> = way.line.iter().map(|p| metres([p[0], p[2] * 0.5, p[1]])).collect();
            json!({
                "kind": match way.kind { WayKind::Grand => "Grand", WayKind::Passage => "Passage", WayKind::Shortcut => "Shortcut" },
                "main": way.main, "from": way.from.map_or(-1, index), "to": index(way.to),
                "width": way.width as f32 * size, "points": points,
            })
        })
        .collect();
    let lights: Vec<Value> = layout
        .lights
        .iter()
        .map(|light| {
            let kind = match light.kind {
                LightKind::Brazier => "Brazier",
                LightKind::Crystal => "Crystal",
                LightKind::Glow => "Glow",
            };
            json!({ "kind": kind, "at": metres([light.at[0] as f32 + 0.5, light.at[1] as f32, light.at[2] as f32 + 0.5]) })
        })
        .collect();
    let kind = match e.kind {
        EntranceKind::Cliff => "Cliff",
        EntranceKind::Temple => "Temple",
        EntranceKind::Sinkhole => "Sinkhole",
    };
    json!({
        "world": entity.to_bits(), "structure": which, "floors": layout.floors,
        "entrance": { "kind": kind, "at": centre(e.at), "facing": [-e.into[0], 0, -e.into[1]] },
        "spaces": spaces, "ways": ways, "lights": lights,
    })
}

fn space_name(kind: SpaceKind) -> &'static str {
    match kind {
        SpaceKind::Landing => "Landing",
        SpaceKind::Room => "Room",
        SpaceKind::Hall => "Hall",
        SpaceKind::Gorge => "Gorge",
        SpaceKind::Chasm => "Chasm",
        SpaceKind::Flooded => "Flooded",
        SpaceKind::Arena => "Arena",
        SpaceKind::Descent => "Descent",
        SpaceKind::Treasury => "Treasury",
        SpaceKind::Shrine => "Shrine",
    }
}
