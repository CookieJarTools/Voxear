//! Trees of blocks in a game: a log is mined a slice at a time, and a
//! tree comes down once one is cut through.
//!
//! A block whose shape is a log and which chops (`BlockDef::chops`) loses
//! a slice each time it is mined ([`chop`]): it stands thinner, as its
//! placed state says (see `voxear_terrain::blocks::shape::BlockState`),
//! until there is nothing left and it is gone. A log that fells then
//! brings down what it held up ([`fell`]): the logs of its block over the
//! cut, the leaves round them and the vines on them, where nothing else
//! holds them (see `voxear_terrain::blocks::fell`).
//!
//! What comes down is taken out of its world and drawn as a world of its
//! own on an Obj of its own ([`FallingTree`], `LooseBlocks`), which turns
//! about the cut away from whoever cut it, slowly at first and faster as
//! it goes, as a pole falls, until one of its blocks meets the ground. A
//! moment later it breaks: each block where it lies drops what its block
//! says (`BlockDef::drops`), a thin log a share of that as thin as it is,
//! and those lie on the ground as items (see `item_drops`). What fell is
//! told to scripts as a [`Felled`].

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld, BuiltBlocks,
    block::{BlockDef, BlockShape},
    chunk::{AIR, BlockId},
    fell::{LooseBlocks, TreePart, loose_map, loose_tree},
    is_bushy, is_ground, is_vine,
    shape::BlockState,
};

use crate::{Content, blocks::BlockWorldAsset};

/// How fast things fall, in metres a second, each second.
const GRAVITY: f32 = 9.81;

/// How far a tree turns before it lies down, at most, in radians: past
/// level, down a slope.
const MOST_TURN: f32 = 1.9;

/// How long a tree lies where it landed before it breaks, in seconds.
const LIES: f32 = 0.15;

/// The most blocks of a falling tree looked at each step for the ground
/// it meets.
const MOST_LOOKED_AT: usize = 512;

/// What mining a block did, as [`chop`] answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chopped {
    /// It is no log that chops: it breaks as any block does.
    No,
    /// It stands a slice thinner.
    Thinner,
    /// It was cut through, and is gone; nothing came down.
    Cut,
    /// It was cut through, and the tree over it came down, on this Obj.
    Felled(Entity),
}

/// A tree coming down: the world it came out of, the way it falls and how
/// far it has turned, and its blocks, each where it stood and its number
/// in that world.
#[derive(Clone, Debug)]
pub struct FallingTree {
    pub from: Entity,
    /// The level way it falls, and the axis it turns about.
    pub away: Vec3,
    pub axis: Vec3,
    /// The point it turns about, at the foot of the cut, in metres.
    pub pivot: Vec3,
    /// How far it has turned, in radians, and how fast it turns.
    pub angle: f32,
    pub speed: f32,
    /// How high its middle stands over the cut, in metres.
    pub reach: f32,
    pub cut: [i32; 3],
    pub blocks: Vec<([i32; 3], BlockId)>,
    pub corner: Vec3,
    pub block_size: f32,
    /// How long it has lain since it landed; none while it falls.
    pub landed: Option<f32>,
}

impl Component for FallingTree {}

/// A tree that came down and broke where it landed, for scripts to hear:
/// the world it came from, where it was cut, the way it fell, where its
/// middle landed, and the items it broke into.
#[derive(Clone, Debug, PartialEq)]
pub struct Felled {
    pub world: Entity,
    pub cut: Vec3,
    pub away: Vec3,
    pub landed: Vec3,
    pub drops: Vec<Entity>,
}

/// The trees that came down since scripts last asked.
#[derive(Default)]
pub struct FelledTrees(pub Vec<Felled>);

/// The `.block` the block numbered `block` in the world of blocks on
/// `entity` is, and what it says.
fn definition(world: &World, content: &Content, entity: Entity, block: BlockId) -> Option<std::sync::Arc<BlockDef>> {
    let guid = world.get::<BlockWorldAsset>(entity)?.drawn.guid(block)?;
    content.loaded_block(guid)
}

/// The state the block numbered `block` in the world on `entity` is placed
/// in, as its file has it.
fn state_of(world: &World, entity: Entity, block: BlockId) -> BlockState {
    let byte =
        world.get::<BlockWorldAsset>(entity).and_then(|asset| asset.drawn.kind(block)).map_or(0, |kind| kind.state);
    BlockState::from_byte(byte).normalized(BlockShape::Log)
}

