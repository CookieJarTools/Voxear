//! Plants a Block World grows by itself: a Foliage under a Block World with
//! a `grow` section (see `scene_file::GrowData`) has its plants worked out
//! from the world's ground, a chunk column at a time, as the ground is made
//! round the camera (see `voxear_terrain::blocks::grow`).
//!
//! Each column's plants are kept on an Obj of their own under the Block
//! World, out of sight: with no name, in no scene, never saved
//! ([`GrownColumn`]). It draws with the Foliage's mesh, material and
//! distances and collides with its trunks, so everything a Foliage does its
//! grown plants do, and a column coming or going builds nothing else again.
//! Only columns within the Foliage's draw distance of where the world is
//! seen from have their plants, eight columns made a tick, nearest first. Where
//! that reaches past the world's own blocks, a column under a coarser ring
//! that is showing has the same plants, worked out from the generator's
//! heights, and grown again from its blocks once they are made.
//!
//! A plant whose block is dug away or built over is marked gone, and is
//! back once the ground is as it was made: worked out here from the blocks,
//! on every machine alike, so no save or message says which plants stand.
//! One a script takes away is kept on the Foliage's Obj by its column and
//! number (`voxear_scene::CutPlants`), and stays gone however often its
//! column's plants are made again.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use voxear_assets::AssetGuid;
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_render::{Foliage, StarterPlant};
use voxear_scene::{
    CutPlants, Destroying, EditorOnly, Layer, Parent, Plant, PlantSet, Plants, Switch, Switched, Transform, Trunk,
    WorldTransform,
};
use voxear_terrain::blocks::{
    BlockMap, BlockWorld,
    chunk::{CHUNK, Chunk},
    grow::{FarGround, Ground, GrowRule, Grown, Tops, Woods, chunks_under, gone, gone_far, grow_far, grow_on},
    mesh::ChunkMesh,
    stream::{FarBuilt, MOST_LEVELS},
};
use voxear_terrain::generate::WorldGen;

use crate::{Content, blocks::BlockWorldAsset, scene_file::FoliageSource};

/// How many columns' plants one Foliage has made in a tick, at most, and
/// how many all of them together.
pub const COLUMNS_A_TICK: usize = 4;
pub const COLUMNS_A_TICK_IN_ALL: usize = 8;

/// One chunk column's plants of a Foliage that grows on a Block World: the
/// Foliage's Obj and which column. The Obj it is on is the engine's own,
/// under the Block World.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrownColumn {
    pub source: Entity,
    pub column: [i32; 2],
}

impl Component for GrownColumn {}

/// A column's plants as they are kept: the chunk that tells the ground they
/// grew from is still the ground there, the Obj that draws them, each
/// plant's block, and the faces of the chunks those blocks are in, which
/// are made again whenever one of them changes.
struct Kept {
    made_from: Option<([i32; 3], Arc<Chunk>)>,
    child: Option<Entity>,
    grown: Arc<Vec<Grown>>,
    watched: Vec<([i32; 3], Option<Arc<ChunkMesh>>)>,
    /// Where they were grown on ground with no blocks of its own made: the level of the coarsest ring whose
    /// ground they are kept clear of.
    far: Option<u32>,
    /// Those gone by what their ground is now.
    gone: PlantSet,
    /// The plants marked gone on their Obj as this last left them: any more are a script's doing.
    set: Arc<PlantSet>,
}

/// What a Foliage's grown plants share with it: how they are drawn, their
/// trunks, what one cut down becomes, and their layer.
#[derive(Clone, PartialEq)]
struct Look {
    foliage: Option<Foliage>,
    trunk: Option<Trunk>,
    cut: Option<String>,
    layer: Option<Layer>,
}

/// A world's coarser rings' faces, as it has them.
type FarFaces = Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>;

/// What decided which columns a Foliage's plants were wanted on, when that
/// was last worked out: the world's coarser rings showing, its own columns
/// made, where it was seen from (to eight blocks), and how far its plants
/// reach. While none of it changes, and every column wanted has its plants,
/// nothing is worked out again.
struct Seen {
    far: FarFaces,
    made: Option<Arc<HashSet<[i32; 2]>>>,
    from: Option<[i32; 2]>,
    reach: f32,
    /// The first ring its plants show as a wood in, where they do.
    wood_from: Option<u32>,
}

impl Seen {
    fn same(&self, other: &Seen) -> bool {
        let made = match (&self.made, &other.made) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let same = self.from == other.from && self.reach == other.reach && self.wood_from == other.wood_from;
        Arc::ptr_eq(&self.far, &other.far) && made && same
    }
}

/// One Foliage's grown plants: what they were grown from, and each column's.
struct Growing {
    stamp: u64,
    look: Look,
    columns: HashMap<[i32; 2], Kept>,
    /// Whether every column wanted has its plants.
    settled: bool,
    seen: Option<Seen>,
}

/// Every growing Foliage's plants, by its Obj; and each world's coarser
/// rings showing, as last worked out from its faces.
#[derive(Default)]
struct Growth {
    sources: HashMap<Entity, Growing>,
    shown: HashMap<Entity, (FarFaces, Arc<HashSet<[i32; 3]>>)>,
    /// The highest blocks of columns plants were lately grown on, by world and column, each with the chunk
    /// that tells its ground is still the ground there, and how many ticks since it was last read: one
    /// Foliage after another grows on the same columns, some ticks apart.
    tops: HashMap<(Entity, [i32; 2]), (Option<Arc<Chunk>>, u32, Arc<Tops>)>,
}

/// How many ticks a column's highest blocks are kept unread before they are let go.
const TOPS_KEPT: u32 = 240;

