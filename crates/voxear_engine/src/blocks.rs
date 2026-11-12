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
//!
//! A world can hold any block of the project, and numbers a block the
//! first time it holds one ([`block_number`]): its file keeps that
//! numbering, its palette, which only ever grows, so no number changes. How
//! its faces are drawn comes from its file with each entry naming a block
//! given that block's look ([`resolved`]), and a block saved since is taken
//! up by every world holding it ([`refresh`]).
//!
//! The editor's world keeps what it numbers in its asset, saved with the
//! level. A game's world numbers a block a script puts down the same way,
//! but nothing writes its file: such a number is that machine's own, so
//! what scripts changed is kept with which block each is
//! (`voxear_scripting::blocks`).

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
        block::{Liquid, MISSING},
        chunk::BlockId,
        edits::{BlockEdits, blocks_file},
        file::{BlockFile, BlockKind, MAX_BLOCKS, MAX_LOOKS},
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
    /// How many blocks its file had numbered as it was read. One numbered
    /// since by a game is numbered on this machine alone (see
    /// [`blocks_numbered_since`]).
    pub from_file: usize,
    /// Its file with each block it holds given that block's look (see
    /// [`resolved`]), as its world's faces and look were made from it, and
    /// the blocks' version that was at: how things are kept, and no part of
    /// what it is. [`refresh`] makes it anew once a block was saved since.
    pub drawn: Arc<Resolved>,
    pub derived: u64,
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

/// A world of blocks' file with every entry naming a block given that
/// block's look and name, and which block each is: what its faces are drawn
/// from (see [`resolved`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    file: BlockFile,
    /// The `.block` each entry is, by its place in the list; none for a kind
    /// of the world's own, a place left empty, or a block that is not there.
    guids: Vec<Option<AssetGuid>>,
}

impl std::ops::Deref for Resolved {
    type Target = BlockFile;

    fn deref(&self) -> &BlockFile {
        &self.file
    }
}

impl Resolved {
    /// The `.block` numbered `block`, where that is one.
    pub fn guid(&self, block: BlockId) -> Option<AssetGuid> {
        usize::from(block).checked_sub(1).and_then(|at| self.guids.get(at).copied()).flatten()
    }

    /// The number of the `.block` at `guid`, where it is listed.
    pub fn number_of(&self, guid: AssetGuid) -> Option<BlockId> {
        let at = self.guids.iter().position(|listed| *listed == Some(guid))?;
        Some((at + 1) as BlockId)
    }
}

/// `file` with each entry naming a block drawn as the block says and
/// called as its file is. One whose block cannot be found is drawn in a
/// colour loud enough to see.
pub fn resolved(content: &mut Content, file: &BlockFile) -> Resolved {
    let mut file = file.clone();
    let mut guids = Vec::with_capacity(file.blocks.len());
    for kind in &mut file.blocks {
        let Some(reference) = kind.block.clone().filter(|_| kind.is_live()) else {
            guids.push(None);
            continue;
        };
        match content.block(&reference) {
            Some((guid, block)) => {
                block.fill(kind);
                kind.name = content.block_name(guid).unwrap_or_default();
                guids.push(Some(guid));
            }
            None => {
                (kind.material, kind.top, kind.bottom, kind.water) = (None, None, None, false);
                (kind.name, kind.color) = ("Missing Block".into(), MISSING);
                guids.push(None);
            }
        }
    }
    // A block drawn with a material that is cut out is seen through, as leaves are: every face against it shows.
    for kind in file.blocks.iter_mut().filter(|kind| kind.is_live()) {
        let mut cut_out = |material: &Option<String>| {
            let guid = material.as_deref().and_then(|reference| content.parse(reference).ok()?.guid());
            guid.and_then(|guid| content.material_data(guid).ok())
                .is_some_and(|data| data.cutout.is_some_and(|c| c > 0.0))
        };
        kind.cutout = cut_out(&kind.material) || cut_out(&kind.top) || cut_out(&kind.bottom);
    }
    Resolved { file, guids }
}

