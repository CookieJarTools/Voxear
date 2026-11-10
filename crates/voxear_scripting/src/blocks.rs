//! Worlds of blocks for C#: `Blocks` in Voxear.Core asks which block is at
//! a point and where a ray meets one, and changes them: a block, a box or a
//! ball of them set to a kind or to air, every world they reach, each
//! chunk changed made again (see `voxear_terrain::blocks`). What scripts
//! changed is kept for a save of the scene or a player joining a game, as
//! only what differs from the world as its level built it.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld, BuiltBlocks,
    chunk::{AIR, BlockId},
    edits::BlockEdits,
    smooth::Ball,
};

use crate::{interop::with_world, services::with_services};

/// Where a ray met a block, as `BlockHitData` in Voxear.Core reads it: the
/// world's Obj, the point and the way its face faces, how far along, the
/// middle of the block and how large a block is.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockHitData {
    pub obj: u64,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub middle: [f32; 3],
    pub distance: f32,
    pub block_size: f32,
    pub block: u32,
}

const _: () = assert!(size_of::<BlockHitData>() == 56, "BlockHitData in Voxear.Core is 56 bytes");

/// Every world of blocks, with its corner.
fn worlds(world: &World) -> Vec<(Entity, Vec3)> {
    world
        .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _, local, placed)| (entity, voxear_terrain::corner(local, placed)))
        .collect()
}

/// The block at `at` in the first world of blocks with one there, and its
/// Obj; 0 for air everywhere.
pub(crate) unsafe extern "C" fn block_at(at: *const [f32; 3], obj: *mut u64) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = Vec3::from(unsafe { *at });
    let found = with_world(|world| {
        worlds(world).into_iter().find_map(|(entity, corner)| {
            let blocks = world.get::<BlockWorld>(entity)?;
            let (_, block) = blocks.block_at((at - corner).to_array());
            (block != AIR).then_some((entity, block))
        })
    });
    let Some((entity, block)) = found else { return 0 };
    if !obj.is_null() {
        unsafe { *obj = entity.to_bits() };
    }
    i32::from(block)
}

/// What `block_set` changes: one block, a box between two points, or a ball.
const ONE: i32 = 0;
const BOX: i32 = 1;
const BALL: i32 = 2;

/// Which blocks `block_set` changes: all it reaches, only air (to place),
/// or only blocks (to paint).
const ALL: i32 = 0;
const AIR_ONLY: i32 = 1;
const BLOCKS_ONLY: i32 = 2;

