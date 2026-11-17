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
    // Laid as its chunks lay it: over the floor under its ground, its top in half blocks where it says.
    let halved = voxear_terrain::blocks::generate::halved(generate) && !file.smooth;
    let laid = (size, generate.position, generate.lowest_open(), halved);
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

/// On a Portal the engine stands in the gate of a dungeon that is in a
/// dimension of its own: which dungeon it leads into. The Portal leads
/// nowhere until the game makes that dimension, as a camera comes near
/// enough to see through it (`Game::link_dimensions`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DungeonGate {
    /// The world of blocks the gate is built in.
    pub world: Entity,
    /// Which of that world's structures the dungeon is.
    pub structure: usize,
    /// Its site, in metres in the generator's world, and the site's seed.
    pub site: [f32; 2],
    pub seed: u32,
}

impl voxear_ecs::Component for DungeonGate {}

impl DungeonGate {
    /// What tells this dungeon's dimension from every other's: the world it
    /// stands in, which dungeon of it, and its site.
    pub fn key(&self, world_asset: voxear_assets::AssetGuid) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        world_asset.hash(&mut hasher);
        self.structure.hash(&mut hasher);
        self.site.map(f32::to_bits).hash(&mut hasher);
        hasher.finish()
    }
}

/// On the world of blocks of a dungeon's own dimension: the gate it was made
/// from, for players' machines in a networked game to make their own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DungeonFrom(pub DungeonGate);

impl voxear_ecs::Component for DungeonFrom {}

/// On the world of blocks of a dungeon's own dimension: the dungeon as it
/// was laid out there, in that world's blocks.
#[derive(Clone, Debug)]
pub struct DungeonInside(pub std::sync::Arc<DungeonLayout>);

impl voxear_ecs::Component for DungeonInside {}

/// On the Portal in a dungeon's own dimension that leads back out through
/// its gate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DungeonWayOut;

impl voxear_ecs::Component for DungeonWayOut {}

/// How far from where the game is seen from a dungeon's gate has its Portal
/// stood in it, in metres; and how much further it is kept, so walking back
/// and forth at the edge makes nothing twice.
const GATES_WITHIN: f32 = 160.0;
const GATES_KEPT_PAST: f32 = 64.0;

/// How far the viewer moves before the gates round it are looked for again,
/// in metres, and how many ticks go by at most.
const GATES_LOOKED_EVERY: f32 = 16.0;
const GATES_TICKS: u32 = 60;

/// How large a gate's Portal is, in metres: its width and height.
const GATE_PORTAL: [f32; 2] = [3.0, 4.0];

/// When the gates were last looked for, and from where.
#[derive(Default)]
struct GatesLooked {
    from: Vec<(Vec3, voxear_scene::DimensionId)>,
    ticks: u32,
}