/// `file` with the block at `guid` numbered after those it has, and that
/// file as it is drawn: none where the world can number no more, or draw no
/// more ways than it does (see `MAX_LOOKS`).
pub fn with_block(content: &mut Content, file: &BlockFile, guid: AssetGuid) -> Option<(BlockFile, Resolved)> {
    if file.blocks.len() >= MAX_BLOCKS {
        return None;
    }
    let mut file = file.clone();
    file.blocks.push(BlockKind::of(&guid.to_string()));
    let drawn = resolved(content, &file);
    (drawn.looks() <= MAX_LOOKS).then_some((file, drawn))
}

/// Says, once for each world, that the world of blocks at `world` has no
/// room for the block at `block`.
pub fn say_full(content: &mut Content, world: AssetGuid, block: AssetGuid) {
    if !content.block_world_full(world) {
        return;
    }
    let path = |guid| content.db().get(guid).map_or_else(|| guid.to_string(), |entry| entry.path.clone());
    let block = content.block_name(block).unwrap_or_else(|| path(block));
    tracing::warn!("{} is full: a world draws {MAX_LOOKS} looks at most. {block} was not added.", path(world));
}

/// The number of the block `reference` names in the world of blocks on
/// `entity`: the one it has, or, with `grow`, the next after those it has
/// numbered, which is the block's from now on. None where there is no such
/// block, the world is not an asset's, or it can draw no more.
pub fn block_number(
    world: &mut World,
    content: &mut Content,
    entity: voxear_ecs::Entity,
    reference: &str,
    grow: bool,
) -> Option<BlockId> {
    let (guid, _) = content.block(reference)?;
    let asset = world.get::<BlockWorldAsset>(entity)?;
    if let Some(known) = asset.drawn.number_of(guid) {
        return Some(known);
    }
    if !grow {
        return None;
    }
    let Some((file, drawn)) = with_block(content, &asset.file, guid) else {
        say_full(content, asset.guid, guid);
        return None;
    };
    // Its faces take the new block's on at their end, so nothing made is made again; its look is taken up by
    // `refresh`, which has the renderer.
    let (_, faces) = drawn.faces();
    let number = file.blocks.len() as BlockId;
    let asset = BlockWorldAsset { file, drawn: Arc::new(drawn), derived: u64::MAX, ..asset.clone() };
    world.get_mut::<BlockWorld>(entity)?.faces = faces.into();
    world.insert(entity, asset);
    Some(number)
}

/// The `.block` numbered `block` in the world of blocks on `entity`, as its
/// GUID: none for air, a kind of the world's own, or a world that is no
/// asset's.
pub fn block_guid(world: &World, entity: voxear_ecs::Entity, block: BlockId) -> Option<String> {
    Some(world.get::<BlockWorldAsset>(entity)?.drawn.guid(block)?.to_string())
}

/// The blocks the world of blocks on `entity` numbered since its file was
/// read, each with its number: those whose numbers are this machine's own
/// in a game.
pub fn blocks_numbered_since(world: &World, entity: voxear_ecs::Entity) -> Vec<(BlockId, String)> {
    let Some(asset) = world.get::<BlockWorldAsset>(entity) else { return Vec::new() };
    let since = asset.from_file + 1..=asset.drawn.blocks.len();
    since.filter_map(|at| Some((at as BlockId, asset.drawn.guid(at as BlockId)?.to_string()))).collect()
}

/// Whether `block` is the number of a block in the world of blocks on
/// `entity`: any number in a world that is no asset's.
pub fn block_listed(world: &World, entity: voxear_ecs::Entity, block: BlockId) -> bool {
    world.get::<BlockWorldAsset>(entity).is_none_or(|asset| asset.drawn.lists(block))
}

/// Every block of the project, by its GUID, in the order of their paths.
pub fn every_block(content: &Content) -> Vec<String> {
    let blocks = content.db().entries().filter(|entry| entry.kind == voxear_assets::AssetKind::Block);
    let mut found: Vec<_> = blocks.collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.into_iter().map(|entry| entry.guid.to_string()).collect()
}

/// What the block `reference` names says, for scripts, as JSON: its GUID,
/// its name, its look and its rules, what it names as its file does.
pub fn block_json(content: &mut Content, reference: &str) -> Result<String> {
    let (guid, block) = content.block(reference).with_context(|| format!("the project has no block {reference}"))?;
    let drops: Vec<_> = block
        .drops
        .iter()
        .map(|drop| serde_json::json!({ "item": drop.item, "count": drop.count, "chance": drop.chance }))
        .collect();
    let said = serde_json::json!({
        "guid": guid.to_string(),
        "name": content.block_name(guid).unwrap_or_default(),
        "color": block.color,
        "liquid": match block.liquid { Liquid::None => "none", Liquid::Water => "water" },
        "hardness": block.hardness,
        "unbreakable": block.unbreakable,
        "tool": block.tool,
        "tier": block.tier,
        "data": block.data,
        "drops": drops,
        "sounds": { "break": block.sounds.broken, "place": block.sounds.placed, "step": block.sounds.step },
    });
    Ok(said.to_string())
}

