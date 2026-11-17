//! The game's part in sharing dimensions over the network (see
//! `net::dimensions`): on a server, saying what each dimension it made is
//! and gathering a dimension's edits for a player coming into it; on a
//! player's machine, making the dimensions the server names and taking in
//! their edits.

use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::{Destroying, DimensionId};
use voxear_terrain::blocks::{
    BlockWorld, BuiltBlocks,
    chunk::{AIR, BlockId, CHUNK},
    edits::{BlockEdits, ChunkChange},
};

use super::Game;
use crate::blocks::BlockWorldAsset;
use crate::dimensions::Dimensions;
use crate::net::{self, DimensionWhat, EditPiece, NetRole, NetSession};

/// About how many bytes of changes, before they are packed, one piece of a
/// dimension's edits holds: a few kilobytes once packed.
const PIECE_BYTES: usize = 16 * 1024;

impl Game {
    /// On a server or a host, each frame before what it sends goes: tells
    /// the session what every dimension it made is, and gives each player
    /// coming into one its edits.
    pub(super) fn serve_dimensions(&mut self) {
        let world = &mut self.engine.world;
        let serving =
            world.resource::<NetSession>().is_some_and(|s| matches!(s.role(), NetRole::Server | NetRole::Host));
        if !serving {
            return;
        }
        net::forget_described(world);
        let made: Vec<(DimensionId, Option<voxear_assets::AssetGuid>, u64)> = world
            .resource::<Dimensions>()
            .map(|dimensions| dimensions.made().map(|made| (made.id, made.asset, made.key)).collect())
            .unwrap_or_default();
        for (id, asset, key) in made {
            let what = match self.dungeon_what(id) {
                Some(what) => Some(what),
                None => asset.map(|guid| DimensionWhat::Asset { guid: guid.to_string(), key }),
            };
            if let Some(what) = what {
                net::describe(&mut self.engine.world, id, what);
            }
        }
        for (player, dimension, toward) in net::take_edits_wanted(&mut self.engine.world) {
            let pieces = self.dimension_edits(dimension, toward);
            net::owe_edits(&mut self.engine.world, player, dimension, pieces);
        }
    }

    /// What a dungeon's own dimension is, by its gate: none for one that is
    /// no dungeon's, or whose gate's world no scene placed.
    fn dungeon_what(&self, id: DimensionId) -> Option<DimensionWhat> {
        let world = &self.engine.world;
        let gate = world
            .query_ref::<(Entity, &crate::dungeons::DungeonFrom)>()
            .without::<Destroying>()
            .into_iter()
            .find(|(entity, _)| voxear_scene::dimension_of(world, *entity) == id)
            .map(|(_, from)| from.0)?;
        let (scene, path) = self.levels.place_of(gate.world)?;
        Some(DimensionWhat::Dungeon {
            scene: scene.to_string(),
            path: path.to_string(),
            structure: u16::try_from(gate.structure).ok()?,
            site: gate.site,
            seed: gate.seed,
        })
    }