/// A Foliage that grows, as a tick finds it: its Obj, its rule as written,
/// how its plants look, and the Block World it is under.
struct Grower {
    entity: Entity,
    data: crate::scene_file::GrowData,
    look: Look,
    draw_distance: f32,
    world: Entity,
    /// The mesh its plants are, as its scene names it: a part of a model.
    mesh: String,
}

/// The Block World an Obj is under, or is itself.
fn world_over(world: &World, entity: Entity) -> Option<Entity> {
    let mut at = Some(entity);
    for _ in 0..64 {
        let entity = at?;
        if world.has::<BlockWorld>(entity) && world.has::<BlockWorldAsset>(entity) {
            return Some(entity);
        }
        at = world.get::<Parent>(entity).map(|parent| parent.0);
    }
    None
}

fn growers(world: &World) -> Vec<Grower> {
    let switched = Switched::of(world);
    world
        .query_ref::<(Entity, &FoliageSource, &Plants)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| switched.is_on(world, *entity, Switch::Foliage))
        .filter_map(|(entity, source, plants)| {
            let data = source.0.grow.clone()?;
            let foliage = world.get::<Foliage>(entity).cloned();
            let draw_distance = foliage.as_ref().map_or(source.0.draw_distance, |foliage| foliage.draw_distance);
            let look = Look {
                foliage,
                trunk: plants.trunk,
                cut: plants.cut.clone(),
                layer: world.get::<Layer>(entity).copied(),
            };
            let mesh = source.0.mesh.clone();
            Some(Grower { entity, data, look, draw_distance, world: world_over(world, entity)?, mesh })
        })
        .collect()
}

/// How far `from`, in blocks along x and z, is from chunk column `column`:
/// nothing inside it.
fn away(column: [i32; 2], from: [f32; 2]) -> f32 {
    let side = CHUNK as f32;
    let off = |axis: usize| {
        let low = column[axis] as f32 * side;
        (low - from[axis]).max(from[axis] - (low + side)).max(0.0)
    };
    (off(0) * off(0) + off(1) * off(1)).sqrt()
}

/// The highest chunk of each chunk column of `blocks`.
fn tops(blocks: &BlockMap) -> HashMap<[i32; 2], ([i32; 3], Arc<Chunk>)> {
    let mut tops: HashMap<[i32; 2], ([i32; 3], Arc<Chunk>)> = HashMap::new();
    for (key, chunk) in blocks.chunks().filter(|(_, chunk)| !chunk.is_air()) {
        let top = tops.entry([key[0], key[2]]).or_insert_with(|| (*key, chunk.clone()));
        if key[1] > top.0[1] {
            *top = (*key, chunk.clone());
        }
    }
    tops
}

fn despawn(world: &mut World, kept: &Kept) {
    if let Some(child) = kept.child {
        world.despawn(child);
    }
}

/// The faces of chunks `keys` of `blocks` as they are now.
fn faces_of(blocks: &BlockWorld, keys: Vec<[i32; 3]>) -> Vec<([i32; 3], Option<Arc<ChunkMesh>>)> {
    keys.into_iter().map(|key| (key, blocks.meshes.get(&key).cloned())).collect()
}

/// Whether any of the chunks `watched` has had its faces made again.
fn changed(blocks: &BlockWorld, watched: &[([i32; 3], Option<Arc<ChunkMesh>>)]) -> bool {
    watched.iter().any(|(key, was)| match (was, blocks.meshes.get(key)) {
        (Some(was), Some(now)) => !Arc::ptr_eq(was, now),
        (None, None) => false,
        _ => true,
    })
}

/// The chunk columns of a world's coarser rings that are showing, by level
/// and place in that level's columns.
fn shown(blocks: &BlockWorld) -> HashSet<[i32; 3]> {
    blocks.far.keys().map(|[level, x, _, z]| [*level, *x, *z]).collect()
}

/// The finest of the coarser rings showing over chunk column `column` of a
/// world's own, by its level; none where none shows.
fn finest(shown: &HashSet<[i32; 3]>, [x, z]: [i32; 2]) -> Option<u32> {
    (1..MOST_LEVELS as i32).find(|level| shown.contains(&[*level, x >> level, z >> level])).map(|level| level as u32)
}

/// Whether plants kept clear of the ground of rings out to level `rings`
/// still suit a column whose finest ring showing is `finest`: that ring or
/// the one inside it. Under a coarser one they may hang over its ground, and
/// under finer ones they are sunk further than they need be, so either way
/// they are grown again.
fn suits(rings: u32, finest: Option<u32>) -> bool {
    finest.is_some_and(|finest| finest <= rings && finest + 1 >= rings)
}