/// Sets the blocks `shape` reaches to `block` (0 for air) in every world of
/// blocks, those `only` allows: at `a`; in the box from `a` to `b`; or in the
/// ball round `a`, `b`'s first number its radius, all in metres in the world.
/// How many blocks changed.
pub(crate) unsafe extern "C" fn block_set(
    shape: i32,
    a: *const [f32; 3],
    b: *const [f32; 3],
    block: i32,
    only: i32,
) -> i32 {
    if a.is_null() || (shape != ONE && b.is_null()) || !(0..=i32::from(u16::MAX)).contains(&block) {
        return 0;
    }
    let (a, b) = (Vec3::from(unsafe { *a }), if b.is_null() { Vec3::ZERO } else { Vec3::from(unsafe { *b }) });
    let block = block as BlockId;
    with_world(|world| {
        let mut changed = 0;
        for (entity, corner) in worlds(world) {
            let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { continue };
            let size = blocks.block_size;
            // Drawn smooth, a ball's surface lies between blocks.
            if shape == BALL && blocks.smooth {
                let does = match (block, only) {
                    (AIR, _) => Ball::Take,
                    (_, BLOCKS_ONLY) => Ball::Paint(block),
                    _ => Ball::Add(block),
                };
                let middle = ((a - corner) / size).to_array();
                let radius = b.x.max(0.0) / size;
                let changes = voxear_terrain::blocks::smooth::ball(&blocks.blocks, middle, radius, does, &blocks.faces);
                if changes.is_empty() {
                    continue;
                }
                if !world.has::<BuiltBlocks>(entity) {
                    world.insert(entity, BuiltBlocks(blocks.blocks.clone()));
                }
                let count = blocks.set_cells(changes);
                if count > 0 {
                    world.insert(entity, blocks);
                    changed += count as i32;
                }
                continue;
            }
            let cell = |p: Vec3| ((p - corner) / size).floor().as_ivec3();
            let reached: Vec<[i32; 3]> = match shape {
                ONE => vec![cell(a).to_array()],
                BOX => {
                    let (low, high) = (cell(a.min(b)), cell(a.max(b)));
                    let count = ((high - low + 1).as_i64vec3().element_product()).max(0);
                    if count > 4_000_000 {
                        tracing::warn!(target: "script", "Blocks.Fill: {count} blocks are too many at once");
                        continue;
                    }
                    (low.z..=high.z)
                        .flat_map(|z| (low.y..=high.y).flat_map(move |y| (low.x..=high.x).map(move |x| [x, y, z])))
                        .collect()
                }
                BALL => {
                    let radius = b.x.max(0.0);
                    let (low, high) = (cell(a - Vec3::splat(radius)), cell(a + Vec3::splat(radius)));
                    let mut inside = Vec::new();
                    for z in low.z..=high.z {
                        for y in low.y..=high.y {
                            for x in low.x..=high.x {
                                let middle = corner + (Vec3::new(x as f32, y as f32, z as f32) + 0.5) * size;
                                if middle.distance(a) <= radius {
                                    inside.push([x, y, z]);
                                }
                            }
                        }
                    }
                    inside
                }
                _ => continue,
            };
            let changes: Vec<([i32; 3], BlockId)> = reached
                .into_iter()
                .filter(|&at| {
                    let now = blocks.blocks.get(at);
                    match only {
                        AIR_ONLY => now == AIR,
                        BLOCKS_ONLY => now != AIR,
                        ALL => true,
                        _ => true,
                    }
                })
                .map(|at| (at, block))
                .collect();
            if changes.is_empty() {
                continue;
            }
            // Kept as built from the first change, so a save holds only what changed since.
            if !world.has::<BuiltBlocks>(entity) {
                world.insert(entity, BuiltBlocks(blocks.blocks.clone()));
            }
            let count = blocks.set_blocks(changes);
            if count > 0 {
                world.insert(entity, blocks);
                changed += count as i32;
            }
        }
        Some(changed)
    })
    .unwrap_or(0)
}

/// The first block a ray from `origin` along `direction` meets in any world
/// of blocks within `distance` metres; 0 where it meets none.
pub(crate) unsafe extern "C" fn block_raycast(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    distance: f32,
    hit: *mut BlockHitData,
) -> i32 {
    if origin.is_null() || direction.is_null() || hit.is_null() || distance.is_nan() || distance <= 0.0 {
        return 0;
    }
    let (origin, direction) = unsafe { (Vec3::from(*origin), Vec3::from(*direction).normalize_or_zero()) };
    if direction == Vec3::ZERO {
        return 0;
    }
    let found = with_world(|world| {
        worlds(world)
            .into_iter()
            .filter_map(|(entity, corner)| {
                let blocks = world.get::<BlockWorld>(entity)?;
                let size = blocks.block_size;
                let from = ((origin - corner) / size).to_array();
                let met = voxear_terrain::blocks::raycast(&blocks.blocks, from, direction.to_array(), distance / size)?;
                let block = blocks.blocks.get(met.block);
                Some((entity, corner, size, met, block))
            })
            .min_by(|a, b| (a.3.distance * a.2).total_cmp(&(b.3.distance * b.2)))
    });
    let Some((entity, corner, size, met, block)) = found else { return 0 };
    let along = met.distance * size;
    let middle = corner + (Vec3::from(met.block.map(|v| v as f32)) + 0.5) * size;
    unsafe {
        *hit = BlockHitData {
            obj: entity.to_bits(),
            point: (origin + direction * along).to_array(),
            normal: met.normal.map(|v| v as f32),
            middle: middle.to_array(),
            distance: along,
            block_size: size,
            block: u32::from(block),
        };
    }
    1
}

