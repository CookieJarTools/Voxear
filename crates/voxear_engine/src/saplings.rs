//! Saplings in a game: they take root where there is room and light, grow
//! into trees over time, and die without them (see
//! `voxear_terrain::blocks::sapling`).
//!
//! A sapling is planted by a player putting one down, by a falling tree
//! scattering them where its crown lands ([`scatter`]), or by a sapling
//! item lying on the ground a few seconds where there is room and light.
//! Each world of blocks keeps those growing in it in its `Saplings`, and a
//! sapling block found in it that is not there yet (put down by the editor,
//! or by a script) is taken up as it is first seen.
//!
//! Every second each grows on: at each stage its block, or the stage before
//! it, gives way to the next, the same tree at a larger share of its size,
//! until it stands whole and is a tree like any other. Where the next stage
//! has no room, or the light over it is less than its plant needs, it waits;
//! a sapling that has waited as long as a stage takes dies and is gone, and
//! a young tree stops growing where it stands.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld, BuiltBlocks,
    block::{BlockDef, BlockShape},
    chunk::{AIR, BlockId, CHUNK},
    is_bushy, is_ground, is_part, is_vine, is_water,
    plant::{PlantDef, PlantPart},
    sapling::{Sapling, Saplings, Shade, has_room, light_at, stage_model},
    shape::BlockState,
};

use crate::{Content, blocks::BlockWorldAsset};

/// How often saplings grow on, in seconds.
const EVERY: f32 = 1.0;

/// How long a sapling item lies on the ground before it takes root, in
/// seconds.
pub const TAKES_ROOT: f32 = 4.0;

/// The fewest seconds a sapling waits for room or light before it dies.
const FEWEST_WAITED: f32 = 30.0;

/// What saplings keep between ticks: time since they last grew, and of
/// each world, the chunks looked through for saplings as they were then.
#[derive(Default)]
struct Growing {
    since: f32,
    seen: HashMap<Entity, HashMap<[i32; 3], usize>>,
    /// Of each item lying on the ground, the sapling block it places, where
    /// it places one.
    places: HashMap<String, Option<String>>,
}

/// The `.block` the block numbered `block` in the world on `entity` is.
fn definition(world: &World, content: &Content, entity: Entity, block: BlockId) -> Option<Arc<BlockDef>> {
    let guid = world.get::<BlockWorldAsset>(entity)?.drawn.guid(block)?;
    content.loaded_block(guid)
}

/// The plant a sapling block grows into, where it is a sapling that names one.
fn plant_of(content: &mut Content, block: &BlockDef) -> Option<Arc<PlantDef>> {
    let grows = block.grows.as_deref().filter(|_| block.shape == BlockShape::Sapling)?;
    content.plant(grows).map(|(_, plant)| plant)
}

/// Whether a block of `faces` is ground a sapling takes root on: a whole
/// block, not leaves or a log, a liquid or a block of parts.
fn soil(world: &World, content: &Content, entity: Entity, id: BlockId) -> bool {
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return false };
    let faces = &blocks.faces;
    let log = definition(world, content, entity, id).is_some_and(|def| def.shape == BlockShape::Log);
    is_ground(faces, id) && !is_part(faces, id) && !is_bushy(faces, id) && !is_water(faces, id) && !log
}

/// What the block at `at` does to the light under it.
fn shade(blocks: &BlockWorld, at: [i32; 3]) -> Shade {
    let id = blocks.blocks.get(at);
    let kind = blocks.faces.get(usize::from(id)).copied().unwrap_or_default();
    match () {
        _ if id == AIR || kind.water || is_vine(&blocks.faces, id) => Shade::Open,
        _ if is_bushy(&blocks.faces, id) || kind.sways || kind.see_through => Shade::Leaf,
        _ if is_ground(&blocks.faces, id) => Shade::Ground,
        _ => Shade::Open,
    }
}

/// Whether a plant may grow into the block at `at`: air, or a vine.
fn open(blocks: &BlockWorld, at: [i32; 3]) -> bool {
    let id = blocks.blocks.get(at);
    id == AIR
        || is_vine(&blocks.faces, id) && blocks.faces.get(usize::from(id)).is_some_and(|f| f.shape == BlockShape::Vine)
}

/// Puts `changes` in the world on `entity`, kept as built first so a save
/// holds them.
fn set_blocks(world: &mut World, entity: Entity, changes: Vec<([i32; 3], BlockId)>) {
    let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { return };
    if !world.has::<BuiltBlocks>(entity) {
        world.insert(entity, BuiltBlocks(blocks.blocks.clone()));
    }
    if blocks.set_blocks(changes) > 0 {
        world.insert(entity, blocks);
    }
}

