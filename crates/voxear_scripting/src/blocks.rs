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
use voxear_scene::{CutPlants, Destroying, Transform, WorldTransform};
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
    /// By the `.block` it is, placed in a state (see
    /// `voxear_terrain::blocks::shape::BlockState`).
    Placed(&'a str, u8),
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
                Picked::Placed(named, state) => {
                    let numbered =
                        with_services(|services| services.block_number_state(world, entity, named, state, true));
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
            // One block put down or taken away takes a door's other block with it: numbered first, so read again.
            let changes = match shape {
                ONE => {
                    let halves = with_services(|services| services.door_halves(world, entity, changes.clone()));
                    let Some(now) = world.get::<BlockWorld>(entity).cloned() else { continue };
                    blocks = now;
                    halves.unwrap_or(changes)
                }
                _ => changes,
            };
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

/// The first world of blocks with a block at `at`, a point in metres, and
/// where that block is in it.
fn block_place(world: &World, at: Vec3) -> Option<(Entity, [i32; 3])> {
    worlds(world).into_iter().find_map(|(entity, corner)| {
        let blocks = world.get::<BlockWorld>(entity)?;
        let (place, block) = blocks.block_at((at - corner).to_array());
        (block != AIR).then_some((entity, place))
    })
}

/// Where the point `at`, in metres, is among the blocks of the world of
/// blocks on `entity`.
fn place_in(world: &World, entity: Entity, at: Vec3) -> Option<[i32; 3]> {
    let (_, corner) = worlds(world).into_iter().find(|(e, _)| *e == entity)?;
    Some(world.get::<BlockWorld>(entity)?.block_at((at - corner).to_array()).0)
}

/// `Blocks.Use`: uses the block at `at`, a point in metres, in the first
/// world of blocks with one there (see `voxear_terrain::blocks::usable`):
/// 0 where it is no block a player uses, else what kind of use it was, by
/// `UseKind`'s place, with 16 over it where it is open (or on) now.
pub(crate) unsafe extern "C" fn block_use(at: *const [f32; 3]) -> i32 {
    if at.is_null() {
        return 0;
    }
    let at = Vec3::from(unsafe { *at });
    with_world(|world| {
        let (entity, place) = block_place(world, at)?;
        if !world.has::<BuiltBlocks>(entity) {
            let built = world.get::<BlockWorld>(entity)?.blocks.clone();
            world.insert(entity, BuiltBlocks(built));
        }
        let (kind, now) = with_services(|services| services.block_use(world, entity, place)).flatten()?;
        Some(i32::from(kind) | i32::from(now) << 4)
    })
    .unwrap_or(0)
}

/// `Blocks.StateAt`: the state the block at `at`, a point in metres, is
/// placed in, as a byte; -1 for air everywhere.
pub(crate) unsafe extern "C" fn block_state(at: *const [f32; 3]) -> i32 {
    if at.is_null() {
        return -1;
    }
    let at = Vec3::from(unsafe { *at });
    with_world(|world| {
        let (entity, place) = block_place(world, at)?;
        let blocks = world.get::<BlockWorld>(entity)?;
        Some(i32::from(blocks.faces.get(usize::from(blocks.blocks.get(place)))?.state))
    })
    .unwrap_or(-1)
}

/// `Blocks.SetState`: the block at `at`, a point in metres, put in `state`
/// (a byte), moving there by its motion where `animate` is not 0; how many
/// blocks changed.
pub(crate) unsafe extern "C" fn block_set_state(at: *const [f32; 3], state: i32, animate: i32) -> i32 {
    if at.is_null() || !(0..=255).contains(&state) {
        return 0;
    }
    let at = Vec3::from(unsafe { *at });
    with_world(|world| {
        let (entity, place) = block_place(world, at)?;
        if !world.has::<BuiltBlocks>(entity) {
            let built = world.get::<BlockWorld>(entity)?.blocks.clone();
            world.insert(entity, BuiltBlocks(built));
        }
        let changed = with_services(|services| {
            services.block_change_state(world, entity, place, state as u8, animate != 0)
        });
        Some(changed.unwrap_or(0) as i32)
    })
    .unwrap_or(0)
}

/// `Blocks.Set` with a state: the block named by its `.block` (a project
/// path or GUID) put at `at`, a point in metres, placed in `state` (a
/// byte), in every world of blocks it falls in; how many blocks changed.
pub(crate) unsafe extern "C" fn block_set_placed(at: *const [f32; 3], block: *const u8, len: i32, state: i32) -> i32 {
    if at.is_null() || block.is_null() || len <= 0 || !(0..=255).contains(&state) {
        return 0;
    }
    let at = Vec3::from(unsafe { *at });
    let named = unsafe { crate::interop::utf8(block, len) };
    set(ONE, at, Vec3::ZERO, Picked::Placed(&named, state as u8), ALL)
}

/// The state a block of shape `shape` (its place in `BlockShape::ALL`) is
/// placed in against a face looking `normal`, put `within` (0 to 1 across
/// the block it goes in) by someone looking `look`, as a byte (see
/// `voxear_terrain::blocks::shape::placed_state`).
pub(crate) unsafe extern "C" fn block_placed_state(
    shape: i32,
    within: *const [f32; 3],
    normal: *const [f32; 3],
    look: *const [f32; 3],
) -> i32 {
    use voxear_terrain::blocks::{block::BlockShape, shape::placed_state};
    let Some(&shape) = usize::try_from(shape).ok().and_then(|at| BlockShape::ALL.get(at)) else { return 0 };
    if within.is_null() || normal.is_null() || look.is_null() {
        return 0;
    }
    let (within, normal, look) = unsafe { (*within, *normal, *look) };
    let normal = normal.map(|v| v.round() as i32);
    i32::from(placed_state(shape, normal, within, look).to_byte())
}

/// `Blocks.Link` (`what` 0): links the lever or button at `from` to the
/// block at `to`, both points in metres in one world of blocks;
/// `Blocks.Unlink` (`what` 1) takes that link away, and with `what` 2 every
/// link from `from`. 1 where something changed, 0 where nothing did.
pub(crate) unsafe extern "C" fn block_link(what: i32, from: *const [f32; 3], to: *const [f32; 3]) -> i32 {
    if from.is_null() || (what != 2 && to.is_null()) {
        return 0;
    }
    let from = Vec3::from(unsafe { *from });
    let to = if to.is_null() { None } else { Some(Vec3::from(unsafe { *to })) };
    with_world(|world| {
        let (entity, start) = block_place(world, from)?;
        let end = match to {
            Some(to) => Some(place_in(world, entity, to)?),
            None => None,
        };
        if !world.has::<BuiltBlocks>(entity) {
            let built = world.get::<BlockWorld>(entity)?.blocks.clone();
            world.insert(entity, BuiltBlocks(built));
        }
        let blocks = world.get_mut::<BlockWorld>(entity)?;
        let changed = match (what, end) {
            (0, Some(end)) => blocks.link(start, end),
            (1, Some(end)) => blocks.unlink(start, Some(end)),
            (2, _) => blocks.unlink(start, None),
            _ => false,
        };
        Some(i32::from(changed))
    })
    .unwrap_or(0)
}

/// `Blocks.LinksFrom`: the middles, in metres, of the blocks the lever or
/// button at `from` drives, written into `out` as many as fit in
/// `capacity`: how many there are.
pub(crate) unsafe extern "C" fn block_links(from: *const [f32; 3], out: *mut [f32; 3], capacity: i32) -> i32 {
    if from.is_null() {
        return 0;
    }
    let from = Vec3::from(unsafe { *from });
    let found = with_world(|world| {
        let (entity, start) = block_place(world, from)?;
        let (_, corner) = worlds(world).into_iter().find(|(e, _)| *e == entity)?;
        let blocks = world.get::<BlockWorld>(entity)?;
        let size = blocks.block_size;
        let middle = |at: &[i32; 3]| (corner + (Vec3::from(at.map(|v| v as f32)) + 0.5) * size).to_array();
        Some(blocks.links.get(&start).map(|all| all.iter().map(middle).collect::<Vec<_>>()).unwrap_or_default())
    })
    .unwrap_or_default();
    if !out.is_null() {
        for (at, middle) in found.iter().take(capacity.max(0) as usize).enumerate() {
            unsafe { *out.add(at) = *middle };
        }
    }
    found.len() as i32
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

/// `World.Quality` and the numbers under it, by `which`: the preset by its
/// place in `WorldQuality::ALL`, the threads, the chunk columns made at
/// once, the megabytes uploaded a frame, and the view and far distances
/// (see `voxear_terrain::WorldBudget`). NaN for any other.
pub(crate) unsafe extern "C" fn world_budget(which: i32) -> f32 {
    use voxear_terrain::{WorldBudget, WorldQuality};
    let read = |world: &mut World| {
        let budget = WorldBudget::of(world);
        Some(match which {
            0 => WorldQuality::ALL.iter().position(|quality| *quality == budget.quality)? as f32,
            1 => budget.threads as f32,
            2 => budget.columns as f32,
            3 => budget.upload as f32,
            4 => budget.view_distance,
            5 => budget.far_distance,
            _ => return None,
        })
    };
    with_world(read).unwrap_or(f32::NAN)
}

/// Sets `World.Quality` or a number under it, as [`world_budget`] numbers
/// them: a preset brings its numbers, and a number makes it Custom. Taken
/// up at once, with nothing made again.
pub(crate) unsafe extern "C" fn set_world_budget(which: i32, value: f32) {
    use voxear_terrain::{WorldBudget, WorldQuality};
    if !value.is_finite() {
        return;
    }
    with_world(|world| {
        let custom = WorldBudget { quality: WorldQuality::Custom, ..WorldBudget::of(world) };
        let whole = value.max(0.0).round() as u32;
        let budget = match which {
            0 => WorldQuality::ALL.get(value.max(0.0) as usize)?.budget().unwrap_or(custom),
            1 => WorldBudget { threads: whole, ..custom },
            2 => WorldBudget { columns: whole, ..custom },
            3 => WorldBudget { upload: whole, ..custom },
            4 => WorldBudget { view_distance: value, ..custom },
            5 => WorldBudget { far_distance: value, ..custom },
            _ => return None,
        };
        budget.put(world);
        Some(())
    });
}

/// What follows the changes in the bytes `capture` gives, where a world
/// numbered blocks since its file was read: this mark, a version, how many
/// worlds, and for each its key and the blocks it numbered, each number
/// with its block's GUID. Bytes without it, as they were kept before blocks
/// were assets of their own, read as they did.
///
/// Version 2, for a world that numbered a block in a state (see
/// `voxear_terrain::blocks::shape::BlockState`), names each block once, as
/// a door has a number for each way it stands: after its key, how many
/// blocks and each one's GUID, then how many numbers and each one with its
/// block's place among those and its state, a byte. Bytes without states
/// are written as before.
const NUMBERED: &[u8; 4] = b"VXBT";
const NUMBERED_VERSION: u32 = 2;

/// What follows those, where scripts took away plants a world of blocks
/// grew by itself (see `voxear_scene::CutPlants`): this mark, a version, how
/// many Foliage, and for each its Obj's key and its columns, each column's
/// place and the numbers of its plants taken away.
const CUT: &[u8; 4] = b"VXCP";
const CUT_VERSION: u32 = 1;

/// The plants scripts took from one Foliage's grown columns, as they are
/// kept: its Obj's key, and each column's place with its plants' numbers.
#[derive(Clone, Debug, PartialEq)]
struct KeptCuts {
    key: String,
    columns: Vec<([i32; 2], Vec<u32>)>,
}

/// A world's changes as they are kept: its Obj's key, the changes since its
/// level built it, and the blocks it numbered since.
struct Kept {
    key: String,
    edits: Vec<u8>,
    numbered: Vec<(BlockId, String)>,
    /// The state each block numbered is placed in, in the same order; none
    /// for blocks all in their first state.
    states: Vec<u8>,
}

/// `kept` as bytes: how many worlds, each one's key and changes, and then,
/// where any numbered a block, which block each number is.
fn to_bytes(kept: &[Kept]) -> Vec<u8> {
    to_bytes_with(kept, &[])
}

/// As [`to_bytes`], and after them the plants `cuts` took away, where any.
fn to_bytes_with(kept: &[Kept], cuts: &[KeptCuts]) -> Vec<u8> {
    let mut out = worlds_bytes(kept);
    if cuts.is_empty() {
        return out;
    }
    out.extend_from_slice(CUT);
    out.extend_from_slice(&CUT_VERSION.to_le_bytes());
    out.extend_from_slice(&(cuts.len() as u32).to_le_bytes());
    for foliage in cuts {
        out.extend_from_slice(&(foliage.key.len() as u16).to_le_bytes());
        out.extend_from_slice(foliage.key.as_bytes());
        out.extend_from_slice(&(foliage.columns.len() as u32).to_le_bytes());
        for ([x, z], numbers) in &foliage.columns {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&z.to_le_bytes());
            out.extend_from_slice(&(numbers.len() as u32).to_le_bytes());
            numbers.iter().for_each(|number| out.extend_from_slice(&number.to_le_bytes()));
        }
    }
    out
}

/// The worlds' part of [`to_bytes`].
fn worlds_bytes(kept: &[Kept]) -> Vec<u8> {
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
    // The first version where no block is numbered in a state, so an older Voxear reads them.
    let stated = numbering.iter().any(|world| world.states.iter().any(|&state| state != 0));
    out.extend_from_slice(NUMBERED);
    out.extend_from_slice(&(if stated { NUMBERED_VERSION } else { 1 }).to_le_bytes());
    out.extend_from_slice(&(numbering.len() as u32).to_le_bytes());
    for world in numbering {
        out.extend_from_slice(&(world.key.len() as u16).to_le_bytes());
        out.extend_from_slice(world.key.as_bytes());
        if stated {
            // Each block once, then each number by its block's place among them.
            let mut guids: Vec<&str> = Vec::new();
            for (_, guid) in &world.numbered {
                if !guids.contains(&guid.as_str()) {
                    guids.push(guid);
                }
            }
            out.extend_from_slice(&(guids.len() as u16).to_le_bytes());
            for guid in &guids {
                out.extend_from_slice(&(guid.len() as u16).to_le_bytes());
                out.extend_from_slice(guid.as_bytes());
            }
            out.extend_from_slice(&(world.numbered.len() as u16).to_le_bytes());
            for (at, (number, guid)) in world.numbered.iter().enumerate() {
                let place = guids.iter().position(|kept| kept == guid).unwrap_or(0) as u16;
                out.extend_from_slice(&number.to_le_bytes());
                out.extend_from_slice(&place.to_le_bytes());
                out.push(world.states.get(at).copied().unwrap_or(0));
            }
            continue;
        }
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
    Ok(read_bytes(bytes)?.0)
}

/// The plants taken away that follow the worlds from `at` in `bytes`, where
/// any do.
fn cuts_from(bytes: &[u8], mut at: usize) -> anyhow::Result<Vec<KeptCuts>> {
    use anyhow::Context;
    if bytes.get(at..at + 4) != Some(CUT.as_slice()) {
        return Ok(Vec::new());
    }
    let take = |at: usize, len: usize| bytes.get(at..at + len).context("plants taken away end early");
    let whole = |at: usize| anyhow::Ok(u32::from_le_bytes(take(at, 4)?.try_into()?));
    let version = whole(at + 4)?;
    anyhow::ensure!(version == CUT_VERSION, "block changes from a newer Voxear (plants taken away, version {version})");
    let count = whole(at + 8)?;
    at += 12;
    let mut cuts = Vec::new();
    for _ in 0..count {
        let key_len = usize::from(u16::from_le_bytes(take(at, 2)?.try_into()?));
        let key = std::str::from_utf8(take(at + 2, key_len)?)?.to_string();
        at += 2 + key_len;
        let columns = whole(at)?;
        at += 4;
        let mut kept = Vec::new();
        for _ in 0..columns {
            let column = [whole(at)? as i32, whole(at + 4)? as i32];
            let numbers = whole(at + 8)? as usize;
            at += 12;
            anyhow::ensure!(numbers <= bytes.len() / 4, "plants taken away end early");
            kept.push((column, (0..numbers).map(|n| whole(at + n * 4)).collect::<anyhow::Result<Vec<u32>>>()?));
            at += numbers * 4;
        }
        cuts.push(KeptCuts { key, columns: kept });
    }
    Ok(cuts)
}

/// The worlds `bytes` keep, and the plants taken away after them.
fn read_bytes(bytes: &[u8]) -> anyhow::Result<(Vec<Kept>, Vec<KeptCuts>)> {
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
        kept.push(Kept { key, edits: take(at + 4, len)?.to_vec(), numbered: Vec::new(), states: Vec::new() });
        at += 4 + len;
    }
    if bytes.get(at..at + 4) != Some(NUMBERED.as_slice()) {
        let cuts = cuts_from(bytes, at)?;
        return Ok((kept, cuts));
    }
    let version = u32::from_le_bytes(take(at + 4, 4)?.try_into()?);
    let known = (1..=NUMBERED_VERSION).contains(&version);
    anyhow::ensure!(known, "block changes from a newer Voxear (their blocks' numbers, version {version})");
    let worlds = u32::from_le_bytes(take(at + 8, 4)?.try_into()?);
    at += 12;
    for _ in 0..worlds {
        let key_len = short(at)?;
        let key = text(at + 2, key_len)?;
        at += 2 + key_len;
        let mut numbered = Vec::new();
        let mut states = Vec::new();
        if version >= 2 {
            let count = short(at)?;
            at += 2;
            let mut guids = Vec::with_capacity(count);
            for _ in 0..count {
                let guid_len = short(at)?;
                guids.push(text(at + 2, guid_len)?);
                at += 2 + guid_len;
            }
            let numbers = short(at)?;
            at += 2;
            for _ in 0..numbers {
                let number = u16::from_le_bytes(take(at, 2)?.try_into()?);
                let guid = guids.get(short(at + 2)?).context("a number of a block not named")?.clone();
                numbered.push((number, guid));
                states.push(take(at + 4, 1)?[0]);
                at += 5;
            }
        } else {
            let blocks = short(at)?;
            at += 2;
            for _ in 0..blocks {
                let number = u16::from_le_bytes(take(at, 2)?.try_into()?);
                let guid_len = short(at + 2)?;
                numbered.push((number, text(at + 4, guid_len)?));
                at += 4 + guid_len;
            }
        }
        if let Some(world) = kept.iter_mut().find(|world| world.key == key) {
            (world.numbered, world.states) = (numbered, states);
        }
    }
    let cuts = cuts_from(bytes, at)?;
    Ok((kept, cuts))
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
            // The state each is placed in, which is part of its number.
            let states = numbered.iter().map(|(number, _)| blocks.faces.get(usize::from(*number)).map_or(0, |f| f.state));
            let states: Vec<u8> = states.collect();
            let states = if states.iter().all(|&state| state == 0) { Vec::new() } else { states };
            let mut edits = BlockEdits::between(&built.0, &blocks.blocks);
            edits.links = blocks.link_list();
            Some(Kept { key, edits: edits.to_bytes(), numbered, states })
        })
        .collect();
    kept.sort_by(|a, b| a.key.cmp(&b.key));
    // The plants scripts took from what a world grew by itself, by their Foliage's key.
    let mut cuts: Vec<KeptCuts> = world
        .query_ref::<(Entity, &CutPlants)>()
        .without::<Destroying>()
        .into_iter()
        .filter_map(|(entity, cut)| {
            let columns: Vec<([i32; 2], Vec<u32>)> =
                cut.0.iter().filter(|(_, gone)| !gone.is_empty()).map(|(at, gone)| (*at, gone.iter().collect())).collect();
            let key = with_services(|services| services.obj_key(world, entity)).flatten()?;
            (!columns.is_empty()).then_some(KeptCuts { key, columns })
        })
        .collect();
    cuts.sort_by(|a, b| a.key.cmp(&b.key));
    to_bytes_with(&kept, &cuts)
}

