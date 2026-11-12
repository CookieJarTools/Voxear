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
//! seen from have their plants, a few made each tick, nearest first.
//!
//! A plant whose block is dug away or built over is marked gone, and is
//! back once the ground is as it was made: worked out here from the blocks,
//! on every machine alike, so no save or message says which plants stand.

use std::{collections::HashMap, sync::Arc};

use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_render::{Foliage, StarterPlant};
use voxear_scene::{
    Destroying, EditorOnly, Layer, Parent, Plant, PlantSet, Plants, Switch, Switched, Transform, Trunk, WorldTransform,
};
use voxear_terrain::blocks::{
    BlockMap, BlockWorld,
    chunk::{CHUNK, Chunk},
    grow::{Ground, GrowRule, Grown, chunks_under, gone, grow},
    mesh::ChunkMesh,
};
use voxear_terrain::generate::WorldGen;

use crate::{Content, blocks::BlockWorldAsset, scene_file::FoliageSource};

/// How many columns' plants one Foliage has made in a tick, at most.
pub const COLUMNS_A_TICK: usize = 4;

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

/// One Foliage's grown plants: what they were grown from, and each column's.
struct Growing {
    stamp: u64,
    look: Look,
    columns: HashMap<[i32; 2], Kept>,
    /// Whether every column wanted has its plants.
    settled: bool,
}

/// Every growing Foliage's plants, by its Obj.
#[derive(Default)]
struct Growth(HashMap<Entity, Growing>);

