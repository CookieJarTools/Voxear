//! The towns a world of blocks' generator lays out, as JSON in the level's
//! metres, worked out from the generator with no chunk made: for scripts
//! (`Towns` in Voxear.Core) and for `vox editor call towns.near`. Each town
//! is its structure, its site and its buildings, the middle first: each
//! building's structure, where its floor's middle is, its size, which way
//! it faces and where its door opens. A house's door is on its model's -z
//! side, turned with it.

use anyhow::{Result, bail};
use serde_json::{Value, json};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{BlockWorld, structure::towns_near};

use crate::{Content, blocks::BlockWorldAsset};

/// Which way a building's door faces, by how many quarter turns it is
/// turned: along -z unturned.
const FRONTS: [[f32; 2]; 4] = [[0.0, -1.0], [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]];

/// The towns of the world of blocks on `obj` (0 for each world of blocks of
/// the dimension asked of, in the order of their Objs) whose sites lie
/// within `at[3]` metres of the point `at` in the level, and each town's own
/// reach, as JSON.
pub fn towns_json(world: &World, content: &mut Content, obj: u64, at: [f32; 4]) -> Result<String> {
    let worlds: Vec<Entity> = match obj {
        0 => {
            let current = crate::dimensions::current(world);
            let mut worlds: Vec<Entity> = world
                .query_ref::<(Entity, &BlockWorld)>()
                .without::<Destroying>()
                .into_iter()
                .map(|(e, _)| e)
                .filter(|&entity| voxear_scene::dimension_of(world, entity) == current)
                .collect();
            worlds.sort_by_key(|entity| entity.to_bits());
            worlds
        }
        obj => vec![Entity::from_bits(obj)],
    };
    let mut towns = Vec::new();
    for entity in worlds {
        towns.extend(of_world(world, content, entity, at)?);
    }
    Ok(json!({ "towns": towns }).to_string())
}

fn of_world(world: &World, content: &mut Content, entity: Entity, at: [f32; 4]) -> Result<Vec<Value>> {
    let Some(asset) = world.get::<BlockWorldAsset>(entity) else { return Ok(Vec::new()) };
    let file = asset.file.clone();
    let Some(generate) = file.generate.as_ref() else { return Ok(Vec::new()) };
    let Some(built) = generate.built.as_deref() else { return Ok(Vec::new()) };
    // The generator its chunks are built on, rivers and all, so the towns are the ones built.
    let Some(landed) = crate::blocks::generator(content, &file) else { return Ok(Vec::new()) };
    let generator = voxear_terrain::blocks::generate::ground_generator(generate, &landed, file.block_size.max(0.01));
    let Some(transform) = world.get::<Transform>(entity) else { bail!("the world of blocks has no Transform") };
    let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
    let size = file.block_size;
    let [px, pz] = generate.position;
    let biomes = generate.placed.as_deref().filter(|placed| !placed.biomes.is_empty());
    let halved = voxear_terrain::blocks::generate::halved(generate) && !file.smooth;
    let laid = (size, generate.position, generate.lowest_open(), halved);
    let near = [at[0] - corner.x + px, at[2] - corner.z + pz];
    let found = towns_near(built, &generator, biomes, laid, near, at[3].max(0.0));
    // A block's middle along x and z, at the bottom of block `y`, in the level's metres.
    let metres =
        |[x, z]: [i32; 2], y: i32| (corner + Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5) * size).to_array();
    // A town that laid out no building at all is not a town to tell of.
    Ok(found
        .into_iter()
        .filter(|plan| !plan.buildings.is_empty())
        .map(|plan| {
            let source = built.sources.get(plan.which);
            let buildings: Vec<Value> = plan
                .buildings
                .iter()
                .map(|building| {
                    let structure = match building.house {
                        Some(kind) => source.and_then(|source| source.houses.get(kind).cloned()),
                        None => source.and_then(|source| source.middle.clone()),
                    };
                    let turns = usize::from(building.turns % 4);
                    let [w, h, d] = building.size.map(|side| side as f32 * size);
                    json!({
                        "structure": structure, "middle": building.house.is_none(),
                        "at": metres(building.at, building.floor), "size": [w, h, d],
                        "turns": turns, "yaw": -(turns as f32) * std::f32::consts::FRAC_PI_2,
                        "front": [FRONTS[turns][0], 0.0, FRONTS[turns][1]],
                        "door": building.door.map(|door| metres(door, building.floor)),
                    })
                })
                .collect();
            let first = &plan.buildings[0];
            let middle = metres(first.at, first.floor);
            json!({
                "world": entity.to_bits(), "structure": source.map(|source| source.structure.clone()),
                "site": { "at": plan.site.at, "seed": plan.site.seed }, "middle": middle, "buildings": buildings,
            })
        })
        .collect())
}