/// Grows every growing Foliage's plants on the columns of its Block World
/// near where the world is seen from (the editor's view, or else the game's
/// camera; everywhere where there is neither, as on a server), lets go of
/// those of columns that left, and marks the plants whose ground changed
/// and those scripts took away. Past the world's own blocks, out to the
/// Foliage's draw distance, plants grow wherever a coarser ring shows the
/// ground, from the generator's heights. After [`crate::blocks::stream`],
/// so ground made this tick has its plants this tick.
pub fn update(world: &mut World, content: &mut Content) {
    voxear_core::profile_scope!("Grown Plants");
    let growers = growers(world);
    let mut growth = world.remove_resource::<Growth>().unwrap_or_default();
    // A Foliage that grows no longer, or is gone, has none.
    growth.sources.retain(|source, growing| {
        let grows = growers.iter().any(|grower| grower.entity == *source);
        if !grows {
            growing.columns.values().for_each(|kept| despawn(world, kept));
        }
        grows
    });
    // A far block its world does not hold yet is numbered there, as a block built with is: the far view takes it
    // up from the next tick.
    for grower in &growers {
        let Some(far_block) = grower.data.far_block.as_deref() else { continue };
        let held = content.block(far_block).is_none_or(|(guid, _)| {
            world.get::<BlockWorldAsset>(grower.world).is_none_or(|asset| asset.drawn.number_of(guid).is_some())
        });
        if !held {
            crate::blocks::block_number(world, content, grower.world, far_block, true);
        }
    }
    let viewer = crate::terrain::viewer(world);
    let mut all_tops: HashMap<Entity, HashMap<[i32; 2], ([i32; 3], Arc<Chunk>)>> = HashMap::new();
    growth.shown.retain(|world, _| growers.iter().any(|grower| grower.world == *world));
    // Each world read once a tick, however many of its Foliage grow.
    let mut worlds: HashMap<Entity, Option<(BlockWorldAsset, BlockWorld, Option<Arc<WorldGen>>)>> = HashMap::new();
    let mut left = COLUMNS_A_TICK_IN_ALL;
    growth.tops.retain(|_, (_, unread, _)| {
        *unread += 1;
        *unread <= TOPS_KEPT
    });
    // Each Foliage's model, for the Foliages whose plants keep near another's: found by the model it grows.
    let model_of = |content: &Content, reference: &str| content.parse(reference).ok().and_then(|parsed| parsed.guid());
    let grown_models: Vec<(Entity, Option<AssetGuid>, crate::scene_file::GrowData)> =
        growers.iter().map(|grower| (grower.world, model_of(content, &grower.mesh), grower.data.clone())).collect();
    for grower in growers {
        let read = worlds.entry(grower.world).or_insert_with(|| {
            let asset = world.get::<BlockWorldAsset>(grower.world).cloned()?;
            let blocks = world.get::<BlockWorld>(grower.world).cloned()?;
            let generator = crate::blocks::generator(content, &asset.file);
            Some((asset, blocks, generator))
        });
        let Some((asset, blocks, generator)) = read.clone() else { continue };
        let wanted: Vec<AssetGuid> =
            grower.data.near_plants.iter().filter_map(|model| model_of(content, model)).collect();
        let mut number = |block: &str| {
            let (guid, _) = content.block(block)?;
            asset.drawn.number_of(guid)
        };
        let mut rule = grower.data.rule(&mut number);
        // The rules of the world's other Foliages that grow the models its plants keep near to.
        if let Some(near) = &mut rule.near {
            for (world, model, data) in &grown_models {
                if *world == grower.world && model.is_some_and(|model| wanted.contains(&model)) {
                    near.plants.push(GrowRule { near: None, ..data.rule(&mut number) });
                }
            }
        }
        let stamp = stamp_of(&rule, &asset, &blocks, generator.as_deref(), grower.world);
        // Where its plants show as a wood far off, they are grown only on the rings inside the wood's first.
        let wood_from = woods_of(world, content, &grower, &asset).map(|woods| woods.from);
        let inside = |ring: Option<u32>| ring.is_some_and(|ring| wood_from.is_none_or(|from| ring < from));
        let growing = growth.sources.entry(grower.entity).or_insert_with(|| Growing {
            stamp,
            look: grower.look.clone(),
            columns: HashMap::new(),
            settled: false,
            seen: None,
        });
        if growing.stamp != stamp {
            growing.columns.drain().for_each(|(_, kept)| despawn(world, &kept));
            (growing.stamp, growing.seen) = (stamp, None);
        }
        // Where the world is seen from, in its blocks; and how far its plants reach.
        let placed = world.get::<WorldTransform>(grower.world).copied();
        let corner = match (world.get::<Transform>(grower.world), &placed) {
            (Some(local), placed) => voxear_terrain::corner(local, placed.as_ref()),
            (None, _) => Vec3::ZERO,
        };
        let size = blocks.block_size.max(0.01);
        let from = viewer.map(|viewer| [(viewer.x - corner.x) / size, (viewer.z - corner.z) / size]);
        let reach = grower.draw_distance.max(0.0) / size;
        let near = |column: [i32; 2], slack: f32| from.is_none_or(|from| away(column, from) <= reach + slack);
        // Ground past the world's own blocks: only a world without end's, seen from somewhere, with plants drawn
        // that far.
        let made = blocks.columns.clone();
        let endless = asset.file.generate.as_ref().and_then(|generate| Some((generate, generate.endless?)));
        let far = match (endless, generator.as_deref(), from, &made) {
            (Some((generate, endless)), Some(generator), Some(from), Some(_)) if reach * size > endless.distance => {
                Some((FarGround { file: &asset.drawn, generate, world: generator }, from))
            }
            _ => None,
        };
        // While nothing that decides which columns are wanted has changed, and all of them have their plants,
        // only the plants' ground and what scripts took are looked at.
        let seen = Seen {
            far: blocks.far.clone(),
            made: made.clone(),
            from: from.map(|from| from.map(|v| (v / 8.0).floor() as i32)),
            reach,
            wood_from,
        };
        let unchanged = growing.settled && growing.seen.as_ref().is_some_and(|was| was.same(&seen));
        let shown = match (&far, growth.shown.get(&grower.world)) {
            (None, _) => Arc::default(),
            (Some(_), Some((faces, shown))) if Arc::ptr_eq(faces, &blocks.far) => shown.clone(),
            (Some(_), _) => {
                let shown = Arc::new(shown(&blocks));
                growth.shown.insert(grower.world, (blocks.far.clone(), shown.clone()));
                shown
            }
        };
        let shown = &*shown;
        // Columns that left, went too far, or whose ground was made again, let go of: and those whose own blocks
        // came or went, which are grown again from what is there now.
        let stays = |column: &[i32; 2], kept: &Kept| {
            let is_made = made.as_ref().is_none_or(|made| made.contains(column));
            let same_ground = kept
                .made_from
                .as_ref()
                .is_none_or(|(key, chunk)| asset.base.chunk(*key).is_some_and(|now| Arc::ptr_eq(now, chunk)));
            let there = match kept.far {
                Some(rings) => {
                    let ring = finest(shown, *column);
                    !is_made && far.is_some() && suits(rings, ring) && inside(ring)
                }
                None => is_made && same_ground,
            };
            there && near(*column, CHUNK as f32 * 0.5)
        };
        let leaving: Vec<[i32; 2]> = match unchanged {
            true => Vec::new(),
            false => growing.columns.iter().filter(|(at, kept)| !stays(at, kept)).map(|(at, _)| *at).collect(),
        };
        for column in leaving {
            if let Some(kept) = growing.columns.remove(&column) {
                despawn(world, &kept);
            }
        }
        // How its plants look changed: every column's Obj takes it up.
        if growing.look != grower.look {
            growing.look = grower.look.clone();
            for child in growing.columns.values().filter_map(|kept| kept.child) {
                dress(world, child, &growing.look);
            }
        }
        // Plants whose ground changed: gone, or back.
        for kept in growing.columns.values_mut().filter(|kept| changed(&blocks, &kept.watched)) {
            kept.watched = faces_of(&blocks, chunks_under(&kept.grown));
            kept.gone = gone(&kept.grown, &blocks.blocks);
        }
        // Plants a script took away since last tick are kept as cut, by column, on the Foliage's Obj; and every
        // column's plants are gone where their ground changed or they were cut.
        let mut cut = world.get::<CutPlants>(grower.entity).cloned().unwrap_or_default();
        let cut_before = cut.clone();
        for (column, kept) in growing.columns.iter_mut() {
            let Some(plants) = kept.child.and_then(|child| world.get_mut::<Plants>(child)) else { continue };
            if !Arc::ptr_eq(&plants.removed, &kept.set) {
                cut.take(*column, &plants.removed.without(&kept.set));
            }
            let now = cut.of(*column).map_or_else(|| kept.gone.clone(), |cut| kept.gone.with(cut));
            if !now.same(&plants.removed) {
                plants.removed = Arc::new(now);
            }
            kept.set = plants.removed.clone();
        }
        if cut != cut_before {
            world.insert(grower.entity, cut.clone());
        }
        if unchanged {
            continue;
        }
        growing.seen = Some(seen);
        // Columns wanted that have no plants yet, nearest first, a few a tick: those with blocks of their own,
        // then those a coarser ring shows.
        let tops = all_tops.entry(grower.world).or_insert_with(HashMap::new);
        let mut wanted: Vec<([i32; 2], Option<u32>)> = match &made {
            Some(made) => made.iter().map(|column| (*column, None)).collect(),
            None => {
                if tops.is_empty() {
                    *tops = self::tops(&asset.base);
                }
                tops.keys().map(|column| (*column, None)).collect()
            }
        };
        if let (Some((_, from)), Some(made)) = (&far, &made) {
            let side = CHUNK as f32;
            let span = |axis: usize| {
                ((from[axis] - reach) / side).floor() as i32..=((from[axis] + reach) / side).floor() as i32
            };
            let around = span(1).flat_map(|z| span(0).map(move |x| [x, z]));
            // Clear of the ground of the finest ring showing there and of the next out, which shows as the
            // camera goes from it.
            let rings = |column: &[i32; 2]| {
                let ring = finest(shown, *column).filter(|ring| inside(Some(*ring)))?;
                Some((ring + 1).min(MOST_LEVELS - 1))
            };
            let far_off = around.filter(|column| !made.contains(column));
            wanted.extend(far_off.filter_map(|column| Some((column, Some(rings(&column)?)))));
        }
        wanted.retain(|(column, _)| !growing.columns.contains_key(column) && near(*column, 0.0));
        let now = COLUMNS_A_TICK.min(left);
        growing.settled = wanted.len() <= now;
        if wanted.is_empty() || now == 0 {
            continue;
        }
        let nearness = |column: &[i32; 2]| from.map_or(0.0, |from| away(*column, from));
        wanted.sort_by(|(a, _), (b, _)| nearness(a).total_cmp(&nearness(b)).then(a.cmp(b)));
        // A far column asks the generator for its ground, so it counts as two of those whose blocks are read.
        let mut room = now * 2;
        wanted.retain(|(_, far)| {
            let cost = if far.is_some() { 2 } else { 1 };
            let fits = room >= cost;
            room = room.saturating_sub(cost);
            fits
        });
        left = left.saturating_sub((now * 2 - room).div_ceil(2));
        if tops.is_empty() && wanted.iter().any(|(_, far)| far.is_none()) {
            *tops = self::tops(&asset.base);
        }
        let ground = Ground {
            blocks: &asset.base,
            faces: &blocks.faces,
            block_size: blocks.block_size,
            smooth: blocks.smooth,
            generated: asset.file.generate.as_ref().zip(generator.as_deref()),
        };
        let parent_matrix = placed.map_or_else(|| voxear_core::math::Mat4::from_translation(corner), |placed| placed.0);
        for (column, far_off) in wanted {
            let (grown, gone) = match (far_off, &far) {
                (Some(rings), Some((far, _))) => {
                    let grown = grow_far(&rule, far, column, rings);
                    let gone = gone_far(&grown, &asset.pending);
                    (grown, gone)
                }
                _ => {
                    // Its highest blocks as read for another Foliage, while its ground is the ground they were
                    // read from.
                    let from = tops.get(&column).map(|(_, chunk)| chunk.clone());
                    let same = |kept: &Option<Arc<Chunk>>| match (kept, &from) {
                        (Some(kept), Some(from)) => Arc::ptr_eq(kept, from),
                        (None, None) => true,
                        _ => false,
                    };
                    let read = growth.tops.get_mut(&(grower.world, column)).filter(|(kept, ..)| same(kept));
                    let highest = match read {
                        Some((_, unread, highest)) => {
                            *unread = 0;
                            highest.clone()
                        }
                        None => {
                            let highest = Arc::new(Tops::of(&asset.base, column, &blocks.faces));
                            growth.tops.insert((grower.world, column), (from, 0, highest.clone()));
                            highest
                        }
                    };
                    let grown = grow_on(&rule, &ground, column, &highest);
                    let gone = gone(&grown, &blocks.blocks);
                    (grown, gone)
                }
            };
            let grown = Arc::new(grown);
            let set = Arc::new(cut.of(column).map_or_else(|| gone.clone(), |cut| gone.with(cut)));
            let child = (!grown.is_empty()).then(|| {
                let plants: Vec<Plant> = grown.iter().map(|grown| grown.plant).collect();
                let plants = Plants { removed: set.clone(), ..Plants::new(plants) };
                let marker = GrownColumn { source: grower.entity, column };
                let placed = WorldTransform(parent_matrix);
                let child =
                    world.spawn((Transform::IDENTITY, placed, Parent(grower.world), EditorOnly, marker, plants));
                dress(world, child, &growing.look);
                child
            });
            let watched = if far_off.is_some() { Vec::new() } else { faces_of(&blocks, chunks_under(&grown)) };
            let made_from = tops.get(&column).filter(|_| far_off.is_none()).cloned();
            growing.columns.insert(column, Kept { made_from, child, grown, watched, far: far_off, gone, set });
        }
    }
    // An Obj of a column nothing keeps any more, as after a world was copied: gone.
    let kept: HashSet<Entity> =
        growth.sources.values().flat_map(|growing| growing.columns.values().filter_map(|kept| kept.child)).collect();
    let strays: Vec<Entity> = world
        .query_ref::<(Entity, &GrownColumn)>()
        .into_iter()
        .map(|(entity, _)| entity)
        .filter(|entity| !kept.contains(entity))
        .collect();
    for stray in strays {
        world.despawn(stray);
    }
    world.insert_resource(growth);
}