/// Keeps `sapling` among the saplings growing in the world on `entity`.
fn keep(world: &mut World, entity: Entity, sapling: Sapling) {
    let mut saplings = world.get::<Saplings>(entity).cloned().unwrap_or_default();
    saplings.list.retain(|kept| kept.at != sapling.at);
    saplings.list.push(sapling);
    world.insert(entity, saplings);
}

/// Plants the sapling `reference` names (a `.block`) at `at` in the world
/// on `entity`, where `at` is open, the block under it is ground a sapling
/// takes root on, and the light there is enough for its plant: whether it
/// took root.
pub fn plant(world: &mut World, content: &mut Content, entity: Entity, at: [i32; 3], reference: &str) -> bool {
    let Some((_, block)) = content.block(reference) else { return false };
    let Some(grows) = plant_of(content, &block) else { return false };
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return false };
    let under = blocks.blocks.get([at[0], at[1] - 1, at[2]]);
    if !open(blocks, at) || light_at(&|p| shade(blocks, p), at) < grows.light {
        return false;
    }
    if !soil(world, content, entity, under) {
        return false;
    }
    let Some(number) = crate::blocks::block_number(world, content, entity, reference, true) else { return false };
    set_blocks(world, entity, vec![(at, number)]);
    keep(world, entity, Sapling { at, ..Sapling::default() });
    true
}

/// The world of blocks `point`, in metres, lies in, and the block over the
/// ground under it there: what a sapling dropped there takes root in.
fn ground_under(world: &World, point: Vec3) -> Option<(Entity, [i32; 3])> {
    world
        .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .find_map(|(entity, blocks, local, placed)| {
            let corner = voxear_terrain::corner(local, placed);
            let (place, _) = blocks.block_at((point - corner).to_array());
            // Down from where it is, to the first block of ground, at most a few blocks.
            (0..4).map(|down| [place[0], place[1] - down, place[2]]).find_map(|at| {
                let id = blocks.blocks.get([at[0], at[1] - 1, at[2]]);
                (id != AIR && is_ground(&blocks.faces, id)).then_some((entity, at))
            })
        })
}

/// Plants the sapling `reference` names on the ground under `point`, in
/// metres, where it takes root: whether it did.
pub fn plant_at(world: &mut World, content: &mut Content, point: Vec3, reference: &str) -> bool {
    let Some((entity, at)) = ground_under(world, point) else { return false };
    plant(world, content, entity, at, reference)
}

/// Scatters `count` saplings of `plant` over the places `around`, in
/// metres, in the world on `entity`: each on the ground under one of them,
/// where it takes root. How many did.
pub fn scatter(
    world: &mut World,
    content: &mut Content,
    plant: &PlantDef,
    around: &[Vec3],
    count: u32,
    seed: u32,
) -> u32 {
    let Some(sapling) = plant.sapling.clone() else { return 0 };
    let mut planted = 0;
    for tried in 0..(count * 6).min(around.len() as u32 * 2) {
        if planted >= count || around.is_empty() {
            break;
        }
        let pick = crate::item_drops::mix(seed, tried, 11) as usize % around.len();
        if plant_at(world, content, around[pick], &sapling) {
            planted += 1;
        }
    }
    planted
}

/// The plant whose leaves are the block at `leaves` (a GUID), and which has
/// saplings: what a tree of those leaves scatters as it falls.
pub fn plant_by_leaves(content: &mut Content, leaves: voxear_assets::AssetGuid) -> Option<Arc<PlantDef>> {
    let plants: Vec<String> = content
        .db()
        .entries()
        .filter(|entry| entry.kind == voxear_assets::AssetKind::Plant)
        .map(|entry| entry.guid.to_string())
        .collect();
    plants.into_iter().find_map(|reference| {
        let (_, plant) = content.plant(&reference)?;
        let named = content.parse(plant.leaves.as_deref()?).ok()?.guid()?;
        (named == leaves && plant.sapling.is_some() && plant.saplings[1] > 0).then_some(plant)
    })
}

