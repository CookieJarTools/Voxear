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
    // And one drawn with a material that bends in the wind, as a foliage material does, sways as leaves do.
    for kind in file.blocks.iter_mut().filter(|kind| kind.is_live()) {
        let mut bends = |material: &Option<String>| {
            let guid = material.as_deref().and_then(|reference| content.parse(reference).ok()?.guid());
            guid.and_then(|guid| content.material_data(guid).ok()).is_some_and(|data| data.wind_bend() > 0.0)
        };
        kind.sways |= bends(&kind.material) || bends(&kind.top) || bends(&kind.bottom);
    }
    // And a face drawn with a material whose pictures are a joining face's five tiles joins those of its block
    // beside it, as panes of glass do.
    for kind in file.blocks.iter_mut().filter(|kind| kind.is_live()) {
        use voxear_terrain::blocks::file::{JOINS_BOTTOM, JOINS_SIDES, JOINS_TOP};
        let mut joins = |material: &Option<String>, bit: u8| {
            let guid = material.as_deref().and_then(|reference| content.parse(reference).ok()?.guid());
            if guid.is_some_and(|guid| crate::texture_packs::material_connected(content, guid)) { bit } else { 0 }
        };
        kind.connected = joins(&kind.material, JOINS_SIDES) | joins(&kind.top, JOINS_TOP) | joins(&kind.bottom, JOINS_BOTTOM);
    }
    // The texture pack worn draws the blocks it names, and with a grain those of a colour alone, with faces of
    // its own: after what a block's own materials say of it is read, which stays so under any pack.
    if let Some(pack) = content.textures().pack().cloned() {
        let own = file.clone();
        for (kind, guid) in file.blocks.iter_mut().zip(&guids) {
            if let Some(guid) = guid.filter(|_| kind.is_live()) {
                pack.dress(guid, kind);
            }
        }
        // A world can draw only so many looks: one the pack's faces would take past that keeps its blocks' own.
        if file.looks() > MAX_LOOKS && own.looks() <= MAX_LOOKS {
            tracing::warn!("a world of blocks draws {MAX_LOOKS} looks at most, so the texture pack is left off it");
            file = own;
        }
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

/// `file` with its generated ground's biomes read (see
/// `voxear_terrain::generate::biomes`): each biome its `[generate]` lists,
/// with the blocks it names numbered by the world, a block the world did
/// not hold yet numbered after those it has, in the order the biomes name
/// them, so the same file gives the same numbers on every machine. A biome
/// or a block that cannot be found is left out, with a warning; a file
/// that lists no biomes comes back with none read.
pub fn with_biomes(content: &mut Content, file: &BlockFile) -> BlockFile {
    use voxear_terrain::generate::{BiomeBlocks, Biomes, WorldBiomes};
    let mut file = file.clone();
    let Some(generate) = file.generate.clone().filter(|generate| !generate.biomes.is_empty()) else {
        // None listed, or none any more: its ground is the world's own layers.
        if let Some(generate) = file.generate.as_mut() {
            generate.placed = None;
        }
        return file;
    };
    let seed = generator(content, &file).map_or(0, |world| world.seed());
    let (mut list, mut blocks) = (Vec::new(), Vec::new());
    for reference in &generate.biomes {
        let Some((_, biome)) = content.biome(reference) else {
            tracing::warn!("a world of blocks lists the biome {reference}, which is not in the project");
            continue;
        };
        let numbered = BiomeBlocks::of(&biome.ground, |named| number_named(content, &mut file, reference, named));
        list.push((*biome).clone());
        blocks.push(numbered);
    }
    // How far a world's climate strays, for weighing temperature against humidity: a generator's usual spread.
    let biomes = Biomes::new(list, 15.0, 0.3, seed);
    if let Some(generate) = file.generate.as_mut() {
        generate.placed = Some(Arc::new(WorldBiomes { biomes, blocks }));
    }
    file
}

/// The number `file` gives the block `named`, which `owner` names: the one
/// it has, or the next after those it has, `file` growing to hold it. Air,
/// with a warning, for a block not in the project or one the world has no
/// room for.
fn number_named(content: &mut Content, file: &mut BlockFile, owner: &str, named: &str) -> BlockId {
    let Some((guid, _)) = content.block(named) else {
        tracing::warn!("{owner} names the block {named}, which is not in the project");
        return voxear_terrain::blocks::chunk::AIR;
    };
    if let Some(number) = resolved(content, file).number_of(guid) {
        return number;
    }
    match with_block(content, file, guid) {
        Some((grown, _)) => {
            *file = grown;
            file.blocks.len() as BlockId
        }
        None => voxear_terrain::blocks::chunk::AIR,
    }
}

/// `file` with its generated ground's structures read (see
/// `voxear_terrain::blocks::structure`): each structure its `[generate]`
/// lists, its blocks or its plant's wood and leaves numbered by the world as
/// [`with_biomes`] numbers a biome's, and the biomes it stands in found
/// among the world's own. A structure, plant or block that cannot be found
/// is left out, with a warning.
pub fn with_structures(content: &mut Content, file: &BlockFile) -> BlockFile {
    use voxear_terrain::blocks::structure::{
        Dungeon, PlacedStructure, StructureBlocks, Town, TownHouseKind, TownPiece, WorldStructures,
    };
    let Some(generate) = file.generate.as_ref().filter(|generate| !generate.structures.is_empty()) else {
        return file.clone();
    };
    let (references, biomes) = (generate.structures.clone(), generate.biomes.clone());
    let mut file = file.clone();
    let biome_guids: Vec<Option<AssetGuid>> =
        biomes.iter().map(|reference| content.biome(reference).map(|(guid, _)| guid)).collect();
    let mut list = Vec::new();
    for reference in &references {
        let Some((_, structure)) = content.structure(reference) else {
            tracing::warn!("a world of blocks lists the structure {reference}, which is not in the project");
            continue;
        };
        let blocks = match (&structure.model, &structure.plant) {
            (Some(model), _) => {
                let Some(entries) = model.blocks() else {
                    tracing::warn!("the structure {reference} has a model whose blocks do not fill its size");
                    continue;
                };
                let numbers: Vec<BlockId> =
                    model.palette.iter().map(|named| number_named(content, &mut file, reference, named)).collect();
                let blocks =
                    entries.iter().map(|&entry| if entry == 0 { 0 } else { numbers[usize::from(entry) - 1] }).collect();
                StructureBlocks::Model { size: model.size, blocks }
            }
            (None, Some(plant)) => {
                let Some((_, def)) = content.plant(plant) else {
                    tracing::warn!("the structure {reference} grows the plant {plant}, which is not in the project");
                    continue;
                };
                let mut number = |named: &Option<String>| {
                    named.as_deref().map_or(0, |named| number_named(content, &mut file, plant, named))
                };
                let (wood, leaves) = (number(&def.wood), number(&def.leaves));
                StructureBlocks::Plant { plant: (*def).clone(), wood, leaves }
            }
            (None, None) if structure.dungeon.is_some() => {
                let dungeon = structure.dungeon.as_ref().expect("checked above");
                let piece = |content: &mut Content, file: &mut BlockFile, named: &str| -> Option<TownPiece> {
                    let (_, part) = content.structure(named).or_else(|| {
                        tracing::warn!("the dungeon {reference} names {named}, which is not in the project");
                        None
                    })?;
                    let model = part.model.as_ref()?;
                    let entries = model.blocks()?;
                    let numbers: Vec<BlockId> =
                        model.palette.iter().map(|block| number_named(content, file, named, block)).collect();
                    let blocks =
                        entries.iter().map(|&e| if e == 0 { 0 } else { numbers[usize::from(e) - 1] }).collect();
                    Some(TownPiece { size: model.size, blocks, ground: part.ground, sink: part.sink })
                };
                let entrance = dungeon.entrance.as_deref().and_then(|named| piece(content, &mut file, named));
                let mut rooms = Vec::new();
                for room in &dungeon.rooms {
                    let Some(piece) = piece(content, &mut file, &room.structure) else { continue };
                    let kind = TownHouseKind {
                        piece,
                        weight: room.weight,
                        most: room.most,
                        zone: room.zone,
                        biomes: Vec::new(),
                    };
                    rooms.push((room.role, kind));
                }
                let mut block = |named: &Option<String>| {
                    named.as_deref().map_or(0, |named| number_named(content, &mut file, reference, named))
                };
                let (wall, floor) = (block(&dungeon.wall), block(&dungeon.floor));
                StructureBlocks::Dungeon(Dungeon {
                    entrance,
                    rooms,
                    count: dungeon.count,
                    main_path: dungeon.main_path,
                    halls: dungeon.halls,
                    corridor: dungeon.corridor,
                    loops: dungeon.loops,
                    floors: dungeon.floors,
                    floor_gap: dungeon.floor_gap,
                    depth: dungeon.depth,
                    wall,
                    floor,
                    corridor_width: dungeon.corridor_width,
                    corridor_height: dungeon.corridor_height,
                    room_size: dungeon.room_size,
                    hall_size: dungeon.hall_size,
                    loot_size: dungeon.loot_size,
                    boss_size: dungeon.boss_size,
                })
            }
            (None, None) => {
                let Some(town) = &structure.town else { continue };
                // Each of its pieces a structure with a model, read and numbered as one would be.
                let piece = |content: &mut Content, file: &mut BlockFile, named: &str| -> Option<TownPiece> {
                    let (_, part) = content.structure(named).or_else(|| {
                        tracing::warn!("the town {reference} names {named}, which is not in the project");
                        None
                    })?;
                    let model = part.model.as_ref()?;
                    let entries = model.blocks()?;
                    let numbers: Vec<BlockId> =
                        model.palette.iter().map(|block| number_named(content, file, named, block)).collect();
                    let blocks =
                        entries.iter().map(|&e| if e == 0 { 0 } else { numbers[usize::from(e) - 1] }).collect();
                    Some(TownPiece { size: model.size, blocks, ground: part.ground, sink: part.sink })
                };
                let middle = town.middle.as_deref().and_then(|named| piece(content, &mut file, named));
                // Each kind of house with the world's biomes it stands in, by their place in the world's list.
                let in_biomes = |content: &mut Content, named: &[String]| -> Vec<usize> {
                    let wanted: Vec<Option<AssetGuid>> =
                        named.iter().map(|named| content.biome(named).map(|(guid, _)| guid)).collect();
                    let guids = biome_guids.iter().enumerate();
                    guids.filter(|(_, guid)| guid.is_some() && wanted.contains(guid)).map(|(at, _)| at).collect()
                };
                let mut houses = Vec::new();
                for house in &town.houses {
                    let Some(piece) = piece(content, &mut file, &house.structure) else { continue };
                    let biomes = in_biomes(content, &house.biomes);
                    houses.push(TownHouseKind {
                        piece,
                        weight: house.weight,
                        most: house.most,
                        zone: house.zone,
                        biomes,
                    });
                }
                let path = town.path.as_deref().map_or(0, |named| number_named(content, &mut file, reference, named));
                StructureBlocks::Town(Town {
                    middle,
                    houses,
                    count: town.count,
                    radius: town.radius,
                    path,
                    path_width: town.path_width,
                    wander: town.wander,
                    branching: town.branching,
                    roads: town.roads,
                    road: town.road.as_deref().map_or(0, |named| number_named(content, &mut file, reference, named)),
                    road_width: town.road_width,
                    bridge: town
                        .bridge
                        .as_deref()
                        .map_or(0, |named| number_named(content, &mut file, reference, named)),
                    link: town.link,
                })
            }
        };
        // The world's biomes it stands in, by their place in the world's list.
        let wanted: Vec<Option<AssetGuid>> =
            structure.placement.biomes.iter().map(|named| content.biome(named).map(|(guid, _)| guid)).collect();
        let in_biomes = biome_guids
            .iter()
            .enumerate()
            .filter(|(_, guid)| guid.is_some() && wanted.contains(guid))
            .map(|(at, _)| at)
            .collect();
        list.push(PlacedStructure {
            placement: structure.placement.clone(),
            ground: structure.ground,
            sink: structure.sink,
            blocks,
            biomes: in_biomes,
        });
    }
    if let Some(generate) = file.generate.as_mut() {
        generate.built = Some(Arc::new(WorldStructures { list }));
    }
    file
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

/// Takes up the blocks and biomes as they are now in every world of blocks
/// made before one of them was last saved: its look, its faces made again
/// where they are not as they were, and, where a biome it lists lies
/// elsewhere or names other blocks now, its ground made again from it with
/// what was built on it kept. A world without end is made anew by its
/// streamer instead (see [`stream`]).
pub fn refresh(world: &mut World, content: &mut Content, mut renderer: Option<&mut Renderer>) {
    let version = content.blocks_version();
    let stale: Vec<(voxear_ecs::Entity, BlockWorldAsset)> = world
        .query_ref::<(voxear_ecs::Entity, &BlockWorldAsset)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, asset)| asset.derived != version)
        .map(|(entity, asset)| (entity, asset.clone()))
        .collect();
    for (entity, mut asset) in stale {
        // Its biomes as their files have them now, the blocks they name numbered.
        let placed = with_biomes(content, &asset.file);
        let regrown = regrown(&asset.file, &placed);
        asset.file = placed;
        let file = Arc::new(resolved(content, &asset.file));
        let endless = file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
        let (_, faces) = file.faces();
        let remade = match world.get::<BlockWorld>(entity) {
            Some(blocks) if regrown && !endless => {
                let ground = Arc::new(ground(content, &asset.file));
                let changes = BlockEdits::between(&asset.base, &blocks.blocks);
                let mut made = make(&file, &ground, Some(&changes));
                made.cast_shadows = blocks.cast_shadows;
                asset.base = ground;
                Some(made)
            }
            Some(blocks) if !endless && *blocks.faces != faces[..] => Some(blocks.restyled(&faces, file.smooth)),
            _ => None,
        };
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

/// Whether `placed`, a file with its biomes read again, makes other ground
/// than `was`: its biomes lie elsewhere or name other blocks. A biome's
/// colour changed makes none.
pub(crate) fn regrown(was: &BlockFile, placed: &BlockFile) -> bool {
    let of = |file: &BlockFile| file.generate.as_ref().and_then(|generate| generate.placed.clone());
    match (of(was), of(placed)) {
        (None, None) => false,
        (Some(was), Some(placed)) => !was.same_ground(&placed),
        _ => true,
    }
}

/// The biomes `file`'s generated ground has, by GUID, in the order its
/// biomes as read are in (see [`with_biomes`]): those it lists that are in
/// the project.
pub fn listed_biomes(content: &mut Content, file: &BlockFile) -> Vec<AssetGuid> {
    let listed = file.generate.as_ref().map(|generate| generate.biomes.clone()).unwrap_or_default();
    listed.iter().filter_map(|reference| Some(content.biome(reference)?.0)).collect()
}

/// The biome the place at `position` is, in the first world of blocks whose
/// generated ground has biomes and reaches there: its GUID. A place's biome
/// is its ground's, however high over it or deep under it the place is.
pub fn biome_at(world: &World, content: &mut Content, position: voxear_core::math::Vec3) -> Option<AssetGuid> {
    use voxear_scene::{Transform, WorldTransform};
    let mut worlds: Vec<(voxear_ecs::Entity, BlockWorldAsset, voxear_core::math::Vec3)> = world
        .query_ref::<(voxear_ecs::Entity, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, asset, ..)| asset.file.generate.as_ref().is_some_and(|generate| generate.placed.is_some()))
        .map(|(entity, asset, local, placed)| (entity, asset.clone(), voxear_terrain::corner(local, placed)))
        .collect();
    worlds.sort_by_key(|(entity, ..)| entity.to_bits());
    for (_, asset, corner) in worlds {
        let Some(generate) = asset.file.generate.as_ref() else { continue };
        let Some(placed) = generate.placed.as_deref() else { continue };
        // In metres from the world's corner; one of a set size reaches only so far.
        let (x, z) = (position.x - corner.x, position.z - corner.z);
        let size = asset.file.ground.size.map(|blocks| blocks as f32 * asset.file.block_size);
        let within = (0.0..size[0]).contains(&x) && (0.0..size[1]).contains(&z);
        if generate.endless.is_none() && !within {
            continue;
        }
        let Some(generator) = generator(content, &asset.file) else { continue };
        let (x, z) = (x + generate.position[0], z + generate.position[1]);
        let height = generator.height_at(x, z);
        let (temperature, humidity) = generator.climate(x, z, height);
        let above_sea = height - generator.sea_level();
        // How far the ground there is sunk under the land round about, where a biome keeps to sunk ground.
        let sunk = if placed.any_sunk() { generator.sunk(&[x], &[z], &[height]) } else { None };
        let sunk = sunk.map_or(0.0, |sunk| sunk[0]);
        let place = voxear_terrain::generate::biomes::Place { temperature, humidity, above_sea, sunk };
        let Some(biome) = placed.biomes.at(x, z, place) else { continue };
        return listed_biomes(content, &asset.file).get(biome).copied();
    }
    None
}

/// What the biome `reference` names says, for scripts, as JSON: its GUID,
/// its name and its colour.
pub fn biome_json(content: &mut Content, reference: &str) -> Result<String> {
    let (guid, biome) = content.biome(reference).with_context(|| format!("the project has no biome {reference}"))?;
    let name = content.biome_name(guid).unwrap_or_default();
    Ok(serde_json::json!({ "guid": guid.to_string(), "name": name, "color": biome.color }).to_string())
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

/// A face drawn with its block's colour alone: matt, as earth and stone
/// are, so it shines back nothing of the sky. A block that should shine
/// names a material.
fn colored(color: [f32; 3]) -> MaterialData {
    MaterialData { color, roughness: 1.0, ..default_material() }
}

/// How a world of blocks looks: each way its faces are drawn, with its
/// material where it names one and else its colour, a picture a block.
pub fn look(content: &mut Content, renderer: &mut Renderer, file: &Resolved) -> Result<BlockLook> {
    let fallback = content.material(renderer, &MaterialSource::Inline(colored(default_material().color)))?;
    let (looks, _) = file.faces();
    let mut surfaces = Vec::with_capacity(looks.len());
    for face in looks {
        let named = face.material.as_ref().and_then(|reference| {
            // A face of the texture pack worn: its picture on the block's own material, or on plain white.
            if let Some(face) = reference.strip_prefix(crate::texture_packs::FACE_PREFIX) {
                let made = crate::texture_packs::face_material(content, renderer, face);
                return made.map_err(|error| tracing::warn!("a block's face under the texture pack: {error:#}")).ok();
            }
            let source = MaterialSource::Asset(content.parse(reference).ok()?);
            content.material(renderer, &source).map_err(|error| tracing::warn!("a block's material: {error:#}")).ok()
        });
        let material = match named {
            Some(material) => material,
            None => {
                content.material(renderer, &MaterialSource::Inline(colored(face.color)))?
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
    let voxels = voxear_render::BlockVoxels { count: file.voxels, relief: file.relief };
    Ok(BlockLook { material: fallback, surfaces: surfaces.into(), voxels })
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

/// What a generated world of blocks' chunks are made from, as its file and
/// its blocks are now: none without a generator.
pub fn making(content: &mut Content, asset: &BlockWorldAsset) -> Option<voxear_terrain::blocks::stream::BlockMaking> {
    let generate = asset.file.generate.clone()?;
    let generator = generator(content, &asset.file)?;
    // Its file as its blocks are now, so a block saved since makes it anew; a block it comes to hold changes
    // its faces alone, and nothing made is made again.
    let (_, faces) = asset.drawn.faces();
    // Its far view kept in the project's own folder of what is made, so ground seen once comes back at once.
    let far_folder = Some(content.db().root().join(".voxear").join("FarView").join(asset.guid.to_string()));
    Some(voxear_terrain::blocks::stream::BlockMaking {
        file: asset.drawn.for_ground(),
        generate,
        world: generator,
        faces: faces.into(),
        far_folder,
        // Nothing built over its ground: who streams it adds what it grows.
        far_built: Vec::new(),
    })
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
        let Some(far) = asset.file.generate.as_ref().and_then(|generate| generate.endless) else { continue };
        let Some(mut making) = making(content, &asset) else { continue };
        // Woods of the plants it grows, where they are thick enough to show so far off.
        making.far_built = crate::grown_plants::far_built(world, content, entity);
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
    fn a_face_of_its_blocks_colour_alone_is_matt() {
        let face = colored([0.2, 0.6, 0.1]);
        assert_eq!((face.color, face.roughness, face.metallic), ([0.2, 0.6, 0.1], 1.0, 0.0));
    }

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