/// What a growing Foliage shows as where its plants are not drawn, far off
/// or on a map (see `voxear_terrain::blocks::grow::Woods`): where it has a
/// far block its world holds and its world is one without end, as tall and
/// as wide as its mesh at its middle size. As a wood it begins in the
/// first ring past those its plants are drawn on. None where nothing
/// draws, as on a server: neither is seen there.
fn woods_of(world: &World, content: &mut Content, grower: &Grower, asset: &BlockWorldAsset) -> Option<Woods> {
    let far_block = grower.data.far_block.as_deref()?;
    let endless = asset.file.generate.as_ref()?.endless?;
    let block = content.block(far_block).and_then(|(guid, _)| asset.drawn.number_of(guid))?;
    let boxes = world.resource::<crate::mesh_boxes::MeshBoxes>()?;
    let (least, most) = boxes.of(grower.look.foliage.as_ref()?.mesh)?;
    let rule = grower.data.rule(|block| {
        let (guid, _) = content.block(block)?;
        asset.drawn.number_of(guid)
    });
    let size = (rule.scale[0] + rule.scale[1]) * 0.5;
    let wide = (most.x - least.x).max(most.z - least.z);
    let from = Woods::first_past(grower.draw_distance, endless.distance);
    Some(Woods { rule, block, height: most.y.max(0.0) * size, width: wide * size, from })
}