/// Takes up the blocks as they are now in every world of blocks made
/// before one of them was last saved: its look, and its faces made again
/// where they are not as they were. A world without end is made anew by
/// its streamer instead (see [`stream`]).
pub fn refresh(world: &mut World, content: &mut Content, mut renderer: Option<&mut Renderer>) {
    let version = content.blocks_version();
    let stale: Vec<(voxear_ecs::Entity, BlockWorldAsset)> = world
        .query_ref::<(voxear_ecs::Entity, &BlockWorldAsset)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.derived != version)
        .map(|(entity, asset)| (entity, asset.clone()))
        .collect();
    for (entity, asset) in stale {
        let file = Arc::new(resolved(content, &asset.file));
        let endless = file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
        let (_, faces) = file.faces();
        let remade = world
            .get::<BlockWorld>(entity)
            .filter(|blocks| !endless && *blocks.faces != faces[..])
            .map(|blocks| blocks.restyled(&faces, file.smooth));
        if let Some(remade) = remade {
            world.insert(entity, remade);
        }
        if let Some(renderer) = renderer.as_deref_mut() {
            match look(content, renderer, &file) {
                Ok(look) => drop(world.insert(entity, look)),
                Err(error) => tracing::warn!("a world of blocks' look: {error:#}"),
            }
        }
        world.insert(entity, BlockWorldAsset { drawn: file, derived: version, ..asset });
    }
}

/// The world of blocks made on `base`, `file`'s ground, with `edits` made
/// to it, each chunk's faces worked out on every core.
pub fn make(file: &Resolved, base: &Arc<BlockMap>, edits: Option<&BlockEdits>) -> BlockWorld {
    let (_, faces) = file.faces();
    let mut blocks = (**base).clone();
    if let Some(edits) = edits {
        edits.apply(&mut blocks);
    }
    BlockWorld::made(blocks, &faces, file.block_size, file.smooth)
}

/// How a world of blocks looks: each way its faces are drawn, with its
/// material where it names one and else its colour, a picture a block.
pub fn look(content: &mut Content, renderer: &mut Renderer, file: &Resolved) -> Result<BlockLook> {
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
        content.keep_block_world(asset.guid, (&asset.file, asset.derived), (&asset.base, &asset.pending), blocks);
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
    voxear_core::profile_scope!("Block Streaming");
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
    let budget = voxear_terrain::WorldBudget::of(world);
    let mut streams = world.remove_resource::<Streams>().unwrap_or_default();
    streams.0.retain(|entity, _| endless.iter().any(|(e, ..)| e == entity));
    for (entity, mut asset, corner) in endless {
        let Some(generate) = asset.file.generate.clone() else { continue };
        let Some(far) = generate.endless else { continue };
        let Some(generator) = generator(content, &asset.file) else { continue };
        // Its file as its blocks are now, so a block saved since makes it anew; a block it comes to hold changes
        // its faces alone, and nothing made is made again.
        let (_, faces) = asset.drawn.faces();
        // Its far view kept in the project's own folder of what is made, so ground seen once comes back at once.
        let far_folder = Some(content.db().root().join(".voxear").join("FarView").join(asset.guid.to_string()));
        let making = voxear_terrain::blocks::stream::BlockMaking {
            file: asset.drawn.for_ground(),
            generate,
            world: generator,
            faces: faces.into(),
            far_folder,
        };
        let Some(mut blocks) = world.get::<BlockWorld>(entity).cloned() else { continue };
        let camera = [viewer.x - corner.x, viewer.z - corner.z];
        let streamer = streams.0.entry(entity).or_default();
        // As many columns at once and as far as the game's budget lets it, and no further than the world's own.
        streamer.at_once = budget.columns as usize;
        let distances = (far.distance.min(budget.view_distance), far.far.min(budget.far_distance));
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
