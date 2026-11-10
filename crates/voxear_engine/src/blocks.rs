//! Worlds of blocks in a game (the Terrain module).
//!
//! Each Block World loaded from a `.blockworld` asset carries a
//! [`BlockWorldAsset`]: which asset it is, and its file as the game has it
//! now. Its blocks are made from its file's ground as it loads, the same on
//! every machine, and the editor's changes since are made again from the
//! `.blocks` file beside it, which keeps only the chunks they changed. The
//! editor changes a world's file and blocks in the world; [`keep`] keeps
//! them in the game's content as the asset's, and saving the level writes
//! each changed asset's files ([`save`]).

use std::path::Path;

use anyhow::{Context, Result};
use std::sync::Arc;
use voxear_assets::AssetGuid;
use voxear_ecs::{Component, World};
use voxear_render::{BlockLook, Renderer};
use voxear_scene::Destroying;

use voxear_terrain::{
    blocks::{
        BlockMap, BlockWorld,
        edits::{BlockEdits, blocks_file},
        file::BlockFile,
    },
    generate::WorldGen,
};

use crate::{Content, MaterialData, MaterialSource, scene_file::default_material};

/// A Block World's `.blockworld` asset: its file as the game has it now,
/// and the ground its file made, from which its blocks' changes are what
/// differs (its chunks shared with the world's wherever nothing changed).
#[derive(Clone, Debug)]
pub struct BlockWorldAsset {
    pub guid: AssetGuid,
    pub file: BlockFile,
    pub base: Arc<BlockMap>,
    /// For a world without end, the changes kept for its chunks not made
    /// now (see `voxear_terrain::blocks::stream`).
    pub pending: Arc<BlockEdits>,
}

impl Component for BlockWorldAsset {}

impl PartialEq for BlockWorldAsset {
    fn eq(&self, other: &Self) -> bool {
        self.guid == other.guid
            && self.file == other.file
            && (Arc::ptr_eq(&self.base, &other.base) || self.base == other.base)
            && (Arc::ptr_eq(&self.pending, &other.pending) || self.pending == other.pending)
    }
}

/// The world generator `file`'s ground is made by, where it is generated
/// and its generator can be read.
pub fn generator(content: &mut Content, file: &BlockFile) -> Option<Arc<WorldGen>> {
    let reference = file.generate.as_ref()?.world.as_ref()?;
    content.world_gen(reference)
}

/// The ground `file` makes, by its generator where it has one.
pub fn ground(content: &mut Content, file: &BlockFile) -> BlockMap {
    let generator = generator(content, file);
    file.make_ground(generator.as_deref())
}

/// The world of blocks made on `base`, `file`'s ground, with `edits` made
/// to it, each chunk's faces worked out on every core.
pub fn make(file: &BlockFile, base: &Arc<BlockMap>, edits: Option<&BlockEdits>) -> BlockWorld {
    let (_, faces) = file.faces();
    let mut blocks = (**base).clone();
    if let Some(edits) = edits {
        edits.apply(&mut blocks);
    }
    let mut world = BlockWorld::made(blocks, &faces, file.block_size, file.smooth);
    world.planet = file.generate.as_ref().and_then(|generate| generate.planet).map(|planet| planet.radius);
    world
}

/// How a world of blocks looks: each way its faces are drawn, with its
/// material where it names one and else its colour, a picture a block.
pub fn look(content: &mut Content, renderer: &mut Renderer, file: &BlockFile) -> Result<BlockLook> {
    let fallback = content.material(renderer, &MaterialSource::Inline(default_material()))?;
    let (looks, _) = file.faces();
    let mut surfaces = Vec::with_capacity(looks.len());
    for face in looks {
        let named = face.material.as_ref().and_then(|reference| {
            let source = MaterialSource::Asset(content.parse(reference).ok()?);
            content.material(renderer, &source).map_err(|error| tracing::warn!("a block's material: {error:#}")).ok()
        });
        let material = match named {
            Some(material) => material,
            None => {
                let colored = MaterialData { color: face.color, ..default_material() };
                content.material(renderer, &MaterialSource::Inline(colored))?
            }
        };
        surfaces.push(voxear_render::SurfaceLook {
            material,
            size: file.block_size,
            sharpness: 0.0,
            triplanar: true,
            tracks: 0.0,
            fills_in: 0.0,
            liquid: Default::default(),
            glitter: false,
        });
    }
    Ok(BlockLook { material: fallback, surfaces: surfaces.into() })
}