/// What the Foliage growing under the Block World on `entity` show as in
/// its far view, each a wood (see [`woods_of`]): those whose plants stand
/// thick enough to show so far off.
pub fn far_built(world: &World, content: &mut Content, entity: Entity) -> Vec<Arc<dyn FarBuilt>> {
    let woods = on_maps(world, content, entity).into_iter().filter(Woods::shows);
    woods.map(|woods| Arc::new(woods) as Arc<dyn FarBuilt>).collect()
}

/// The Foliage growing under the Block World on `entity` that a map of it
/// shows, a kind each (see [`woods_of`]): every one with a far block,
/// however thin it stands, as a map near enough shows each plant.
pub fn on_maps(world: &World, content: &mut Content, entity: Entity) -> Vec<Woods> {
    let Some(asset) = world.get::<BlockWorldAsset>(entity) else { return Vec::new() };
    let growers = growers(world).into_iter().filter(|grower| grower.world == entity);
    growers.filter_map(|grower| woods_of(world, content, &grower, asset)).collect()
}

/// Gives a column's Obj what its plants share with their Foliage.
fn dress(world: &mut World, child: Entity, look: &Look) {
    world.remove::<Foliage>(child);
    if let Some(foliage) = &look.foliage {
        world.insert(child, foliage.clone());
    }
    world.remove::<Layer>(child);
    if let Some(layer) = look.layer {
        world.insert(child, layer);
    }
    if let Some(plants) = world.get_mut::<Plants>(child) {
        (plants.trunk, plants.cut) = (look.trunk, look.cut.clone());
    }
}

