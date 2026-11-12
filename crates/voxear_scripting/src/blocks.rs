//! Worlds of blocks for C#: `Blocks` in Voxear.Core asks which block is at
//! a point and where a ray meets one, and changes them: a block, a box or a
//! ball of them set to a block or to air, every world they reach, each
//! chunk changed made again (see `voxear_terrain::blocks`). What scripts
//! changed is kept for a save of the scene or a player joining a game, as
//! only what differs from the world as its level built it.
//!
//! A block is named by its `.block` asset, which is the same block in
//! every world, or by its number in one world. A world numbers a block the
//! first time it holds one (`ScriptServices::block_number`). In a game,
//! nothing writes that down in the world's file, so such a number is this
//! machine's own: the changes kept say which block each one is, after the
//! changes themselves, and are matched to the numbers of the game they are
//! put back in.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld, BuiltBlocks,
    chunk::{AIR, BlockId},
    edits::BlockEdits,
    flow::Water,
    is_water,
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

/// Which blocks `block_set` changes: all it reaches, only air, only blocks
/// (to paint or to break), air and liquids (to place: a block takes a
/// liquid's place), or only liquids at rest (to take one up).
const ALL: i32 = 0;
const AIR_ONLY: i32 = 1;
const BLOCKS_ONLY: i32 = 2;
const AIR_AND_LIQUIDS: i32 = 3;
const LIQUID_SOURCES: i32 = 4;

/// The block `block_set` sets: by its number, the same in every world, or
/// by the `.block` it is, numbered in each world as that world has it.
enum Picked<'a> {
    Number(BlockId),
    Named(&'a str),
}

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
    set(shape, a, b, Picked::Number(block as BlockId), only)
}

/// As [`block_set`], with the block named by its `.block`, a project path
/// or GUID: in each world the number that world has it by, one given it
/// there now where the world has not held it.
pub(crate) unsafe extern "C" fn block_set_named(
    shape: i32,
    a: *const [f32; 3],
    b: *const [f32; 3],
    block: *const u8,
    len: i32,
    only: i32,
) -> i32 {
    if a.is_null() || (shape != ONE && b.is_null()) || block.is_null() || len <= 0 {
        return 0;
    }
    let (a, b) = (Vec3::from(unsafe { *a }), if b.is_null() { Vec3::ZERO } else { Vec3::from(unsafe { *b }) });
    let named = unsafe { crate::interop::utf8(block, len) };
    set(shape, a, b, Picked::Named(&named), only)
}