/// The numbers of a stage's blocks in the world on `entity`: its wood as a
/// log stands, its leaves and its vines, by its plant's blocks.
fn stage_blocks(
    world: &mut World,
    content: &mut Content,
    entity: Entity,
    plant: &PlantDef,
    sapling: &Sapling,
    stage: u8,
) -> Option<Vec<([i32; 3], BlockId)>> {
    let mut number = |named: &Option<String>| -> Option<BlockId> {
        crate::blocks::block_number(world, content, entity, named.as_deref()?, true)
    };
    let (wood, leaves, vine) = (number(&plant.wood)?, number(&plant.leaves).unwrap_or(AIR), number(&plant.vine));
    let model = stage_model(plant, sapling.at, stage);
    let mut logs: HashMap<u8, BlockId> = HashMap::new();
    let log = content.block(plant.wood.as_deref()?).is_some_and(|(_, block)| block.shape == BlockShape::Log);
    let mut blocks = Vec::with_capacity(model.blocks.len());
    for (at, part, state) in model.blocks {
        let block = match part {
            PlantPart::Leaf => leaves,
            PlantPart::Vine => vine.unwrap_or(AIR),
            _ if !log || state == BlockState::default_for(BlockShape::Log) => wood,
            _ => {
                let byte = state.normalized(BlockShape::Log).to_byte();
                if !logs.contains_key(&byte) {
                    let named = plant.wood.as_deref()?;
                    let number = crate::blocks::block_number_state(world, content, entity, named, byte, true)?;
                    logs.insert(byte, number);
                }
                logs[&byte]
            }
        };
        if block != AIR {
            blocks.push(([sapling.at[0] + at[0], sapling.at[1] + at[1], sapling.at[2] + at[2]], block));
        }
    }
    Some(blocks)
}

/// The blocks standing for `sapling` now: its sapling block, or its stage.
fn standing(
    world: &mut World,
    content: &mut Content,
    entity: Entity,
    plant: &PlantDef,
    sapling: &Sapling,
) -> Option<Vec<([i32; 3], BlockId)>> {
    match sapling.stage {
        0 => {
            let block = world.get::<BlockWorld>(entity)?.blocks.get(sapling.at);
            Some(vec![(sapling.at, block)])
        }
        stage => stage_blocks(world, content, entity, plant, sapling, stage),
    }
}

/// What a sapling did as it grew on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grew {
    /// It grew a stage.
    Grew,
    /// It waits for room or light.
    Waits,
    /// It is growing still, not yet due its next stage.
    Growing,
    /// It died: gone.
    Died,
    /// It stopped growing where it stands, or stands whole: a tree like any other.
    Done,
    /// It is gone from where it stood: broken, or cut down.
    Gone,
}

/// Grows `sapling` of the world on `entity` on by `seconds`, or to its next
/// stage at once where `now` says.
fn grow_on(
    world: &mut World,
    content: &mut Content,
    entity: Entity,
    sapling: &mut Sapling,
    seconds: f32,
    now: bool,
) -> Grew {
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return Grew::Gone };
    let here = blocks.blocks.get(sapling.at);
    let Some(def) = definition(world, content, entity, here) else { return Grew::Gone };
    // Its foot is its sapling, or once grown, a log.
    let plant = match sapling.stage {
        0 => match plant_of(content, &def) {
            Some(plant) => plant,
            None => return Grew::Gone,
        },
        _ if def.shape == BlockShape::Log => match planted_by(world, content, entity, sapling) {
            Some(plant) => plant,
            None => return Grew::Done,
        },
        _ => return Grew::Gone,
    };
    let stages = plant.stages.clamp(1, 32) as u8;
    let each = (plant.grow_time / f32::from(stages)).max(0.1);
    if !now {
        sapling.grown += seconds;
        if sapling.grown < each * f32::from(sapling.stage + 1) {
            return Grew::Growing;
        }
    }
    let Some(before) = standing(world, content, entity, &plant, sapling) else { return Grew::Done };
    let mut next_one = *sapling;
    next_one.stage += 1;
    let Some(next) = stage_blocks(world, content, entity, &plant, &next_one, next_one.stage) else { return Grew::Done };
    let blocks = world.get::<BlockWorld>(entity).expect("looked at above");
    let was: HashMap<[i32; 3], BlockId> = before.iter().copied().collect();
    let places: Vec<[i32; 3]> = next.iter().map(|(at, _)| *at).collect();
    let room = has_room(&places, &|at| open(blocks, at) || was.get(&at).is_some_and(|&id| blocks.blocks.get(at) == id));
    // The light over its top: over its leaves, as theirs is its own.
    let top = before.iter().map(|(at, _)| at[1]).max().unwrap_or(sapling.at[1]);
    let light = light_at(&|p| shade(blocks, p), [sapling.at[0], top, sapling.at[2]]);
    if !room || light < plant.light {
        sapling.idle += if now { 0.0 } else { seconds.max(EVERY) };
        if sapling.idle < each.max(FEWEST_WAITED) || now {
            return Grew::Waits;
        }
        if sapling.stage > 0 {
            return Grew::Done;
        }
        set_blocks(world, entity, vec![(sapling.at, AIR)]);
        return Grew::Died;
    }
    sapling.idle = 0.0;
    let taken: Vec<([i32; 3], BlockId)> = before
        .iter()
        .filter(|(at, id)| !places.contains(at) && blocks.blocks.get(*at) == *id)
        .map(|(at, _)| (*at, AIR))
        .collect();
    set_blocks(world, entity, taken.into_iter().chain(next).collect());
    sapling.stage = next_one.stage;
    sapling.grown = sapling.grown.max(each * f32::from(sapling.stage));
    if sapling.stage >= stages { Grew::Done } else { Grew::Grew }
}