/// A number that changes when what a rule's plants are grown from does: the
/// rule, the world it is under, how that world's ground is made and how
/// large its blocks are.
fn stamp_of(
    rule: &GrowRule,
    asset: &BlockWorldAsset,
    blocks: &BlockWorld,
    generator: Option<&WorldGen>,
    world: Entity,
) -> u64 {
    let generate = asset.file.generate.as_ref().map(|generate| (generate.position, generate.bottom));
    let text = format!(
        "{rule:?} {} {:?} {} {} {generate:?} {:?}",
        world.to_bits(),
        generator.map(|generator| generator.hash()),
        blocks.block_size,
        blocks.smooth,
        asset.guid,
    );
    voxear_terrain::layers::stable_hash(text.as_bytes())
}

/// How a starter plant grows on a world: how it is drawn, its trunk or
/// its solid box, where it grows, whether it keeps to the world's top
/// block, and the starter plants it keeps near to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarterGrowth {
    pub drawn: &'static str,
    pub trunk: Option<&'static str>,
    pub grown: &'static str,
    pub on_top: bool,
    pub near: &'static [StarterPlant],
}

/// The starter plants that are trees: what sticks, logs, stumps and
/// mushrooms keep near to.
pub const STARTER_TREES: [StarterPlant; 3] = [StarterPlant::Tree, StarterPlant::Pine, StarterPlant::Birch];

/// The starter plants that are large stones: what pebbles keep near to.
pub const STARTER_STONES: [StarterPlant; 2] = [StarterPlant::Rock, StarterPlant::Boulder];

/// The starter pebbles again, as they lie along a shore: within a few
/// metres of the sea's height, on any ground, in patches.
pub const SHORE_PEBBLES: StarterGrowth = StarterGrowth {
    drawn: "shadow_distance = 0.0\nimpostors = \"off\"\n",
    trunk: None,
    grown: "height = [-1.0, 2.5]\nspacing = 1.6\nseparation = 0.6\nchance = 0.5\npatches = 7.0\nslope = 40.0\n\
            seed = 19\nscale = [0.7, 1.5]\naligned = false\n",
    on_top: false,
    near: &[],
};