fn set(shape: i32, a: Vec3, b: Vec3, picked: Picked, only: i32) -> i32 {
    with_world(|world| {
        let mut changed = 0;
        for (entity, corner) in worlds(world) {
            // Its number here first: a world numbering the block now takes its faces on.
            let block = match picked {
                Picked::Number(block) => block,
                Picked::Named(named) => {
                    let numbered = with_services(|services| services.block_number(world, entity, named, true));
                    let Some(block) = numbered.flatten() else { continue };
                    block
                }
            };
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
                        AIR_AND_LIQUIDS => now == AIR || is_water(&blocks.faces, now),
                        LIQUID_SOURCES => {
                            is_water(&blocks.faces, now) && Water::of(blocks.blocks.fill(at)) == Water::Source
                        }
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

/// `block_raycast`'s flags: whether the ray meets liquids, rather than
/// passing through them to what lies behind.
const MEET_LIQUIDS: i32 = 1;

/// The first block a ray from `origin` along `direction` meets in any world
/// of blocks within `distance` metres; 0 where it meets none. It passes
/// through liquids unless `flags` say to meet them.
pub(crate) unsafe extern "C" fn block_raycast(
    origin: *const [f32; 3],
    direction: *const [f32; 3],
    distance: f32,
    flags: i32,
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
                let liquids = flags & MEET_LIQUIDS != 0;
                let meets = |block: BlockId| block != AIR && (liquids || !is_water(&blocks.faces, block));
                let reach = distance / size;
                let met =
                    voxear_terrain::blocks::raycast_where(&blocks.blocks, from, direction.to_array(), reach, &meets)?;
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

/// The GUID of the `.block` numbered `block` in the world of blocks on
/// `obj`, written into `buffer` when it fits: its length, or 0 for air, a
/// kind the world keeps itself, or no such world.
pub(crate) unsafe extern "C" fn block_of(obj: u64, block: i32, buffer: *mut u8, capacity: i32) -> i32 {
    let Ok(block) = BlockId::try_from(block) else { return 0 };
    let entity = Entity::from_bits(obj);
    let guid = with_world(|world| with_services(|services| services.block_guid(world, entity, block)).flatten());
    let Some(guid) = guid else { return 0 };
    if !buffer.is_null() && guid.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(guid.as_ptr(), buffer, guid.len()) };
    }
    guid.len() as i32
}

/// Every block of the project, by GUID, a line each, written into `buffer`
/// when it fits: the text's length.
pub(crate) unsafe extern "C" fn blocks_all(buffer: *mut u8, capacity: i32) -> i32 {
    let every = with_services(|services| services.block_guids()).unwrap_or_default().join("\n");
    if !buffer.is_null() && every.len() <= capacity.max(0) as usize {
        unsafe { std::ptr::copy_nonoverlapping(every.as_ptr(), buffer, every.len()) };
    }
    every.len() as i32
}

/// Goes up whenever a block is saved (see `ScriptServices::blocks_version`).
pub(crate) unsafe extern "C" fn blocks_version() -> u64 {
    with_services(|services| services.blocks_version()).unwrap_or(0)
}

/// What follows the changes in the bytes `capture` gives, where a world
/// numbered blocks since its file was read: this mark, a version, how many
/// worlds, and for each its key and the blocks it numbered, each number
/// with its block's GUID. Bytes without it, as they were kept before blocks
/// were assets of their own, read as they did.
const NUMBERED: &[u8; 4] = b"VXBT";
const NUMBERED_VERSION: u32 = 1;

/// A world's changes as they are kept: its Obj's key, the changes since its
/// level built it, and the blocks it numbered since.
struct Kept {
    key: String,
    edits: Vec<u8>,
    numbered: Vec<(BlockId, String)>,
}

/// `kept` as bytes: how many worlds, each one's key and changes, and then,
/// where any numbered a block, which block each number is.
fn to_bytes(kept: &[Kept]) -> Vec<u8> {
    let mut out = (kept.len() as u32).to_le_bytes().to_vec();
    for world in kept {
        out.extend_from_slice(&(world.key.len() as u16).to_le_bytes());
        out.extend_from_slice(world.key.as_bytes());
        out.extend_from_slice(&(world.edits.len() as u32).to_le_bytes());
        out.extend_from_slice(&world.edits);
    }
    let numbering: Vec<&Kept> = kept.iter().filter(|world| !world.numbered.is_empty()).collect();
    if numbering.is_empty() {
        return out;
    }
    out.extend_from_slice(NUMBERED);
    out.extend_from_slice(&NUMBERED_VERSION.to_le_bytes());
    out.extend_from_slice(&(numbering.len() as u32).to_le_bytes());
    for world in numbering {
        out.extend_from_slice(&(world.key.len() as u16).to_le_bytes());
        out.extend_from_slice(world.key.as_bytes());
        out.extend_from_slice(&(world.numbered.len() as u16).to_le_bytes());
        for (number, guid) in &world.numbered {
            out.extend_from_slice(&number.to_le_bytes());
            out.extend_from_slice(&(guid.len() as u16).to_le_bytes());
            out.extend_from_slice(guid.as_bytes());
        }
    }
    out
}

/// The worlds `bytes` keep (see [`to_bytes`]).
fn from_bytes(bytes: &[u8]) -> anyhow::Result<Vec<Kept>> {
    use anyhow::Context;
    let take = |at: usize, len: usize| bytes.get(at..at + len).context("block changes end early");
    let short = |at: usize| anyhow::Ok(usize::from(u16::from_le_bytes(take(at, 2)?.try_into()?)));
    let text = |at: usize, len: usize| anyhow::Ok(std::str::from_utf8(take(at, len)?)?.to_string());
    let count = u32::from_le_bytes(take(0, 4)?.try_into()?);
    let mut at = 4;
    let mut kept = Vec::new();
    for _ in 0..count {
        let key_len = short(at)?;
        let key = text(at + 2, key_len)?;
        at += 2 + key_len;
        let len = u32::from_le_bytes(take(at, 4)?.try_into()?) as usize;
        kept.push(Kept { key, edits: take(at + 4, len)?.to_vec(), numbered: Vec::new() });
        at += 4 + len;
    }
    if bytes.get(at..at + 4) != Some(NUMBERED.as_slice()) {
        return Ok(kept);
    }
    let version = u32::from_le_bytes(take(at + 4, 4)?.try_into()?);
    let known = version == NUMBERED_VERSION;
    anyhow::ensure!(known, "block changes from a newer Voxear (their blocks' numbers, version {version})");
    let worlds = u32::from_le_bytes(take(at + 8, 4)?.try_into()?);
    at += 12;
    for _ in 0..worlds {
        let key_len = short(at)?;
        let key = text(at + 2, key_len)?;
        at += 2 + key_len;
        let blocks = short(at)?;
        at += 2;
        let mut numbered = Vec::with_capacity(blocks);
        for _ in 0..blocks {
            let number = u16::from_le_bytes(take(at, 2)?.try_into()?);
            let guid_len = short(at + 2)?;
            numbered.push((number, text(at + 4, guid_len)?));
            at += 4 + guid_len;
        }
        if let Some(world) = kept.iter_mut().find(|world| world.key == key) {
            world.numbered = numbered;
        }
    }
    Ok(kept)
}

/// Every world of blocks scripts changed, by its Obj's key: its changes
/// since its level built it, and the blocks it numbered since.
fn capture(world: &World) -> Vec<u8> {
    let mut kept: Vec<Kept> = world
        .query_ref::<(Entity, &BlockWorld, &BuiltBlocks)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, blocks, built)| {
            let key = with_services(|services| services.obj_key(world, entity)).flatten()?;
            let numbered = with_services(|services| services.blocks_grown(world, entity)).unwrap_or_default();
            Some(Kept { key, edits: BlockEdits::between(&built.0, &blocks.blocks).to_bytes(), numbered })
        })
        .collect();
    kept.sort_by(|a, b| a.key.cmp(&b.key));
    to_bytes(&kept)
}

/// Puts each world of blocks `bytes` (from [`capture`]) names back as it
/// was built, then makes its changes; how many it found. A block the
/// changes numbered is the number this game has it by, one given it now
/// where the world has none yet; a number of no block here, as of one taken
/// out of the world since, is air.
fn restore(world: &mut World, bytes: &[u8]) -> anyhow::Result<i32> {
    use anyhow::Context;
    let mut found = 0;
    for kept in from_bytes(bytes)? {
        let mut edits =
            BlockEdits::from_bytes(&kept.edits).with_context(|| format!("the blocks of {}", kept.key))?;
        let Some(entity) = with_services(|services| services.obj_by_key(world, &kept.key)).flatten() else { continue };
        if !world.has::<BlockWorld>(entity) {
            continue;
        }
        // Each number the changes gave a block, as this game numbers that block.
        let mut here: std::collections::HashMap<BlockId, BlockId> = std::collections::HashMap::new();
        for (number, guid) in &kept.numbered {
            let numbered = with_services(|services| services.block_number(world, entity, guid, true)).flatten();
            here.insert(*number, numbered.unwrap_or(AIR));
        }
        let listed = |block: BlockId| {
            block == AIR || with_services(|services| services.block_listed(world, entity, block)).unwrap_or(true)
        };
        let renumber = |block: BlockId| match here.get(&block) {
            Some(&numbered) => numbered,
            None if listed(block) => block,
            None => AIR,
        };
        edits = edits.remapped(&renumber);
        // Read once the blocks are numbered: a world numbering one takes its faces on.
        let Some(now) = world.get::<BlockWorld>(entity).cloned() else { continue };
        let built = world.get::<BuiltBlocks>(entity).map_or_else(|| now.blocks.clone(), |built| built.0.clone());
        let mut blocks = (*built).clone();
        edits.apply(&mut blocks);
        let mut remade = BlockWorld::made(blocks, &now.faces, now.block_size, now.smooth);
        remade.cast_shadows = now.cast_shadows;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_kept_before_blocks_were_numbered_read_as_they_did() {
        // As the bytes were written then: how many worlds, and each one's key and changes.
        let mut older = 2u32.to_le_bytes().to_vec();
        for (key, edits) in [("scene:1", vec![1u8, 2, 3]), ("scene:7/2", Vec::new())] {
            older.extend_from_slice(&(key.len() as u16).to_le_bytes());
            older.extend_from_slice(key.as_bytes());
            older.extend_from_slice(&(edits.len() as u32).to_le_bytes());
            older.extend_from_slice(&edits);
        }
        let kept = from_bytes(&older).unwrap();
        assert_eq!(kept.len(), 2);
        assert_eq!((kept[0].key.as_str(), kept[0].edits.as_slice()), ("scene:1", [1u8, 2, 3].as_slice()));
        assert!(kept[1].edits.is_empty() && kept.iter().all(|world| world.numbered.is_empty()));
        // Worlds that numbered no block are written just so still, so an older Voxear reads them too.
        assert_eq!(to_bytes(&kept), older);
        assert!(from_bytes(&older[..older.len() - 2]).is_err(), "cut short");
    }

    #[test]
    fn blocks_numbered_while_the_game_ran_are_kept_with_which_block_each_is() {
        let guid = "0123456789abcdef0123456789abcdef";
        let kept = vec![
            Kept { key: "scene:1".into(), edits: vec![9, 9], numbered: Vec::new() },
            Kept {
                key: "scene:4".into(),
                edits: vec![5],
                numbered: vec![(6, guid.to_string()), (7, "fedcba9876543210fedcba9876543210".to_string())],
            },
        ];
        let bytes = to_bytes(&kept);
        let read = from_bytes(&bytes).unwrap();
        assert_eq!(read.len(), 2);
        assert!(read[0].numbered.is_empty() && read[0].edits == [9, 9]);
        assert_eq!(read[1].numbered, kept[1].numbered);
        assert_eq!(read[1].edits, [5]);
        // What follows the changes is marked and has a version: one this Voxear does not know is an error, not a guess.
        let mark = bytes.windows(4).position(|window| window == NUMBERED).expect("its mark");
        let mut newer = bytes.clone();
        newer[mark + 4] = 9;
        assert!(from_bytes(&newer).is_err());
    }

    #[test]
    fn changes_put_back_take_this_games_numbers_for_their_blocks_and_air_for_blocks_gone() {
        use voxear_terrain::blocks::{BlockFaces, BlockMap};
        // This game's world: planks numbered 5 here, the block once numbered 2 taken out of it since.
        struct Numbers(Entity);
        impl crate::services::ScriptServices for Numbers {
            fn obj_by_key(&self, _world: &World, key: &str) -> Option<Entity> {
                (key == "scene:4").then_some(self.0)
            }
            fn block_number(&mut self, _world: &mut World, _entity: Entity, block: &str, _grow: bool) -> Option<u16> {
                (block == "planks").then_some(5)
            }
            fn block_listed(&mut self, _world: &World, _entity: Entity, block: u16) -> bool {
                block <= 5 && block != 2
            }
        }
        let mut built = BlockMap::default();
        for x in 0..4 {
            built.set([x, 0, 0], 1);
        }
        let faces = vec![BlockFaces::new(0, 0, 0); 6];
        let mut world = World::new();
        let entity = world.spawn((BlockWorld::new(built.clone(), &faces, 1.0), Transform::default()));
        // Changes as another game kept them: planks numbered 4 there, and a block numbered 9 there that is none here.
        let mut changed = built.clone();
        for (x, block) in [(0, 4), (1, 2), (2, 9), (3, 3)] {
            changed.set([x, 1, 0], block);
        }
        let kept = vec![Kept {
            key: "scene:4".into(),
            edits: BlockEdits::between(&built, &changed).to_bytes(),
            numbered: vec![(4, "planks".into()), (9, "lost".into())],
        }];
        let mut services = Numbers(entity);
        let _scope = crate::services::ServicesScope::enter(&mut services);
        assert_eq!(restore(&mut world, &to_bytes(&kept)).unwrap(), 1);
        let now = world.get::<BlockWorld>(entity).unwrap();
        assert_eq!([0, 1, 2, 3].map(|x| now.blocks.get([x, 1, 0])), [5, AIR, AIR, 3]);
        assert_eq!(now.blocks.get([2, 0, 0]), 1, "its ground as it was built");
    }
}