/// The plant a sapling grown past its first stage is growing into, kept
/// with the world's saplings by the sapling block it was planted as.
fn planted_by(world: &World, content: &mut Content, entity: Entity, sapling: &Sapling) -> Option<Arc<PlantDef>> {
    if let Some(reference) = world.get::<GrowingKinds>(entity).and_then(|kinds| kinds.0.get(&sapling.at).cloned()) {
        let (_, block) = content.block(&reference)?;
        return plant_of(content, &block);
    }
    // Read back from a save: the plant whose wood its foot is, and which has saplings.
    let foot = world.get::<BlockWorld>(entity)?.blocks.get(sapling.at);
    let wood = world.get::<BlockWorldAsset>(entity)?.drawn.guid(foot)?;
    let plants: Vec<String> = content
        .db()
        .entries()
        .filter(|entry| entry.kind == voxear_assets::AssetKind::Plant)
        .map(|entry| entry.guid.to_string())
        .collect();
    plants.into_iter().find_map(|reference| {
        let (_, plant) = content.plant(&reference)?;
        let named = content.parse(plant.wood.as_deref()?).ok()?.guid()?;
        (named == wood && plant.sapling.is_some()).then_some(plant)
    })
}

/// Of each sapling grown past its first stage in a world, the sapling block
/// it was planted as: what it grows into once its block is a log. The
/// moment's: a sapling read back from a save is known by the plant whose
/// wood its foot is.
#[derive(Clone, Debug, Default)]
pub struct GrowingKinds(pub HashMap<[i32; 3], String>);

impl voxear_ecs::Component for GrowingKinds {}

/// Grows the sapling at `at` in the world on `entity` to its next stage
/// now, where it has room: what it did.
pub fn grow_now(world: &mut World, content: &mut Content, entity: Entity, at: [i32; 3]) -> Grew {
    let Some(mut sapling) = world.get::<Saplings>(entity).and_then(|saplings| saplings.at(at).copied()) else {
        return Grew::Gone;
    };
    remember_kind(world, entity, &sapling);
    let grew = grow_on(world, content, entity, &mut sapling, 0.0, true);
    settle(world, entity, sapling, grew);
    grew
}

/// Remembers the sapling block of `sapling`, while it is one.
fn remember_kind(world: &mut World, entity: Entity, sapling: &Sapling) {
    if sapling.stage != 0 {
        return;
    }
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return };
    let id = blocks.blocks.get(sapling.at);
    let Some(guid) = world.get::<BlockWorldAsset>(entity).and_then(|asset| asset.drawn.guid(id)) else { return };
    let mut kinds = world.get::<GrowingKinds>(entity).cloned().unwrap_or_default();
    kinds.0.insert(sapling.at, guid.to_string());
    world.insert(entity, kinds);
}

/// Keeps `sapling` as it is after it grew on, or lets it go where it is
/// done growing or gone.
fn settle(world: &mut World, entity: Entity, sapling: Sapling, grew: Grew) {
    let mut saplings = world.get::<Saplings>(entity).cloned().unwrap_or_default();
    match grew {
        Grew::Died | Grew::Done | Grew::Gone => saplings.list.retain(|kept| kept.at != sapling.at),
        _ => {
            if let Some(kept) = saplings.list.iter_mut().find(|kept| kept.at == sapling.at) {
                *kept = sapling;
            }
        }
    }
    world.insert(entity, saplings);
}

/// The sapling blocks of the world on `entity`, by number.
fn sapling_numbers(world: &World, content: &Content, entity: Entity) -> Vec<BlockId> {
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return Vec::new() };
    (1..blocks.faces.len() as BlockId)
        .filter(|&id| definition(world, content, entity, id).is_some_and(|def| def.shape == BlockShape::Sapling))
        .collect()
}