/// Puts `changes` in the world of blocks on `entity`, kept as built first
/// so a save holds them.
fn set_blocks(world: &mut World, entity: Entity, changes: Vec<([i32; 3], BlockId)>) {
    let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { return };
    if !world.has::<BuiltBlocks>(entity) {
        world.insert(entity, BuiltBlocks(blocks.blocks.clone()));
    }
    if blocks.set_blocks(changes) > 0 {
        world.insert(entity, blocks);
    }
}

/// Mines the log at `at` in the world of blocks on `entity` once: a slice
/// off it, or the last, and then what it held up comes down, falling
/// `away` (a way along the ground; any other is taken level).
pub fn chop(world: &mut World, content: &mut Content, entity: Entity, at: [i32; 3], away: Vec3) -> Chopped {
    let Some(block) = world.get::<BlockWorld>(entity).map(|blocks| blocks.blocks.get(at)) else { return Chopped::No };
    let Some(def) = definition(world, content, entity, block).filter(|_| block != AIR) else { return Chopped::No };
    let Some(slice) = def.slice() else { return Chopped::No };
    let state = state_of(world, entity, block);
    if let Some(thinner) = state.thinned(slice) {
        let family = world.get::<BlockWorld>(entity).and_then(|b| b.faces.get(usize::from(block)).map(|f| f.family));
        let number =
            family.and_then(|family| crate::blocks::state_number(world, content, entity, family, thinner.to_byte()));
        if let Some(number) = number {
            set_blocks(world, entity, vec![(at, number)]);
            return Chopped::Thinner;
        }
    }
    set_blocks(world, entity, vec![(at, AIR)]);
    if !def.fells {
        return Chopped::Cut;
    }
    match fell(world, content, entity, at, block, away) {
        Some(tree) => Chopped::Felled(tree),
        None => Chopped::Cut,
    }
}

/// What each block of the world on `entity` is as a tree comes down, its
/// logs those of the family `log`: leaves are bushy blocks or blocks that
/// sway, vines are vines, and any other log is another tree's.
fn part_of(world: &World, content: &Content, entity: Entity, log: BlockId) -> Box<dyn Fn(BlockId) -> TreePart> {
    let Some(blocks) = world.get::<BlockWorld>(entity) else { return Box::new(|_| TreePart::Nothing) };
    let faces = blocks.faces.clone();
    let logs: Vec<bool> = (0..faces.len())
        .map(|id| definition(world, content, entity, id as BlockId).is_some_and(|def| def.shape == BlockShape::Log))
        .collect();
    Box::new(move |id: BlockId| {
        let kind = faces.get(usize::from(id)).copied().unwrap_or_default();
        match () {
            _ if id == AIR || kind.water => TreePart::Nothing,
            _ if kind.family != 0 && kind.family == log => TreePart::Log,
            _ if logs.get(usize::from(id)).copied().unwrap_or(false) => TreePart::OtherLog,
            _ if is_vine(&faces, id) => TreePart::Vine,
            _ if is_bushy(&faces, id) || kind.sways => TreePart::Leaf,
            _ if is_ground(&faces, id) => TreePart::Ground,
            _ => TreePart::Nothing,
        }
    })
}