/// Keeps every world of blocks' file and blocks in `content` as its
/// asset's, as the world has them now, marking those that changed to be
/// written.
pub fn keep(world: &World, content: &mut Content) {
    for (asset, blocks) in world.query_ref::<(&BlockWorldAsset, &BlockWorld)>().without::<Destroying>() {
        content.keep_block_world(asset.guid, &asset.file, (&asset.base, &asset.pending), blocks);
    }
}

/// Writes each world of blocks changed since it was last written into the
/// project's folder: its `.blockworld`, and beside it its changes from its
/// ground in a `.blocks` file, or none where it has none; gives the paths
/// written.
pub fn save(content: &mut Content, project: &Path) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for (path, file, blocks) in content.take_unsaved_block_worlds() {
        std::fs::write(project.join(&path), file.to_text()).with_context(|| format!("saving {path}"))?;
        let edits = match file.generate.as_ref().and_then(|g| g.endless) {
            Some(_) => voxear_terrain::blocks::stream::all_changes(&blocks.base, &blocks.world.blocks, &blocks.pending),
            None => BlockEdits::between(&blocks.base, &blocks.world.blocks),
        };
        let changes = project.join(blocks_file(&path));
        if edits.is_empty() {
            if changes.is_file() {
                std::fs::remove_file(&changes).with_context(|| format!("removing {}", changes.display()))?;
            }
        } else {
            std::fs::write(&changes, edits.to_bytes()).with_context(|| format!("saving {}", changes.display()))?;
        }
        written.push(path);
    }
    Ok(written)
}

/// Each world of blocks without end's streamer, by its Obj.
#[derive(Default)]
struct Streams(std::collections::HashMap<voxear_ecs::Entity, voxear_terrain::blocks::stream::BlockStreamer>);

/// Brings each world of blocks without end's chunks round where it is seen
/// from: the editor's view where there is one (`terrain::TerrainViewer`), or
/// else the game's camera. Without a camera or a generator, left as it is.
pub fn stream(world: &mut World, content: &mut Content) {
    use voxear_scene::{Transform, WorldTransform};
    let endless: Vec<(voxear_ecs::Entity, BlockWorldAsset, voxear_core::math::Vec3)> = world
        .query_ref::<(voxear_ecs::Entity, &BlockWorld, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, _, asset, ..)| asset.file.generate.as_ref().is_some_and(|g| g.endless.is_some()))
        .map(|(entity, _, asset, local, placed)| (entity, asset.clone(), voxear_terrain::corner(local, placed)))
        .collect();
    if endless.is_empty() {
        world.remove_resource::<Streams>();
        return;
    }
    let Some(viewer) = crate::terrain::viewer(world) else { return };
    let mut streams = world.remove_resource::<Streams>().unwrap_or_default();
    streams.0.retain(|entity, _| endless.iter().any(|(e, ..)| e == entity));
    for (entity, mut asset, corner) in endless {
        let Some(generate) = asset.file.generate.clone() else { continue };
        let Some(far) = generate.endless else { continue };
        let Some(generator) = generator(content, &asset.file) else { continue };
        let (_, faces) = asset.file.faces();
        let making = voxear_terrain::blocks::stream::BlockMaking {
            file: asset.file.clone(),
            generate,
            world: generator,
            faces: faces.into(),
        };
        let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { continue };
        let camera = [viewer.x - corner.x, viewer.z - corner.z];
        let streamer = streams.0.entry(entity).or_default();
        let distances = (far.distance, far.far);
        if streamer.update(&mut blocks, &mut asset.base, &mut asset.pending, making, distances, camera) {
            world.insert(entity, blocks);
            world.insert(entity, asset);
        }
    }
    world.insert_resource(streams);
}