/// Takes up the saplings in each chunk of the world on `entity` changed
/// since it was last looked through: put down by the editor or a script.
fn find_new(world: &mut World, content: &Content, entity: Entity, seen: &mut HashMap<[i32; 3], usize>) {
    let numbers = sapling_numbers(world, content, entity);
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return };
    let mut found = Vec::new();
    let mut now = HashMap::with_capacity(seen.len());
    for (key, chunk) in blocks.blocks.chunks() {
        let pointer = Arc::as_ptr(chunk) as usize;
        now.insert(*key, pointer);
        if seen.get(key) == Some(&pointer) || numbers.is_empty() || !chunk.may_hold(|id| numbers.contains(&id)) {
            continue;
        }
        let side = CHUNK as i32;
        for y in 0..side {
            for z in 0..side {
                for x in 0..side {
                    let at = [key[0] * side + x, key[1] * side + y, key[2] * side + z];
                    if numbers.contains(&blocks.blocks.get(at)) {
                        found.push(at);
                    }
                }
            }
        }
    }
    *seen = now;
    if found.is_empty() {
        return;
    }
    let mut saplings = world.get::<Saplings>(entity).cloned().unwrap_or_default();
    for at in found {
        if saplings.at(at).is_none() {
            saplings.list.push(Sapling { at, ..Sapling::default() });
        }
    }
    world.insert(entity, saplings);
}

/// Grows every world's saplings on by `delta` seconds, once a second, and
/// plants the sapling items that have lain long enough on the ground.
pub fn update(world: &mut World, content: &mut Content, delta: f32) {
    let mut growing = world.remove_resource::<Growing>().unwrap_or_default();
    growing.since += delta;
    if growing.since >= EVERY {
        let seconds = std::mem::take(&mut growing.since);
        let worlds: Vec<Entity> = world
            .query_ref::<(Entity, &BlockWorldAsset)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(e, _)| e)
            .collect();
        for entity in worlds {
            find_new(world, content, entity, growing.seen.entry(entity).or_default());
            let list = world.get::<Saplings>(entity).map(|saplings| saplings.list.clone()).unwrap_or_default();
            for mut sapling in list {
                remember_kind(world, entity, &sapling);
                let grew = grow_on(world, content, entity, &mut sapling, seconds, false);
                settle(world, entity, sapling, grew);
            }
        }
        root_drops(world, content, &mut growing.places);
    }
    world.insert_resource(growing);
}

/// Plants the sapling items that have lain on the ground long enough, one
/// of each drop where it takes root.
fn root_drops(world: &mut World, content: &mut Content, places: &mut HashMap<String, Option<String>>) {
    use crate::item_drops::ItemDrop;
    let lying: Vec<(Entity, ItemDrop, Vec3)> = world
        .query_ref::<(Entity, &ItemDrop, &Transform)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, drop, _)| drop.resting && drop.age >= TAKES_ROOT)
        .map(|(entity, drop, transform)| (entity, drop.clone(), transform.position))
        .collect();
    for (entity, mut drop, at) in lying {
        let sapling = places
            .entry(drop.item.clone())
            .or_insert_with(|| {
                let block = crate::item_drops::places(content, &drop.item)?;
                content.block(&block).filter(|(_, def)| def.shape == BlockShape::Sapling).map(|_| block)
            })
            .clone();
        let Some(sapling) = sapling else { continue };
        if !plant_at(world, content, at, &sapling) {
            continue;
        }
        drop.count -= 1;
        match drop.count {
            0 => world.insert(entity, Destroying),
            _ => world.insert(entity, drop),
        };
    }
}

/// Every sapling growing, in every world of blocks: where it stands, in
/// metres, and how far grown, from 0 to 1.
pub fn all(world: &World, content: &mut Content) -> Vec<(Vec3, f32)> {
    let mut found = Vec::new();
    for (entity, saplings, blocks, local, placed) in world
        .query_ref::<(Entity, &Saplings, &BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
    {
        let corner = voxear_terrain::corner(local, placed);
        for sapling in &saplings.list {
            let id = blocks.blocks.get(sapling.at);
            let time = definition(world, content, entity, id)
                .and_then(|def| plant_of(content, &def))
                .map_or(1200.0, |plant| plant.grow_time.max(0.1));
            let middle =
                corner + (Vec3::from(sapling.at.map(|v| v as f32)) + Vec3::new(0.5, 0.0, 0.5)) * blocks.block_size;
            found.push((middle, (sapling.grown / time).clamp(0.0, 1.0)));
        }
    }
    found
}