/// Brings down what the log numbered `log`, cut through at `cut` in the
/// world on `entity`, held up, where it comes loose: out of the world and
/// onto an Obj of its own that falls `away`. The Obj; none where nothing
/// came loose.
pub fn fell(
    world: &mut World,
    content: &mut Content,
    entity: Entity,
    cut: [i32; 3],
    log: BlockId,
    away: Vec3,
) -> Option<Entity> {
    let (blocks, transform) = (world.get::<BlockWorld>(entity)?, world.get::<Transform>(entity)?);
    let family = blocks.faces.get(usize::from(log))?.family;
    let kind = part_of(world, content, entity, family);
    let map = blocks.blocks.clone();
    let part = |at: [i32; 3]| kind(map.get(at));
    let loose = loose_tree(&part, cut)?;
    let tree: Vec<([i32; 3], BlockId)> = loose.iter().map(|&at| (at, map.get(at))).collect();
    let (size, faces) = (blocks.block_size, blocks.faces.clone());
    let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
    let look = world.get::<voxear_render::BlockLook>(entity).cloned();
    set_blocks(world, entity, tree.iter().map(|&(at, _)| (at, AIR)).collect());
    // Level, away from whoever cut it; any way at all where none is given.
    let level = Vec3::new(away.x, 0.0, away.z);
    let away = if level.length_squared() > 1e-6 { level.normalize() } else { Vec3::X };
    let pivot = corner + (Vec3::new(cut[0] as f32, cut[1] as f32, cut[2] as f32) + Vec3::new(0.5, 0.0, 0.5)) * size;
    let middle = tree.iter().map(|(at, _)| (at[1] - cut[1]) as f32 + 0.5).sum::<f32>() / tree.len().max(1) as f32;
    let falling = FallingTree {
        from: entity,
        away,
        axis: Vec3::Y.cross(away),
        pivot,
        // A nudge, so it starts to go.
        angle: 0.02,
        speed: 0.25,
        reach: (middle * size).max(size),
        cut,
        blocks: tree.clone(),
        corner,
        block_size: size,
        landed: None,
    };
    let offset = (corner - pivot).to_array();
    let loose = LooseBlocks { world: BlockWorld::new(loose_map(&tree), &faces, size), offset, from: Some(entity) };
    let obj = world.spawn((Transform::new(pivot, Quat::IDENTITY, Vec3::ONE), loose, falling, EditorOnly));
    if let Some(look) = look {
        world.insert(obj, look);
    }
    // It falls in the dimension of the world it grew in.
    let dimension = voxear_scene::dimension_of(world, entity);
    voxear_scene::set_dimension(world, obj, dimension);
    Some(obj)
}

/// Where the middle of block `at` of `tree` is now, in metres, as it has
/// turned.
fn turned_middle(tree: &FallingTree, at: [i32; 3]) -> Vec3 {
    let from =
        Vec3::new((at[0] - tree.cut[0]) as f32, (at[1] - tree.cut[1]) as f32 + 0.5, (at[2] - tree.cut[2]) as f32);
    tree.pivot + Quat::from_axis_angle(tree.axis, tree.angle) * (from * tree.block_size)
}

/// Whether a block of `tree` away from its foot has met ground of the
/// world it fell from.
fn met_ground(world: &World, tree: &FallingTree) -> bool {
    let Some(blocks) = world.get::<BlockWorld>(tree.from) else { return true };
    let far: Vec<&([i32; 3], BlockId)> = tree
        .blocks
        .iter()
        .filter(|(at, _)| {
            let d = [at[0] - tree.cut[0], at[1] - tree.cut[1], at[2] - tree.cut[2]];
            d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 4
        })
        .collect();
    let step = far.len().div_ceil(MOST_LOOKED_AT).max(1);
    far.into_iter().step_by(step).any(|(at, _)| {
        let p = (turned_middle(tree, *at) - tree.corner) / tree.block_size;
        let id = blocks.blocks.get(p.floor().as_ivec3().to_array());
        id != AIR && is_ground(&blocks.faces, id) && !is_bushy(&blocks.faces, id)
    })
}

/// Turns each falling tree on by `delta` seconds, and breaks each that has
/// lain long enough where it landed into what its blocks drop.
pub fn update(world: &mut World, content: &mut Content, delta: f32) {
    let trees: Vec<Entity> =
        world.query_ref::<(Entity, &FallingTree)>().without::<Destroying>().into_iter().map(|(e, _)| e).collect();
    for entity in trees {
        let Some(mut tree) = world.get::<FallingTree>(entity).cloned() else { continue };
        match tree.landed {
            None => {
                // As a pole falls about its foot: faster the further over it leans.
                let pull = 3.0 * GRAVITY / (4.0 * tree.reach.max(0.5));
                tree.speed += pull * tree.angle.sin().max(0.05) * delta;
                tree.angle = (tree.angle + tree.speed * delta).min(MOST_TURN);
                if tree.angle >= MOST_TURN || met_ground(world, &tree) {
                    tree.landed = Some(0.0);
                }
                if let Some(transform) = world.get_mut::<Transform>(entity) {
                    transform.rotation = Quat::from_axis_angle(tree.axis, tree.angle);
                }
                world.insert(entity, tree);
            }
            Some(lain) if lain + delta < LIES => {
                tree.landed = Some(lain + delta);
                world.insert(entity, tree);
            }
            Some(_) => {
                let felled = break_up(world, content, &tree);
                world.insert(entity, Destroying);
                if world.resource::<FelledTrees>().is_none() {
                    world.insert_resource(FelledTrees::default());
                }
                world.resource_mut::<FelledTrees>().expect("put in above").0.push(felled);
            }
        }
    }
}