/// How a starter plant is drawn, its trunk, where it grows, and what it
/// keeps near to: what a Foliage growing it starts with. Grass is thick
/// and near, flowers in patches by climate, trees far apart where it is
/// wet enough, stones on any ground; sticks, logs, stumps and mushrooms
/// near trees, and pebbles near larger stones.
pub fn starter_growth(plant: StarterPlant) -> StarterGrowth {
    const SMALL: &str = "shadow_distance = 0.0\nimpostors = \"off\"\n";
    const MIDDLING: &str = "draw_distance = 100.0\nshadow_distance = 30.0\n";
    const TALL: &str = "draw_distance = 400.0\nfade_distance = 40.0\nshadow_distance = 80.0\n";
    const TRUNK: &str = "radius = 0.3\nheight = 3.5\n";
    // Where every kind of coral grows: in patches of warm, shallow sea, the same patches for each.
    macro_rules! reef {
        ($rest:literal) => {
            concat!(
                "under_water = true\nheight = [-14.0, -2.0]\ntemperature = [20.0, 45.0]\npatches = 30.0\nslope = 60.0\n\
                 seed = 8\n",
                $rest
            )
        };
    }
    let growth = |drawn, trunk, grown, on_top| StarterGrowth { drawn, trunk, grown, on_top, near: &[] };
    let near = |growth: StarterGrowth, near| StarterGrowth { near, ..growth };
    match plant {
        StarterPlant::Tuft => growth(SMALL, None, "spacing = 1.0\nseparation = 0.0\nchance = 0.6\nslope = 40.0\n", true),
        StarterPlant::Flower => growth(SMALL, None, "spacing = 2.0\nseparation = 1.0\npatches = 14.0\nseed = 1\n", true),
        StarterPlant::Bush => growth(
            MIDDLING,
            None,
            "spacing = 7.0\nseparation = 3.0\nchance = 0.4\nslope = 35.0\nseed = 2\nscale = [0.8, 1.4]\n",
            true,
        ),
        // A rock is solid: a box a little inside its rounded shape.
        StarterPlant::Rock => growth(
            "draw_distance = 120.0\nshadow_distance = 40.0\n",
            Some("box = [1.35, 0.65, 1.05]\n"),
            "spacing = 9.0\nseparation = 3.0\nchance = 0.35\nslope = 70.0\nseed = 3\nscale = [0.6, 1.8]\n",
            false,
        ),
        StarterPlant::Tree => growth(
            TALL,
            Some(TRUNK),
            "humidity = [0.45, 1.0]\nspacing = 7.0\nseparation = 4.0\nchance = 0.6\nslope = 25.0\nseed = 4\n\
             scale = [0.8, 1.3]\n",
            true,
        ),
        StarterPlant::Pine => growth(
            TALL,
            Some(TRUNK),
            "temperature = [-15.0, 12.0]\nspacing = 6.0\nseparation = 3.5\nchance = 0.6\nseed = 5\n\
             scale = [0.8, 1.4]\n",
            true,
        ),
        StarterPlant::TallGrass => growth(
            SMALL,
            None,
            "humidity = [0.3, 1.0]\nspacing = 1.5\nseparation = 0.5\nchance = 0.4\npatches = 9.0\nslope = 35.0\n\
             seed = 6\nscale = [0.8, 1.3]\n",
            true,
        ),
        StarterPlant::Daisy => growth(
            SMALL,
            None,
            "temperature = [4.0, 26.0]\nhumidity = [0.25, 0.75]\nspacing = 1.5\nseparation = 0.8\npatches = 11.0\n\
             seed = 7\n",
            true,
        ),
        StarterPlant::Bluebell => growth(
            SMALL,
            None,
            "temperature = [2.0, 20.0]\nhumidity = [0.5, 1.0]\nspacing = 1.5\nseparation = 0.8\npatches = 13.0\n\
             seed = 8\n",
            true,
        ),
        StarterPlant::Fern => growth(
            "draw_distance = 80.0\nshadow_distance = 0.0\nimpostors = \"off\"\n",
            None,
            "humidity = [0.55, 1.0]\nspacing = 3.0\nseparation = 1.5\nchance = 0.5\npatches = 16.0\nseed = 9\n\
             scale = [0.8, 1.4]\n",
            true,
        ),
        // Where it is hot and dry, on whatever lies there.
        StarterPlant::DryBush => growth(
            MIDDLING,
            None,
            "temperature = [14.0, 60.0]\nhumidity = [0.0, 0.32]\nspacing = 6.0\nseparation = 3.0\nchance = 0.45\n\
             slope = 35.0\nseed = 10\nscale = [0.8, 1.5]\n",
            false,
        ),
        StarterPlant::Mushroom => near(
            growth(
                SMALL,
                None,
                "temperature = [0.0, 20.0]\nhumidity = [0.45, 1.0]\nspacing = 3.0\nseparation = 1.0\nchance = 0.15\n\
                 seed = 11\nscale = [0.4, 0.9]\nwithin = 5.0\n",
                true,
            ),
            &STARTER_TREES,
        ),
        StarterPlant::Birch => growth(
            TALL,
            Some(TRUNK),
            "temperature = [2.0, 17.0]\nhumidity = [0.35, 0.85]\nspacing = 13.0\nseparation = 5.0\nchance = 0.4\n\
             slope = 25.0\nseed = 12\nscale = [0.8, 1.25]\n",
            true,
        ),
        StarterPlant::DeadTree => growth(
            "draw_distance = 300.0\nfade_distance = 30.0\nshadow_distance = 60.0\n",
            Some(TRUNK),
            "spacing = 34.0\nseparation = 10.0\nchance = 0.5\nslope = 30.0\nseed = 13\nscale = [0.8, 1.3]\n",
            false,
        ),
        // Solid, both: a stump is stepped onto and a log climbed over.
        StarterPlant::Stump => near(
            growth(
                MIDDLING,
                Some("box = [0.7, 0.45, 0.7]\n"),
                "spacing = 11.0\nseparation = 4.0\nchance = 0.5\nslope = 30.0\nseed = 14\nscale = [0.8, 1.4]\n\
                 within = 9.0\n",
                false,
            ),
            &STARTER_TREES,
        ),
        StarterPlant::Log => near(
            growth(
                "draw_distance = 120.0\nshadow_distance = 40.0\n",
                Some("box = [2.3, 0.6, 0.7]\n"),
                "spacing = 15.0\nseparation = 6.0\nchance = 0.55\nslope = 20.0\nseed = 15\nscale = [0.8, 1.3]\n\
                 aligned = false\nwithin = 9.0\n",
                false,
            ),
            &STARTER_TREES,
        ),
        StarterPlant::Stick => near(
            growth(
                SMALL,
                None,
                "spacing = 2.5\nseparation = 1.0\nchance = 0.45\nslope = 35.0\nseed = 16\nscale = [0.7, 1.4]\n\
                 aligned = false\n",
                false,
            ),
            &STARTER_TREES,
        ),
        StarterPlant::Pebbles => near(
            growth(
                SMALL,
                None,
                "spacing = 2.5\nseparation = 1.0\nchance = 0.5\nslope = 50.0\nseed = 17\nscale = [0.7, 1.5]\n\
                 aligned = false\nwithin = 5.0\n",
                false,
            ),
            &STARTER_STONES,
        ),
        StarterPlant::Boulder => growth(
            "draw_distance = 300.0\nfade_distance = 30.0\nshadow_distance = 80.0\n",
            Some("box = [3.0, 2.3, 2.8]\n"),
            "spacing = 42.0\nseparation = 12.0\nchance = 0.55\nslope = 60.0\nseed = 18\nscale = [0.7, 1.5]\n\
             aligned = false\n",
            false,
        ),
        // Plants of the sea's bed, on any ground under water: how deep, and how warm a sea.
        StarterPlant::Seagrass => growth(
            SMALL,
            None,
            "under_water = true\nheight = [-12.0, -1.0]\ntemperature = [1.0, 45.0]\nspacing = 1.0\nseparation = 0.0\n\
             chance = 0.5\npatches = 18.0\nslope = 40.0\nseed = 6\n",
            false,
        ),
        // Kelp stands eight metres tall, in water deeper than that, where the sea is cool.
        StarterPlant::Kelp => growth(
            "draw_distance = 160.0\nfade_distance = 30.0\nshadow_distance = 0.0\nimpostors = \"off\"\n",
            None,
            "under_water = true\nheight = [-60.0, -9.0]\ntemperature = [-2.0, 18.0]\nspacing = 3.0\nseparation = 1.5\n\
             chance = 0.7\npatches = 40.0\nslope = 50.0\nseed = 7\nscale = [0.7, 1.1]\n",
            false,
        ),
        // Coral in warm shallows, every kind in the same reefs.
        StarterPlant::BranchCoral => {
            growth(SEA_BED, None, reef!("spacing = 4.0\nseparation = 2.0\nchance = 0.5\nscale = [0.7, 1.3]\n"), false)
        }
        StarterPlant::BrainCoral => {
            growth(SEA_BED, None, reef!("spacing = 5.0\nseparation = 2.0\nchance = 0.4\nscale = [0.6, 1.6]\n"), false)
        }
        StarterPlant::TableCoral => {
            growth(SEA_BED, None, reef!("spacing = 6.0\nseparation = 3.0\nchance = 0.35\nscale = [0.8, 1.5]\n"), false)
        }
        StarterPlant::FanCoral => {
            growth(SEA_BED, None, reef!("spacing = 5.0\nseparation = 2.0\nchance = 0.4\nscale = [0.7, 1.4]\n"), false)
        }
        StarterPlant::TubeSponge => growth(
            SEA_BED,
            None,
            "under_water = true\nheight = [-30.0, -3.0]\ntemperature = [8.0, 45.0]\nspacing = 6.0\nseparation = 2.0\n\
             chance = 0.3\npatches = 25.0\nslope = 60.0\nseed = 9\nscale = [0.6, 1.3]\n",
            false,
        ),
        StarterPlant::Clam => growth(
            SEA_BED,
            None,
            "under_water = true\nheight = [-20.0, -2.0]\ntemperature = [4.0, 45.0]\nspacing = 8.0\nseparation = 3.0\n\
             chance = 0.3\nseed = 10\nscale = [0.6, 1.2]\n",
            false,
        ),
    }
}