/// How far into its next step the worlds of blocks' water is, in seconds.
struct FlowClock(f32);

/// The most steps water takes in a frame, however long the frame: a long
/// one slows it rather than stalling the game.
const FLOW_STEPS_A_FRAME: u32 = 4;

/// Moves each world of blocks' water on (see `voxear_terrain::blocks::flow`):
/// `STEPS_A_SECOND` steps a second of `delta`, while any is flowing.
pub fn flow(world: &mut World, delta: f32) {
    use voxear_terrain::blocks::flow::STEPS_A_SECOND;
    let flowing: Vec<voxear_ecs::Entity> = world
        .query_ref::<(voxear_ecs::Entity, &BlockWorld)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, blocks)| !blocks.flowing.is_empty())
        .map(|(entity, _)| entity)
        .collect();
    if flowing.is_empty() {
        world.remove_resource::<FlowClock>();
        return;
    }
    let each = 1.0 / STEPS_A_SECOND;
    let since = world.remove_resource::<FlowClock>().map_or(0.0, |clock| clock.0) + delta.max(0.0);
    let steps = ((since / each) as u32).min(FLOW_STEPS_A_FRAME);
    world.insert_resource(FlowClock((since - steps as f32 * each).min(each)));
    for entity in flowing {
        if let Some(blocks) = world.get_mut::<BlockWorld>(entity) {
            for _ in 0..steps {
                blocks.flow();
            }
        }
    }
}

/// Whether every world of blocks without end has made all its chunks round
/// where it is seen from.
pub fn streamed(world: &World) -> bool {
    world.resource::<Streams>().is_none_or(|streams| streams.0.values().all(|s| s.settled()))
}

#[cfg(test)]
mod tests {
    use voxear_terrain::blocks::{
        chunk::AIR,
        file::BlockKind,
        flow::{STEPS_A_SECOND, Water},
    };

    use super::*;

    #[test]
    fn water_placed_flows_as_the_game_ticks_the_same_however_its_frames_fall() {
        let mut file = BlockFile::default();
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let (_, faces) = file.faces();
        let mut blocks = BlockWorld::new(file.make_ground(None), &faces, 1.0);
        blocks.set_blocks([([10, 8, 10], 4)]);
        // Three seconds of frames, short ones and long ones: water runs a block each step either way.
        let run = |frames: &[f32]| {
            let mut world = World::new();
            let entity = world.spawn((blocks.clone(), voxear_scene::Transform::default()));
            for &delta in frames {
                flow(&mut world, delta);
            }
            world.get::<BlockWorld>(entity).unwrap().clone()
        };
        let short = run(&[0.03; 100]);
        let long = run(&[0.1; 30]);
        assert!(short.flowing.is_empty() && long.flowing.is_empty(), "settled within {} steps", 3.0 * STEPS_A_SECOND);
        assert_eq!(short.blocks, long.blocks);
        let water = |x: i32| (short.blocks.get([x, 8, 10]) == 4).then(|| Water::of(short.blocks.fill([x, 8, 10])));
        assert_eq!((water(17), water(18)), (Some(Water::Flowing { level: 1, up: 2 }), None), "seven blocks out");
        // A frame a second long takes no more than a few steps.
        let mut world = World::new();
        let entity = world.spawn((blocks.clone(), voxear_scene::Transform::default()));
        flow(&mut world, 1.0);
        let reached = world.get::<BlockWorld>(entity).unwrap().blocks.get([15, 8, 10]);
        assert_eq!(reached, AIR, "four steps at most in one frame");
    }
}