/// A Foliage that grows, as a tick finds it: its Obj, its rule as written,
/// how its plants look, and the Block World it is under.
struct Grower {
    entity: Entity,
    data: crate::scene_file::GrowData,
    look: Look,
    draw_distance: f32,
    world: Entity,
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
            Some(Grower { entity, data, look, draw_distance, world: world_over(world, entity)? })
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

/// Grows every growing Foliage's plants on the columns of its Block World
/// near where the world is seen from (the editor's view, or else the game's
/// camera; everywhere where there is neither, as on a server), lets go of
/// those of columns that left, and marks the plants whose ground changed.
/// After [`crate::blocks::stream`], so ground made this tick has its plants
/// this tick.
pub fn update(world: &mut World, content: &mut Content) {
    voxear_core::profile_scope!("Grown Plants");
    let growers = growers(world);
    let mut growth = world.remove_resource::<Growth>().unwrap_or_default();
    // A Foliage that grows no longer, or is gone, has none.
    growth.0.retain(|source, growing| {
        let grows = growers.iter().any(|grower| grower.entity == *source);
        if !grows {
            growing.columns.values().for_each(|kept| despawn(world, kept));
        }
        grows
    });
    let viewer = crate::terrain::viewer(world);
    let mut all_tops: HashMap<Entity, HashMap<[i32; 2], ([i32; 3], Arc<Chunk>)>> = HashMap::new();
    // Each world read once a tick, however many of its Foliage grow.
    let mut worlds: HashMap<Entity, Option<(BlockWorldAsset, BlockWorld, Option<Arc<WorldGen>>)>> = HashMap::new();
    for grower in growers {
        let read = worlds.entry(grower.world).or_insert_with(|| {
            let asset = world.get::<BlockWorldAsset>(grower.world).cloned()?;
            let blocks = world.get::<BlockWorld>(grower.world).cloned()?;
            let generator = crate::blocks::generator(content, &asset.file);
            Some((asset, blocks, generator))
        });
        let Some((asset, blocks, generator)) = read.clone() else { continue };
        let rule = grower.data.rule(|block| {
            let (guid, _) = content.block(block)?;
            asset.drawn.number_of(guid)
        });
        let stamp = stamp_of(&rule, &asset, &blocks, generator.as_deref(), grower.world);
        let growing = growth.0.entry(grower.entity).or_insert_with(|| Growing {
            stamp,
            look: grower.look.clone(),
            columns: HashMap::new(),
            settled: false,
        });
        if growing.stamp != stamp {
            growing.columns.drain().for_each(|(_, kept)| despawn(world, &kept));
            growing.stamp = stamp;
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
        // Columns that left, went too far, or whose ground was made again, let go of.
        let made = blocks.columns.clone();
        let stays = |column: &[i32; 2], kept: &Kept| {
            let still_made = made.as_ref().is_none_or(|made| made.contains(column));
            let same_ground = kept
                .made_from
                .as_ref()
                .is_none_or(|(key, chunk)| asset.base.chunk(*key).is_some_and(|now| Arc::ptr_eq(now, chunk)));
            still_made && same_ground && near(*column, CHUNK as f32 * 0.5)
        };
        let leaving: Vec<[i32; 2]> =
            growing.columns.iter().filter(|(column, kept)| !stays(column, kept)).map(|(column, _)| *column).collect();
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
            let now = gone(&kept.grown, &blocks.blocks);
            let Some(plants) = kept.child.and_then(|child| world.get_mut::<Plants>(child)) else { continue };
            if *plants.removed != now {
                plants.removed = Arc::new(now);
            }
        }
        // Columns wanted that have no plants yet, nearest first, a few a tick.
        let tops = all_tops.entry(grower.world).or_insert_with(HashMap::new);
        let mut wanted: Vec<[i32; 2]> = match &made {
            Some(made) => made.iter().copied().collect(),
            None => {
                if tops.is_empty() {
                    *tops = self::tops(&asset.base);
                }
                tops.keys().copied().collect()
            }
        };
        wanted.retain(|column| !growing.columns.contains_key(column) && near(*column, 0.0));
        growing.settled = wanted.len() <= COLUMNS_A_TICK;
        if wanted.is_empty() {
            continue;
        }
        let nearness = |column: &[i32; 2]| from.map_or(0.0, |from| away(*column, from));
        wanted.sort_by(|a, b| nearness(a).total_cmp(&nearness(b)).then(a.cmp(b)));
        wanted.truncate(COLUMNS_A_TICK);
        if tops.is_empty() {
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
        for column in wanted {
            let grown = Arc::new(grow(&rule, &ground, column));
            let child = (!grown.is_empty()).then(|| {
                let plants: Vec<Plant> = grown.iter().map(|grown| grown.plant).collect();
                let removed: PlantSet = gone(&grown, &blocks.blocks);
                let plants = Plants { removed: Arc::new(removed), ..Plants::new(plants) };
                let marker = GrownColumn { source: grower.entity, column };
                let placed = WorldTransform(parent_matrix);
                let child =
                    world.spawn((Transform::IDENTITY, placed, Parent(grower.world), EditorOnly, marker, plants));
                dress(world, child, &growing.look);
                child
            });
            let watched = faces_of(&blocks, chunks_under(&grown));
            growing.columns.insert(column, Kept { made_from: tops.get(&column).cloned(), child, grown, watched });
        }
    }
    // An Obj of a column nothing keeps any more, as after a world was copied: gone.
    let kept: std::collections::HashSet<Entity> =
        growth.0.values().flat_map(|growing| growing.columns.values().filter_map(|kept| kept.child)).collect();
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

/// How a starter plant is drawn, its trunk, where it grows, and whether it
/// keeps to the world's top block: what a Foliage growing it starts with.
fn starter_settings(plant: StarterPlant) -> (&'static str, Option<&'static str>, &'static str, bool) {
    const SMALL: &str = "shadow_distance = 0.0\nimpostors = \"off\"\n";
    const TALL: &str = "draw_distance = 250.0\nfade_distance = 30.0\nshadow_distance = 80.0\n";
    const TRUNK: &str = "radius = 0.3\nheight = 3.5\n";
    match plant {
        StarterPlant::Tuft => (SMALL, None, "spacing = 1.0\nseparation = 0.0\nchance = 0.6\nslope = 40.0\n", true),
        StarterPlant::Flower => (SMALL, None, "spacing = 2.0\nseparation = 1.0\npatches = 14.0\nseed = 1\n", true),
        StarterPlant::Bush => (
            "draw_distance = 100.0\nshadow_distance = 30.0\n",
            None,
            "spacing = 7.0\nseparation = 3.0\nchance = 0.4\nslope = 35.0\nseed = 2\nscale = [0.8, 1.4]\n",
            true,
        ),
        StarterPlant::Rock => (
            "draw_distance = 120.0\nshadow_distance = 40.0\n",
            None,
            "spacing = 9.0\nseparation = 3.0\nchance = 0.35\nslope = 70.0\nseed = 3\nscale = [0.6, 1.8]\n",
            false,
        ),
        StarterPlant::Tree => (
            TALL,
            Some(TRUNK),
            "humidity = [0.45, 1.0]\nspacing = 7.0\nseparation = 4.0\nchance = 0.6\nslope = 25.0\nseed = 4\n\
             scale = [0.8, 1.3]\n",
            true,
        ),
        StarterPlant::Pine => (
            TALL,
            Some(TRUNK),
            "temperature = [-15.0, 12.0]\nspacing = 6.0\nseparation = 3.5\nchance = 0.6\nseed = 5\n\
             scale = [0.8, 1.4]\n",
            true,
        ),
    }
}

/// A Foliage that grows by itself, as a scene writes it under an Obj: its
/// `[obj.foliage]` section, drawing `mesh` with `material`, growing on
/// block `top` where one is given. For a starter plant's model it is drawn
/// and grown as suits the plant: grass thick and near, flowers in patches,
/// trees far apart where it is wet enough, rocks on any ground.
pub fn foliage_text(mesh: &str, material: Option<&str>, starter: Option<StarterPlant>, top: Option<&str>) -> String {
    let string = |text: &str| toml::Value::String(text.to_string()).to_string();
    let (drawn, trunk, grown, on_top) = starter.map_or(("", None, "", true), starter_settings);
    let mut text = format!("[obj.foliage]\nmesh = {}\n", string(mesh));
    if let Some(material) = material {
        text += &format!("material = {}\n", string(material));
    }
    text += drawn;
    if let Some(trunk) = trunk {
        text += &format!("[obj.foliage.trunk]\n{trunk}");
    }
    text += &format!("[obj.foliage.grow]\n{grown}");
    if let Some(top) = top.filter(|_| on_top) {
        text += &format!("on = [{}]\n", string(top));
    }
    text
}

/// Whether every growing Foliage has the plants of every column it wants:
/// nothing left to make until the world is seen from elsewhere or more of
/// its ground is made.
pub fn settled(world: &World) -> bool {
    world.resource::<Growth>().is_none_or(|growth| growth.0.values().all(|growing| growing.settled))
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