/// How a small thing of the sea's bed is drawn.
const SEA_BED: &str = "draw_distance = 90.0\nfade_distance = 20.0\nshadow_distance = 0.0\nimpostors = \"off\"\n";

/// What a Foliage that grows names of its world and its project: the
/// blocks its plants stand on, the block a wood of them shows as far off,
/// and what they keep near to, the models other Foliage grow and blocks.
#[derive(Clone, Copy, Debug, Default)]
pub struct GrownNames<'a> {
    pub top: Option<&'a str>,
    /// More blocks its plants stand on, beside the top one: the tops of the
    /// world's biomes that things grow on, as moss and a forest's floor.
    pub soils: &'a [String],
    pub far: Option<&'a str>,
    pub near_plants: &'a [String],
    pub near_blocks: &'a [String],
}

/// A Foliage that grows by itself, as a scene writes it under an Obj: its
/// `[obj.foliage]` section, drawing `mesh` with `material`, growing on
/// block `top` where one is given, and showing far off as block `far`. For a starter plant's model it is drawn
/// and grown as suits the plant (see [`starter_growth`]).
pub fn foliage_text(
    mesh: &str,
    material: Option<&str>,
    starter: Option<StarterPlant>,
    (top, far): (Option<&str>, Option<&str>),
) -> String {
    let growth = starter.map(starter_growth);
    grown_text(mesh, material, growth.as_ref(), &GrownNames { top, far, ..GrownNames::default() })
}

/// As [`foliage_text`], growing as `growth` says where there is one, and
/// keeping near to what `names` names where the growth keeps near to
/// anything.
pub fn grown_text(mesh: &str, material: Option<&str>, growth: Option<&StarterGrowth>, names: &GrownNames) -> String {
    let string = |text: &str| toml::Value::String(text.to_string()).to_string();
    let list = |names: &[String]| names.iter().map(|name| string(name)).collect::<Vec<_>>().join(", ");
    let (drawn, trunk, grown, on_top) = growth.map_or(("", None, "", true), |g| (g.drawn, g.trunk, g.grown, g.on_top));
    let mut text = format!("[obj.foliage]\nmesh = {}\n", string(mesh));
    if let Some(material) = material {
        text += &format!("material = {}\n", string(material));
    }
    text += drawn;
    if let Some(trunk) = trunk {
        text += &format!("[obj.foliage.trunk]\n{trunk}");
    }
    text += &format!("[obj.foliage.grow]\n{grown}");
    if let Some(top) = names.top.filter(|_| on_top) {
        let more = names.soils.iter().filter(|soil| soil.as_str() != top).map(|soil| format!(", {}", string(soil)));
        text += &format!("on = [{}{}]\n", string(top), more.collect::<String>());
    }
    if let Some(far) = names.far {
        text += &format!("far_block = {}\n", string(far));
    }
    if growth.is_some_and(|growth| !growth.near.is_empty()) {
        if !names.near_plants.is_empty() {
            text += &format!("near_plants = [{}]\n", list(names.near_plants));
        }
        if !names.near_blocks.is_empty() {
            text += &format!("near_blocks = [{}]\n", list(names.near_blocks));
        }
    }
    text
}

/// Whether every growing Foliage has the plants of every column it wants:
/// nothing left to make until the world is seen from elsewhere or more of
/// its ground is made.
pub fn settled(world: &World) -> bool {
    world.resource::<Growth>().is_none_or(|growth| growth.sources.values().all(|growing| growing.settled))
}

/// The columns Foliage `source` has grown plants on, each with the Obj that
/// holds them, in order.
pub fn columns_of(world: &World, source: Entity) -> Vec<([i32; 2], Entity)> {
    let mut columns: Vec<([i32; 2], Entity)> = world
        .query_ref::<(Entity, &GrownColumn)>()
        .into_iter()
        .filter(|(_, grown)| grown.source == source)
        .map(|(entity, grown)| (grown.column, entity))
        .collect();
    columns.sort_unstable();
    columns
}

/// Every plant Foliage `source` has grown that still stands, in metres from
/// its Block World's corner, by column and then number.
pub fn standing(world: &World, source: Entity) -> Vec<Plant> {
    let columns = columns_of(world, source);
    let plants = columns.into_iter().filter_map(|(_, entity)| world.get::<Plants>(entity));
    plants.flat_map(|plants| plants.standing().map(|(_, plant)| *plant).collect::<Vec<_>>()).collect()
}

/// The Foliage an Obj's plants were grown for, where the Obj is one of the
/// engine's own holding a column's plants.
pub fn source_of(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<GrownColumn>(entity).map(|grown| grown.source)
}