/// Every world of blocks scripts changed, by its Obj's key, and its changes
/// since its level built it.
fn capture(world: &World) -> Vec<u8> {
    let mut kept: Vec<(String, Vec<u8>)> = world
        .query_ref::<(Entity, &BlockWorld, &BuiltBlocks)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, blocks, built)| {
            let key = with_services(|services| services.obj_key(world, entity)).flatten()?;
            Some((key, BlockEdits::between(&built.0, &blocks.blocks).to_bytes()))
        })
        .collect();
    kept.sort();
    let mut out = (kept.len() as u32).to_le_bytes().to_vec();
    for (key, edits) in kept {
        out.extend_from_slice(&(key.len() as u16).to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(edits.len() as u32).to_le_bytes());
        out.extend_from_slice(&edits);
    }
    out
}

/// Puts each world of blocks `bytes` (from [`capture`]) names back as it
/// was built, then makes its changes; how many it found.
fn restore(world: &mut World, bytes: &[u8]) -> anyhow::Result<i32> {
    use anyhow::Context;
    let take = |at: usize, len: usize| bytes.get(at..at + len).context("block changes end early");
    let count = u32::from_le_bytes(take(0, 4)?.try_into()?);
    let mut at = 4;
    let mut found = 0;
    for _ in 0..count {
        let key_len = usize::from(u16::from_le_bytes(take(at, 2)?.try_into()?));
        let key = std::str::from_utf8(take(at + 2, key_len)?)?.to_string();
        at += 2 + key_len;
        let len = u32::from_le_bytes(take(at, 4)?.try_into()?) as usize;
        let edits = BlockEdits::from_bytes(take(at + 4, len)?).with_context(|| format!("the blocks of {key}"))?;
        at += 4 + len;
        let Some(entity) = with_services(|services| services.obj_by_key(world, &key)).flatten() else { continue };
        let Some(now) = world.get::<BlockWorld>(entity).cloned() else { continue };
        let built = world.get::<BuiltBlocks>(entity).map_or_else(|| now.blocks.clone(), |built| built.0.clone());
        let mut blocks = (*built).clone();
        edits.apply(&mut blocks);
        let mut remade = BlockWorld::made(blocks, &now.faces, now.block_size, now.smooth);
        (remade.cast_shadows, remade.planet) = (now.cast_shadows, now.planet);
        world.insert(entity, remade);
        world.insert(entity, BuiltBlocks(built));
        found += 1;
    }
    Ok(found)
}

/// `Blocks.CaptureEdits` (`what` 0): every changed world's changes into
/// `out`, their length returned, written when it fits. `Blocks.ApplyEdits`
/// (`what` 1): puts the changes in `data` back, returning how many worlds it
/// found, or -1 with an error logged for bytes that are not changes.
pub(crate) unsafe extern "C" fn block_edits(what: i32, data: *const u8, len: i32, out: *mut u8, capacity: i32) -> i32 {
    match what {
        0 => {
            let bytes = with_world(|world| Some(capture(world))).unwrap_or_else(|| 0u32.to_le_bytes().to_vec());
            if !out.is_null() && bytes.len() <= capacity.max(0) as usize {
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
            }
            bytes.len() as i32
        }
        1 if !data.is_null() && len >= 0 => {
            let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) };
            match with_world(|world| Some(restore(world, bytes))) {
                Some(Ok(found)) => found,
                Some(Err(error)) => {
                    tracing::error!("Blocks.ApplyEdits: {error:#}");
                    -1
                }
                None => 0,
            }
        }
        _ => -1,
    }
}