/// Puts each world of blocks `bytes` (from [`capture`]) names back as it
/// was built, then makes its changes; how many it found. A block the
/// changes numbered is the number this game has it by, one given it now
/// where the world has none yet; a number of no block here, as of one taken
/// out of the world since, is air.
fn restore(world: &mut World, bytes: &[u8]) -> anyhow::Result<i32> {
    use anyhow::Context;
    let mut found = 0;
    let (worlds, cuts) = read_bytes(bytes)?;
    // The plants taken away are those the bytes say, and no others: a Foliage they do not name has all of its.
    let had: Vec<Entity> = world.query_ref::<(Entity, &CutPlants)>().into_iter().map(|(entity, _)| entity).collect();
    for entity in had {
        world.remove::<CutPlants>(entity);
    }
    for foliage in cuts {
        let Some(entity) = with_services(|services| services.obj_by_key(world, &foliage.key)).flatten() else { continue };
        let mut cut = CutPlants::default();
        for (column, numbers) in foliage.columns {
            cut.take(column, &numbers.into_iter().collect());
        }
        world.insert(entity, cut);
    }
    for kept in worlds {
        let mut edits =
            BlockEdits::from_bytes(&kept.edits).with_context(|| format!("the blocks of {}", kept.key))?;
        let Some(entity) = with_services(|services| services.obj_by_key(world, &kept.key)).flatten() else { continue };
        if !world.has::<BlockWorld>(entity) {
            continue;
        }
        // Each number the changes gave a block, as this game numbers that block.
        let mut here: std::collections::HashMap<BlockId, BlockId> = std::collections::HashMap::new();
        for (at, (number, guid)) in kept.numbered.iter().enumerate() {
            let state = kept.states.get(at).copied();
            let numbered = with_services(|services| match state {
                Some(state) => services.block_number_state(world, entity, guid, state, true),
                None => services.block_number(world, entity, guid, true),
            });
            here.insert(*number, numbered.flatten().unwrap_or(AIR));
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
        // Its links as the changes say, and its clock going on as it was.
        remade.set_links(&edits.links);
        remade.clock = now.clock;
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
            Kept { key: "scene:1".into(), edits: vec![9, 9], numbered: Vec::new(), states: Vec::new() },
            Kept {
                key: "scene:4".into(),
                edits: vec![5],
                numbered: vec![(6, guid.to_string()), (7, "fedcba9876543210fedcba9876543210".to_string())],
                states: Vec::new(),
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
    fn blocks_numbered_in_states_name_each_block_once() {
        let door = "0123456789abcdef0123456789abcdef".to_string();
        let lever = "fedcba9876543210fedcba9876543210".to_string();
        let kept = vec![Kept {
            key: "scene:4".into(),
            edits: vec![5],
            numbered: vec![(6, door.clone()), (7, door.clone()), (8, door.clone()), (9, lever.clone())],
            states: vec![0, 8, 16, 37],
        }];
        let bytes = to_bytes(&kept);
        let read = from_bytes(&bytes).unwrap();
        assert_eq!((read[0].numbered.clone(), read[0].states.clone()), (kept[0].numbered.clone(), kept[0].states.clone()));
        assert!(bytes.len() < 4 * 36, "each block named once: {} bytes", bytes.len());
    }

    #[test]
    fn plants_scripts_took_from_what_a_world_grew_are_kept_with_its_changes_and_put_back() {
        // Two Foliage with keys, and one made by a script, which has none.
        struct Keys(Entity, Entity);
        impl crate::services::ScriptServices for Keys {
            fn obj_key(&self, _world: &World, entity: Entity) -> Option<String> {
                [(self.0, "scene:7"), (self.1, "scene:9")].iter().find(|(e, _)| *e == entity).map(|(_, k)| k.to_string())
            }
            fn obj_by_key(&self, _world: &World, key: &str) -> Option<Entity> {
                [(self.0, "scene:7"), (self.1, "scene:9")].iter().find(|(_, k)| *k == key).map(|(e, _)| *e)
            }
        }
        let cut = |columns: &[([i32; 2], &[u32])]| {
            let mut cut = CutPlants::default();
            for (column, numbers) in columns {
                cut.take(*column, &numbers.iter().copied().collect());
            }
            cut
        };
        let mut world = World::new();
        let oaks = world.spawn((cut(&[([3, -2], &[0, 70]), ([0, 0], &[5])]), Transform::default()));
        let pines = world.spawn((cut(&[([-1, 4], &[2])]), Transform::default()));
        let keyless = world.spawn((cut(&[([1, 1], &[1])]), Transform::default()));
        let mut services = Keys(oaks, pines);
        let _scope = crate::services::ServicesScope::enter(&mut services);
        let bytes = capture(&world);
        let (worlds, cuts) = read_bytes(&bytes).unwrap();
        assert!(worlds.is_empty());
        let oak_cuts = KeptCuts { key: "scene:7".into(), columns: vec![([0, 0], vec![5]), ([3, -2], vec![0, 70])] };
        assert_eq!(cuts, [oak_cuts, KeptCuts { key: "scene:9".into(), columns: vec![([-1, 4], vec![2])] }]);
        assert!(from_bytes(&bytes).unwrap().is_empty(), "read as changes alone, they are no worlds' changes");
        // Put back over a game that went on: what the bytes say, and nothing a Foliage lost since.
        world.insert(oaks, cut(&[([8, 8], &[1])]));
        world.remove::<CutPlants>(pines);
        assert_eq!(restore(&mut world, &bytes).unwrap(), 0);
        assert_eq!(world.get::<CutPlants>(oaks), Some(&cut(&[([3, -2], &[0, 70]), ([0, 0], &[5])])));
        assert_eq!(world.get::<CutPlants>(pines), Some(&cut(&[([-1, 4], &[2])])));
        assert!(world.get::<CutPlants>(keyless).is_none(), "one the bytes do not name has all its plants");
        // Bytes with no plants taken away give every Foliage its plants back.
        assert_eq!(restore(&mut world, &to_bytes(&[])).unwrap(), 0);
        assert!(world.get::<CutPlants>(oaks).is_none() && world.get::<CutPlants>(pines).is_none());
        // After the blocks' numbers too; a newer version is an error, and bytes cut short are.
        let numbered = vec![Kept { key: "scene:4".into(), edits: vec![5], numbered: vec![(6, "planks".into())], states: Vec::new() }];
        let both = to_bytes_with(&numbered, &cuts);
        let (worlds, read) = read_bytes(&both).unwrap();
        assert_eq!((worlds.len(), worlds[0].numbered.len(), read.len()), (1, 1, 2));
        let mark = both.windows(4).position(|window| window == CUT).expect("its mark");
        let mut newer = both.clone();
        newer[mark + 4] = 9;
        assert!(read_bytes(&newer).is_err() && read_bytes(&both[..both.len() - 3]).is_err());
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
            states: Vec::new(),
        }];
        let mut services = Numbers(entity);
        let _scope = crate::services::ServicesScope::enter(&mut services);
        assert_eq!(restore(&mut world, &to_bytes(&kept)).unwrap(), 1);
        let now = world.get::<BlockWorld>(entity).unwrap();
        assert_eq!([0, 1, 2, 3].map(|x| now.blocks.get([x, 1, 0])), [5, AIR, AIR, 3]);
        assert_eq!(now.blocks.get([2, 0, 0]), 1, "its ground as it was built");
    }
}