    /// The worlds of blocks of a dimension, in an order every machine
    /// agrees on: by their places in the level's scenes, or in the
    /// dimension's file; a dungeon's own rock last.
    fn dimension_worlds(&self, dimension: DimensionId) -> Vec<Entity> {
        let world = &self.engine.world;
        let made = world.resource::<Dimensions>().and_then(|dimensions| dimensions.get(dimension));
        let mut keyed: Vec<(String, Entity)> = world
            .query_ref::<(Entity, &BlockWorld, &BlockWorldAsset)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, ..)| entity)
            .filter(|entity| voxear_scene::dimension_of(world, *entity) == dimension)
            .filter_map(|entity| {
                let key = match (dimension.is_level(), made) {
                    (true, _) => self.levels.place_of(entity).map(|(scene, path)| format!("{scene}:{path}")),
                    (false, Some(made)) => made
                        .objs
                        .iter()
                        .find(|(_, placed)| *placed == entity)
                        .map(|(path, _)| path.to_string())
                        .or_else(|| world.has::<crate::dungeons::DungeonInside>(entity).then(|| "~dungeon".into())),
                    (false, None) => None,
                };
                Some((key?, entity))
            })
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, entity)| entity).collect()
    }

    /// Every edit of a dimension's worlds of blocks, from how they are made,
    /// in pieces, those nearest `toward` first.
    fn dimension_edits(&self, dimension: DimensionId, toward: voxear_core::math::Vec3) -> Vec<EditPiece> {
        let world = &self.engine.world;
        let mut pieces = Vec::new();
        for (index, entity) in self.dimension_worlds(dimension).into_iter().enumerate() {
            let (Some(asset), Some(blocks)) = (world.get::<BlockWorldAsset>(entity), world.get::<BlockWorld>(entity))
            else {
                continue;
            };
            let endless = asset.file.generate.as_ref().is_some_and(|generate| generate.endless.is_some());
            let mut edits = match endless {
                true => voxear_terrain::blocks::stream::all_changes(&asset.base, &blocks.blocks, &asset.pending),
                false => {
                    let built = world.get::<BuiltBlocks>(entity).map_or(&asset.base, |built| &built.0);
                    BlockEdits::between(built, &blocks.blocks)
                }
            };
            edits.links = blocks.link_list();
            if edits.is_empty() {
                continue;
            }
            // The blocks this machine numbered as it ran, each with the state it is placed in.
            let numbered: Vec<(u16, String, u8)> = crate::blocks::blocks_numbered_since(world, entity)
                .into_iter()
                .map(|(number, guid)| (number, guid, blocks.faces.get(usize::from(number)).map_or(0, |f| f.state)))
                .collect();
            // Nearest first, from the middle of each chunk.
            let corner = voxear_scene::world_matrix(world, entity).w_axis.truncate();
            let side = CHUNK as f32 * asset.file.block_size;
            let away = |key: &[i32; 3]| {
                let middle = corner + (voxear_core::math::Vec3::from(key.map(|v| v as f32)) + 0.5) * side;
                middle.distance_squared(toward)
            };
            let mut keys: Vec<[i32; 3]> = edits.chunks.keys().copied().collect();
            keys.sort_by(|a, b| away(a).total_cmp(&away(b)));
            let mut piece = BlockEdits { links: std::mem::take(&mut edits.links), ..BlockEdits::default() };
            let mut size = 0;
            for key in keys {
                let Some(change) = edits.chunks.remove(&key) else { continue };
                size += 16 + bytes_of(&change);
                piece.chunks.insert(key, change);
                if size >= PIECE_BYTES {
                    let bytes = std::mem::take(&mut piece).to_bytes();
                    pieces.push(EditPiece { world: index as u16, numbered: numbered.clone(), bytes });
                    size = 0;
                }
            }
            if !piece.is_empty() {
                pieces.push(EditPiece { world: index as u16, numbered: numbered.clone(), bytes: piece.to_bytes() });
            }
        }
        pieces
    }

    /// On a player's machine, each frame after what came in: finds or makes
    /// each dimension the server named, and takes in the edits that came
    /// for those it has.
    pub(super) fn join_dimensions(&mut self, mut renderer: Option<&mut Renderer>) {
        for (number, what) in net::dimensions_to_make(&self.engine.world) {
            let made = self.make_named(&what, renderer.as_deref_mut());
            match made {
                Ok(id) => net::dimension_made(&mut self.engine.world, number, id),
                Err(error) => {
                    tracing::warn!("the server's dimension {what:?} could not be made here: {error:#}");
                    net::dimension_failed(&mut self.engine.world, number);
                }
            }
        }
        for arrived in net::take_edits(&mut self.engine.world) {
            if let Err(error) = self.take_in_edits(arrived.dimension, &arrived.piece) {
                tracing::warn!("the server's edits of a dimension could not be made here: {error:#}");
            }
        }
    }

    /// The dimension the server named, found where it is made here already,
    /// or made now, kept while the server needs it.
    fn make_named(&mut self, what: &DimensionWhat, renderer: Option<&mut Renderer>) -> anyhow::Result<DimensionId> {
        use anyhow::Context;
        let world = &self.engine.world;
        let dimensions = world.resource::<Dimensions>();
        match what {
            DimensionWhat::Asset { guid, key } => {
                let guid: voxear_assets::AssetGuid = guid.parse().ok().context("its asset's GUID")?;
                let fresh = dimensions.and_then(|made| made.find(guid, *key)).is_none();
                let id = self.make_dimension(guid, *key, renderer)?;
                if fresh {
                    crate::dimensions::made_on_demand(&mut self.engine.world, id);
                }
                Ok(id)
            }
            DimensionWhat::Dungeon { scene, path, structure, site, seed } => {
                let scene: voxear_assets::AssetGuid = scene.parse().ok().context("its scene's GUID")?;
                let path = crate::prefab::ObjPath::parse(path).ok().context("its world's path")?;
                let gate_world = self.levels.entity_at(scene, &path).context("its world of blocks is not here")?;
                let gate = crate::dungeons::DungeonGate {
                    world: gate_world,
                    structure: usize::from(*structure),
                    site: *site,
                    seed: *seed,
                };
                Ok(self.make_dungeon_dimension(gate, renderer)?.0)
            }
        }
    }

    /// Makes a piece of the server's edits to one of a dimension's worlds
    /// of blocks: its blocks numbered as this machine numbers them, a block
    /// this machine has no such number for given one now.
    fn take_in_edits(&mut self, dimension: DimensionId, piece: &EditPiece) -> anyhow::Result<()> {
        use anyhow::Context;
        let worlds = self.dimension_worlds(dimension);
        let entity = *worlds.get(usize::from(piece.world)).context("no such world of blocks here")?;
        let edits = BlockEdits::from_bytes(&piece.bytes)?;
        let mut here: std::collections::HashMap<BlockId, BlockId> = std::collections::HashMap::new();
        for (number, guid, state) in &piece.numbered {
            let world = &mut self.engine.world;
            let numbered = crate::blocks::block_number_state(world, &mut self.content, entity, guid, *state, true);
            here.insert(*number, numbered.unwrap_or(AIR));
        }
        let edits = match here.is_empty() {
            true => edits,
            false => edits.remapped(|block| here.get(&block).copied().unwrap_or(block)),
        };
        crate::blocks::take_in(&mut self.engine.world, entity, &edits);
        Ok(())
    }
}

/// About how many bytes a chunk's change takes before it is packed.
fn bytes_of(change: &ChunkChange) -> usize {
    match change {
        ChunkChange::Blocks(blocks) => blocks.len() * 4,
        ChunkChange::Cells(cells) => cells.len() * 5,
        ChunkChange::Whole(chunk) => {
            let (palette, _, words) = chunk.parts();
            palette.len() * 2 + words.len() * 8 + if chunk.has_fills() { (CHUNK * CHUNK * CHUNK) as usize } else { 0 }
        }
    }
}