/// `tree`, lying where it landed, broken into what its blocks drop there.
fn break_up(world: &mut World, content: &mut Content, tree: &FallingTree) -> Felled {
    let mut drops = Vec::new();
    let mut middle = Vec3::ZERO;
    for (number, &(at, block)) in tree.blocks.iter().enumerate() {
        let place = turned_middle(tree, at);
        middle += place;
        let Some(def) = definition(world, content, tree.from, block) else { continue };
        // A thin log drops a share of a whole one's, as much of a block as it is.
        let share = match def.shape {
            BlockShape::Log => {
                let across = f32::from(state_of(world, tree.from, block).across()) / 16.0;
                across * across
            }
            _ => 1.0,
        };
        let seed = crate::item_drops::mix(at[0] as u32, at[1] as u32 ^ (number as u32).rotate_left(16), at[2] as u32);
        for (made, drop) in crate::item_drops::rolled(&def.drops, share, seed).into_iter().enumerate() {
            let (item, count) = drop;
            let jitter = crate::item_drops::mix(seed, made as u32, 7);
            let up = 1.5 + (jitter % 100) as f32 / 50.0;
            let side =
                Vec3::new(((jitter >> 8) % 100) as f32 / 100.0 - 0.5, 0.0, ((jitter >> 16) % 100) as f32 / 100.0 - 0.5);
            let thrown = Vec3::new(side.x * 2.0, up, side.z * 2.0);
            drops.push(crate::item_drops::drop_item(world, &item, count, place, thrown));
        }
    }
    // Its plant's saplings, scattered where its crown landed.
    let leaves: Vec<(Vec3, BlockId)> = tree
        .blocks
        .iter()
        .filter(|(_, block)| {
            definition(world, content, tree.from, *block).is_some_and(|def| def.shape != BlockShape::Log)
        })
        .map(|&(at, block)| (turned_middle(tree, at), block))
        .collect();
    let kind = leaves.first().and_then(|(_, block)| world.get::<BlockWorldAsset>(tree.from)?.drawn.guid(*block));
    if let Some(plant) = kind.and_then(|leaves| crate::saplings::plant_by_leaves(content, leaves)) {
        let seed = crate::item_drops::mix(tree.cut[0] as u32, tree.cut[1] as u32, tree.cut[2] as u32);
        let [least, most] = [plant.saplings[0].min(plant.saplings[1]), plant.saplings[0].max(plant.saplings[1])];
        let count = least + crate::item_drops::mix(seed, 3, 5) % (most - least + 1);
        let around: Vec<Vec3> = leaves.iter().map(|(at, _)| *at).collect();
        crate::saplings::scatter(world, content, &plant, &around, count, seed);
    }
    let count = tree.blocks.len().max(1) as f32;
    let cut = tree.pivot;
    Felled { world: tree.from, cut, away: tree.away, landed: middle / count, drops }
}

/// The first world of blocks with a block at `at`, a point in metres, of
/// the dimension the game is seen from, and where that block is in it.
pub fn block_at(world: &World, at: Vec3) -> Option<(Entity, [i32; 3])> {
    let current = crate::dimensions::current(world);
    world
        .query_ref::<(Entity, &BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| voxear_scene::dimension_of(world, *entity) == current)
        .find_map(|(entity, blocks, local, placed)| {
            let corner = voxear_terrain::corner(local, placed);
            let (place, block) = blocks.block_at((at - corner).to_array());
            (block != AIR).then_some((entity, place))
        })
}

/// Cuts the log at `at` in the world of blocks on `entity` through at
/// once, and brings down what it held up, falling `away`.
pub fn cut_through(world: &mut World, content: &mut Content, entity: Entity, at: [i32; 3], away: Vec3) -> Chopped {
    let Some(block) = world.get::<BlockWorld>(entity).map(|blocks| blocks.blocks.get(at)) else { return Chopped::No };
    let Some(def) =
        definition(world, content, entity, block).filter(|def| block != AIR && def.shape == BlockShape::Log)
    else {
        return Chopped::No;
    };
    set_blocks(world, entity, vec![(at, AIR)]);
    match def.fells.then(|| fell(world, content, entity, at, block, away)).flatten() {
        Some(tree) => Chopped::Felled(tree),
        None => Chopped::Cut,
    }
}

/// Every tree that came down since last asked, taken.
pub fn take_felled(world: &mut World) -> Vec<Felled> {
    world.resource_mut::<FelledTrees>().map(|felled| std::mem::take(&mut felled.0)).unwrap_or_default()
}
