//! Models walked on in a world of blocks: a still collider on an Obj marked
//! **Walkable** is stamped into the chunks of blocks it covers for the
//! world's navigation alone, a block a cell its surfaces pass through, so
//! its floors and stairs are stood on, its walls go round and its doorways
//! lead in from the ground outside. Worked out again when the Obj is placed
//! or moved, in the editor and in play; one marked a door is left out, so
//! the doorway it hangs in stays open to walk through.
//!
//! Each chunk a model reaches is kept, made from the world's chunk and the
//! cells stamped into it, so navigation, which makes again only chunks that
//! changed, makes again only those a model came to, left or moved in.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_physics::scene::{Collider, ColliderMesh, Rigidbody};
use voxear_scene::{Destroying, WorldTransform};
use voxear_terrain::blocks::{
    BlockMap, BlockWorld,
    chunk::{AIR, BlockId, CHUNK, Chunk},
};

/// Marks its Obj's still collider as walked on in a world of blocks, a door
/// where `door` says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Walkable {
    /// Whether it is a door or a gate that opens: walked through, not round.
    pub door: bool,
}

impl voxear_ecs::Component for Walkable {}

/// The block a model's cells are stamped as: one no world numbers, which
/// walking takes as whole ground to stand on.
pub const MODEL_BLOCK: BlockId = BlockId::MAX - 1;

/// What one world's stamped chunks were made from.
#[derive(Default)]
struct Kept {
    /// The world's blocks and the models they were stamped from, by a number.
    source: Option<(usize, u64)>,
    map: Option<Arc<BlockMap>>,
    /// Each stamped chunk: the world's chunk it was made from, by pointer,
    /// a number for its cells, and it.
    chunks: HashMap<[i32; 3], (usize, u64, Arc<Chunk>)>,
}

#[derive(Default)]
struct Stamped(HashMap<Entity, Kept>);

/// The cells of the world's blocks each walkable model's surfaces pass
/// through, by chunk, as each chunk's own cells.
fn model_cells(world: &World, blocks: Entity, corner: Vec3, size: f32) -> (u64, HashMap<[i32; 3], Vec<[u8; 3]>>) {
    let here = voxear_scene::dimension_of(world, blocks);
    let models: Vec<(Collider, voxear_core::math::Mat4, Option<&ColliderMesh>)> = world
        .query_ref::<(Entity, &Walkable, &Collider, &WorldTransform, Option<&ColliderMesh>)>()
        .without::<Rigidbody>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, walkable, collider, ..)| {
            !walkable.door && !collider.trigger && voxear_scene::dimension_of(world, *entity) == here
        })
        .map(|(_, _, collider, placed, mesh)| (*collider, placed.0, mesh))
        .collect();
    let mut bytes: Vec<u8> = corner.to_array().iter().chain(&[size]).flat_map(|v| v.to_le_bytes()).collect();
    for (collider, matrix, mesh) in &models {
        bytes.extend(matrix.to_cols_array().iter().flat_map(|v| v.to_le_bytes()));
        bytes.extend(serde_json::to_vec(collider).unwrap_or_default());
        bytes.extend((mesh.map_or(0, |mesh| mesh as *const ColliderMesh as usize) as u64).to_le_bytes());
    }
    let hash = voxear_terrain::layers::stable_hash(&bytes);
    let mut cells: HashMap<[i32; 3], Vec<[u8; 3]>> = HashMap::new();
    let side = CHUNK as i32;
    let size = size.max(0.01);
    for (collider, matrix, mesh) in &models {
        let (points, triangles) = crate::acoustics::shape(collider, matrix, *mesh);
        for [a, b, c] in triangles {
            let (a, b, c) = (points[a as usize], points[b as usize], points[c as usize]);
            // Points over the triangle half a block apart, each marking its cell.
            let longest = (b - a).length().max((c - a).length()).max((c - b).length());
            let steps = ((longest / (size * 0.5)).ceil() as usize).clamp(1, 512);
            for i in 0..=steps {
                for j in 0..=steps - i {
                    let (u, v) = (i as f32 / steps as f32, j as f32 / steps as f32);
                    let at = (a + (b - a) * u + (c - a) * v - corner) / size;
                    let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
                    let key = cell.map(|v| v.div_euclid(side));
                    let inside = cell.map(|v| v.rem_euclid(side) as u8);
                    cells.entry(key).or_default().push(inside);
                }
            }
        }
    }
    for list in cells.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    (hash, cells)
}

/// The blocks the navigation of the world of blocks on `entity` is made
/// from: its own, with its walkable models stamped in; its own where it has
/// none.
pub fn for_navigation(world: &mut World, entity: Entity) -> Option<Arc<BlockMap>> {
    let blocks = world.get::<BlockWorld>(entity)?;
    let source = blocks.blocks.clone();
    let size = blocks.block_size;
    let corner = crate::ground_fit::world_corner(world, entity)?;
    let any = world.query_ref::<(Entity, &Walkable)>().into_iter().next().is_some();
    if !any {
        world.resource_mut::<Stamped>().map(|stamped| stamped.0.remove(&entity));
        return Some(source);
    }
    let (hash, cells) = model_cells(world, entity, corner, size);
    let pointer = Arc::as_ptr(&source) as usize;
    let mut stamped = world.remove_resource::<Stamped>().unwrap_or_default();
    let kept = stamped.0.entry(entity).or_default();
    if kept.source == Some((pointer, hash))
        && let Some(map) = &kept.map
    {
        let map = map.clone();
        world.insert_resource(stamped);
        return Some(map);
    }
    let mut map = (*source).clone();
    let mut chunks: HashMap<[i32; 3], (usize, u64, Arc<Chunk>)> = HashMap::new();
    for (key, list) in &cells {
        let own = source.chunk(*key);
        let own_pointer = own.map_or(0, |chunk| Arc::as_ptr(chunk) as usize);
        let list_hash = voxear_terrain::layers::stable_hash(&list.concat());
        let made = match kept.chunks.get(key) {
            Some((was, was_hash, chunk)) if *was == own_pointer && *was_hash == list_hash => chunk.clone(),
            _ => {
                let mut chunk = own.map_or_else(|| Chunk::filled(AIR), |chunk| (**chunk).clone());
                for &[x, y, z] in list {
                    let (x, y, z) = (u32::from(x), u32::from(y), u32::from(z));
                    if chunk.get(x, y, z) == AIR {
                        chunk.set(x, y, z, MODEL_BLOCK);
                    }
                }
                Arc::new(chunk)
            }
        };
        map.put_shared(*key, made.clone());
        chunks.insert(*key, (own_pointer, list_hash, made));
    }
    let map = Arc::new(map);
    *kept = Kept { source: Some((pointer, hash)), map: Some(map.clone()), chunks };
    world.insert_resource(stamped);
    Some(map)
}

/// How many cells of the world on `entity` its walkable models stand in, as
/// last stamped.
pub fn stamped_cells(world: &World, entity: Entity) -> usize {
    let Some(kept) = world.resource::<Stamped>().and_then(|stamped| stamped.0.get(&entity)) else { return 0 };
    let Some(map) = &kept.map else { return 0 };
    kept.chunks
        .keys()
        .filter_map(|key| map.chunk(*key))
        .map(|chunk| {
            let side = CHUNK;
            (0..side * side * side)
                .filter(|i| chunk.get(i % side, i / side % side, i / (side * side)) == MODEL_BLOCK)
                .count()
        })
        .sum()
}