/// Stands a Portal in the middle of the landing of the gate of each dungeon in a dimension
/// of its own within reach of where the game is seen from, and on a server
/// of each player, in the worlds of blocks of their dimension, and takes away
/// those well past them. While the game plays.
pub fn stand_gates(world: &mut World, content: &mut Content) {
    let mut places: Vec<(Vec3, voxear_scene::DimensionId)> =
        crate::terrain::viewer(world).map(|viewer| (viewer, crate::dimensions::seen_from(world))).into_iter().collect();
    places.extend(crate::net::player_places(world).into_iter().map(|(_, at, dimension)| (at, dimension)));
    if places.is_empty() {
        return;
    }
    let mut looked = world.remove_resource::<GatesLooked>().unwrap_or_default();
    looked.ticks += 1;
    let moved = looked.from.len() != places.len()
        || looked
            .from
            .iter()
            .zip(&places)
            .any(|(from, at)| from.1 != at.1 || from.0.distance(at.0) > GATES_LOOKED_EVERY);
    if !moved && looked.ticks < GATES_TICKS {
        world.insert_resource(looked);
        return;
    }
    (looked.from, looked.ticks) = (places.clone(), 0);
    world.insert_resource(looked);
    let worlds: Vec<(Entity, voxear_scene::DimensionId)> = world
        .query_ref::<(Entity, &BlockWorld, &BlockWorldAsset)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, ..)| (entity, voxear_scene::dimension_of(world, entity)))
        .filter(|(_, dimension)| places.iter().any(|(_, at)| at == dimension))
        .filter(|(entity, _)| {
            let Some(asset) = world.get::<BlockWorldAsset>(*entity) else { return false };
            let built = asset.file.generate.as_ref().and_then(|generate| generate.built.as_deref());
            built.is_some_and(|built| {
                built.list.iter().any(|structure| {
                    matches!(&structure.blocks, voxear_terrain::blocks::structure::StructureBlocks::Dungeon(dungeon)
                        if dungeon.place == voxear_terrain::blocks::structure::DungeonPlace::OwnDimension)
                })
            })
        })
        .collect();
    let standing: Vec<(Entity, DungeonGate, Vec3)> = world
        .query_ref::<(Entity, &DungeonGate, &Transform)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, gate, transform)| (entity, *gate, transform.position))
        .collect();
    let mut wanted: Vec<DungeonGate> = Vec::new();
    let near = places.iter().map(|(at, dimension)| (*at, *dimension));
    let each: Vec<(Entity, voxear_scene::DimensionId, Vec3)> = worlds
        .iter()
        .flat_map(|&(entity, here)| {
            near.clone().filter(move |(_, d)| *d == here).map(move |(at, _)| (entity, here, at))
        })
        .collect();
    for (entity, seen, viewer) in each {
        for (gate, pose) in gates_of(world, content, entity, viewer) {
            if wanted.contains(&gate) {
                continue;
            }
            wanted.push(gate);
            if standing.iter().any(|(_, kept, _)| *kept == gate) {
                continue;
            }
            let portal = voxear_render::Portal { size: GATE_PORTAL.into(), ..voxear_render::Portal::default() };
            let name = voxear_scene::Name("Dungeon Gate".into());
            let transform = Transform::new(pose.0, pose.1, Vec3::ONE);
            let made = world.spawn((name, transform, portal, gate, voxear_scene::EditorOnly));
            voxear_scene::set_dimension(world, made, seen);
        }
    }
    // Those not wanted now and well past where they are stood go.
    for (entity, gate, at) in standing {
        let far = places.iter().all(|(from, _)| at.distance(*from) > GATES_WITHIN + GATES_KEPT_PAST);
        if !wanted.contains(&gate) && far {
            world.insert(entity, Destroying);
        }
    }
}

/// The gates of the dungeons in dimensions of their own of the world of
/// blocks on `entity` within reach of `viewer`, and where each one's Portal
/// stands, in the world: in the middle of its landing, facing the stair down.
fn gates_of(
    world: &World,
    content: &mut Content,
    entity: Entity,
    viewer: Vec3,
) -> Vec<(DungeonGate, (Vec3, voxear_core::math::Quat))> {
    use voxear_terrain::blocks::structure::{DungeonPlace, StructureBlocks};
    let Some(asset) = world.get::<BlockWorldAsset>(entity) else { return Vec::new() };
    let file = asset.file.clone();
    let Some(generate) = file.generate.as_ref() else { return Vec::new() };
    let Some(built) = generate.built.as_deref() else { return Vec::new() };
    let Some(landed) = crate::blocks::generator(content, &file) else { return Vec::new() };
    let generator = voxear_terrain::blocks::generate::ground_generator(generate, &landed, file.block_size.max(0.01));
    let Some(transform) = world.get::<Transform>(entity) else { return Vec::new() };
    let corner = voxear_terrain::corner(transform, world.get::<WorldTransform>(entity));
    let size = file.block_size;
    let [px, pz] = generate.position;
    let biomes = generate.placed.as_deref().filter(|placed| !placed.biomes.is_empty());
    let halved = voxear_terrain::blocks::generate::halved(generate) && !file.smooth;
    let laid = (size, generate.position, generate.lowest_open(), halved);
    let near = [viewer.x - corner.x + px, viewer.z - corner.z + pz];
    let found = dungeons_near(built, &generator, biomes, laid, near, GATES_WITHIN);
    found
        .into_iter()
        .filter(|(which, ..)| {
            matches!(&built.list[*which].blocks, StructureBlocks::Dungeon(dungeon) if dungeon.place == DungeonPlace::OwnDimension)
        })
        .filter_map(|(which, site, layout)| {
            let landing = layout.spaces.first()?;
            let into = layout.entrance.into;
            // In the middle of the landing, clear of its columns, facing back up the stair: its sill half a block
            // under the landing's floor, so one crosses it whether the floor there is laid whole or in half steps.
            let along = Vec3::new(into[0] as f32, 0.0, into[1] as f32).normalize_or_zero();
            let foot = Vec3::new(landing.at[0] as f32 + 0.5, landing.at[1] as f32 + 0.5, landing.at[2] as f32 + 0.5);
            let at = corner + foot * size;
            let facing = -along;
            let turn = voxear_core::math::Quat::from_rotation_y(facing.x.atan2(facing.z));
            let gate = DungeonGate { world: entity, structure: which, site: site.at, seed: site.seed };
            Some((gate, (at, turn)))
        })
        .collect()
}

/// Takes away the changes of `edits`, of a world of blocks made from `file`
/// and written before dungeons went into dimensions of their own, that lie in
/// the spaces and ways of a dungeon that was under its land then and is in a
/// dimension of its own now: rock there now, which they would leave holes
/// and lamps in. Returns how many blocks' changes went.
pub fn drop_old_dungeon_edits(
    content: &mut Content,
    file: &voxear_terrain::blocks::file::BlockFile,
    edits: &mut voxear_terrain::blocks::edits::BlockEdits,
) -> usize {
    use voxear_terrain::blocks::structure::{DungeonPlace, StructureBlocks};
    let Some(generate) = file.generate.as_ref() else { return 0 };
    let Some(built) = generate.built.as_deref() else { return 0 };
    let moved = |blocks: &StructureBlocks| matches!(blocks, StructureBlocks::Dungeon(d) if d.place == DungeonPlace::OwnDimension);
    if edits.chunks.is_empty() || !built.list.iter().any(|structure| moved(&structure.blocks)) {
        return 0;
    }
    let Some(landed) = crate::blocks::generator(content, file) else { return 0 };
    let generator = voxear_terrain::blocks::generate::ground_generator(generate, &landed, file.block_size.max(0.01));
    // The dungeons as they were: under the land.
    let mut before = built.clone();
    for structure in &mut before.list {
        if let StructureBlocks::Dungeon(dungeon) = &mut structure.blocks
            && dungeon.place == DungeonPlace::OwnDimension
        {
            dungeon.place = DungeonPlace::InTheLand;
        }
    }
    let size = file.block_size.max(0.01);
    let biomes = generate.placed.as_deref().filter(|placed| !placed.biomes.is_empty());
    let halved = voxear_terrain::blocks::generate::halved(generate) && !file.smooth;
    let laid = (size, generate.position, generate.lowest_open(), halved);
    // Every space and way, in blocks, of each dungeon that reaches a chunk column changed.
    let side = voxear_terrain::blocks::chunk::CHUNK as i32;
    let mut columns: Vec<[i32; 2]> = edits.chunks.keys().map(|key| [key[0], key[2]]).collect();
    columns.sort_unstable();
    columns.dedup();
    let mut boxes: Vec<([i32; 3], [i32; 3])> = Vec::new();
    let mut seen: Vec<(usize, [u32; 2])> = Vec::new();
    for column in columns {
        let middle = [(column[0] * side + side / 2) as f32 * size, (column[1] * side + side / 2) as f32 * size];
        let near = [middle[0] + generate.position[0], middle[1] + generate.position[1]];
        for (which, site, layout) in dungeons_near(&before, &generator, biomes, laid, near, side as f32 * size) {
            if !moved(&built.list[which].blocks) || seen.contains(&(which, site.at.map(f32::to_bits))) {
                continue;
            }
            seen.push((which, site.at.map(f32::to_bits)));
            for space in layout.spaces.iter().filter(|space| space.height >= 0) {
                let (low, high) = (space.low(), space.high());
                boxes.push(([low[0] - 1, space.bottom() - 1, low[1] - 1], [high[0] + 1, space.top() + 1, high[1] + 1]));
            }
            for way in &layout.ways {
                let ys = way.line.iter().map(|point| (point[2] / 2.0) as i32);
                let (bottom, top) = (ys.clone().min().unwrap_or(0), ys.max().unwrap_or(0));
                boxes.push((
                    [way.low[0] - 1, bottom - 2, way.low[1] - 1],
                    [way.high[0] + 1, top + way.height + 2, way.high[1] + 1],
                ));
            }
        }
    }
    if boxes.is_empty() {
        return 0;
    }
    let inside = |at: [i32; 3]| boxes.iter().any(|(low, high)| (0..3).all(|i| at[i] >= low[i] && at[i] <= high[i]));
    edits.retain_blocks(|at| !inside(at))
}
