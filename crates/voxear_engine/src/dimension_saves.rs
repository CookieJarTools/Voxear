//! What is changed in each dimension but the level's, kept apart from the
//! level's and from every other dimension's.
//!
//! A dimension's worlds of blocks are no Objs of the level's scenes, so a
//! save of the scene does not find them by a scene's key. Each dimension's
//! changes are captured in the same bytes as `Blocks.CaptureEdits` makes for
//! the level's (`voxear_scripting::blocks::capture_edits`), its worlds keyed
//! by their path in the dimension's file, or `dungeon` for a dungeon's rock.
//! They are kept here by the dimension's name ([`DimensionName`]: its asset
//! and key, the same each time it is made) when it is let go, put back when
//! it is made again, and written into a save of the scene beside the level's
//! changes. A dimension never changed has nothing kept and nothing saved: it
//! is made again from its file or its dungeon's seed.

use anyhow::{Context, Result, ensure};
use voxear_ecs::{Entity, World};
use voxear_scene::Destroying;
use voxear_scripting::ScriptServices;
use voxear_terrain::blocks::{chunk::BlockId, edits::BlockEdits};

use crate::{
    Content,
    dimensions::{DimensionId, DimensionName, Dimensions},
};

/// The key of a dungeon's world of rock among its dimension's worlds.
const DUNGEON_KEY: &str = "dungeon";

/// What a save of the scene keeps of the dimensions: this mark, a version,
/// how many, and for each one whether it is made from an asset (a byte), its
/// asset's GUID where it is, its key, and its changes' length and bytes.
const MARK: &[u8; 4] = b"VXDM";
const VERSION: u32 = 1;

/// The changes of each dimension let go, or read from a save, by its name:
/// put back when that dimension is made again. Kept while the level is the
/// one they were made in.
#[derive(Clone, Debug, Default)]
pub struct DimensionEdits {
    level: Option<String>,
    kept: Vec<(DimensionName, Vec<u8>)>,
}

impl DimensionEdits {
    /// The changes kept for a dimension.
    pub fn of(&self, name: DimensionName) -> Option<&[u8]> {
        self.kept.iter().find(|(kept, _)| *kept == name).map(|(_, bytes)| bytes.as_slice())
    }

    /// The names of the dimensions whose changes are kept, and how many bytes each keeps.
    pub fn sizes(&self) -> impl Iterator<Item = (DimensionName, usize)> + '_ {
        self.kept.iter().map(|(name, bytes)| (*name, bytes.len()))
    }

    fn set(&mut self, name: DimensionName, bytes: Option<Vec<u8>>) {
        self.kept.retain(|(kept, _)| *kept != name);
        if let Some(bytes) = bytes {
            self.kept.push((name, bytes));
        }
    }

    /// As bytes for a save: nothing where no dimension has changes kept.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.kept.is_empty() {
            return out;
        }
        out.extend_from_slice(MARK);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(self.kept.len() as u32).to_le_bytes());
        let mut kept: Vec<&(DimensionName, Vec<u8>)> = self.kept.iter().collect();
        kept.sort_by_key(|(name, _)| (name.asset, name.key));
        for (name, bytes) in kept {
            match name.asset {
                Some(asset) => {
                    out.push(1);
                    out.extend_from_slice(&asset.to_u128().to_le_bytes());
                }
                None => out.push(0),
            }
            out.extend_from_slice(&name.key.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    /// What [`to_bytes`](Self::to_bytes) wrote: none kept for no bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut found = Self::default();
        if bytes.is_empty() {
            return Ok(found);
        }
        let take = |at: usize, len: usize| bytes.get(at..at + len).context("the dimensions' changes end early");
        ensure!(take(0, 4)? == MARK.as_slice(), "these are no dimensions' changes");
        let version = u32::from_le_bytes(take(4, 4)?.try_into()?);
        ensure!(version == VERSION, "dimensions' changes from a newer Voxear (version {version})");
        let count = u32::from_le_bytes(take(8, 4)?.try_into()?);
        let mut at = 12;
        for _ in 0..count {
            let asset = match take(at, 1)?[0] {
                0 => None,
                _ => {
                    let guid = u128::from_le_bytes(take(at + 1, 16)?.try_into()?);
                    at += 16;
                    Some(voxear_assets::AssetGuid::from_u128(guid))
                }
            };
            let key = u64::from_le_bytes(take(at + 1, 8)?.try_into()?);
            let len = u32::from_le_bytes(take(at + 9, 4)?.try_into()?) as usize;
            found.kept.push((DimensionName { asset, key }, take(at + 13, len)?.to_vec()));
            at += 13 + len;
        }
        Ok(found)
    }
}

/// The store, as it is for `level`: emptied where it was kept for another.
fn store(world: &mut World, level: Option<&str>) -> DimensionEdits {
    let store = world.remove_resource::<DimensionEdits>().unwrap_or_default();
    match store.level.as_deref() == level {
        true => store,
        false => DimensionEdits { level: level.map(str::to_string), kept: Vec::new() },
    }
}

/// The worlds of blocks, Foliage and saplings of a dimension a save keeps,
/// each by its key in the dimension: its path in the dimension's file, or
/// for a dungeon's rock, `dungeon`.
fn keyed(world: &World, id: DimensionId) -> Vec<(String, Entity)> {
    let Some(made) = world.resource::<Dimensions>().and_then(|dimensions| dimensions.get(id)) else {
        return Vec::new();
    };
    let mut keyed: Vec<(String, Entity)> = made
        .objs
        .iter()
        .filter(|(_, entity)| world.is_alive(*entity) && !world.has::<Destroying>(*entity))
        .map(|(path, entity)| (path.to_string(), *entity))
        .collect();
    let rock = world
        .query_ref::<(Entity, &crate::dungeons::DungeonInside)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, _)| entity)
        .find(|&entity| voxear_scene::dimension_of(world, entity) == id);
    keyed.extend(rock.map(|rock| (DUNGEON_KEY.to_string(), rock)));
    keyed
}

/// What block changes are captured and put back with while one dimension's
/// worlds are: their keys in that dimension, and the project's blocks.
struct OneDimension<'a> {
    content: &'a mut Content,
    objs: Vec<(String, Entity)>,
}

impl ScriptServices for OneDimension<'_> {
    fn obj_key(&self, _world: &World, entity: Entity) -> Option<String> {
        self.objs.iter().find(|(_, obj)| *obj == entity).map(|(key, _)| key.clone())
    }

    fn obj_by_key(&self, _world: &World, key: &str) -> Option<Entity> {
        self.objs.iter().find(|(kept, _)| kept == key).map(|(_, entity)| *entity)
    }

    fn block_number(&mut self, world: &mut World, entity: Entity, reference: &str, grow: bool) -> Option<u16> {
        crate::blocks::block_number(world, self.content, entity, reference, grow)
    }

    fn block_number_state(
        &mut self,
        world: &mut World,
        entity: Entity,
        reference: &str,
        state: u8,
        grow: bool,
    ) -> Option<u16> {
        crate::blocks::block_number_state(world, self.content, entity, reference, state, grow)
    }

    fn blocks_grown(&mut self, world: &World, entity: Entity) -> Vec<(BlockId, String)> {
        crate::blocks::blocks_numbered_since(world, entity)
    }

    fn block_listed(&mut self, world: &World, entity: Entity, block: BlockId) -> bool {
        crate::blocks::block_listed(world, entity, block)
    }

    fn endless_changes(&mut self, world: &World, entity: Entity) -> Option<BlockEdits> {
        crate::blocks::endless_changes(world, entity)
    }

    fn put_endless_changes(&mut self, world: &mut World, entity: Entity, edits: &BlockEdits) -> bool {
        crate::blocks::put_endless_changes(world, entity, edits)
    }
}

/// Bytes `capture_edits` gives for nothing changed: no worlds, nothing after.
fn nothing(bytes: &[u8]) -> bool {
    bytes == 0u32.to_le_bytes()
}

/// A dimension's changes as they are now; none where nothing in it changed.
pub fn capture(world: &World, content: &mut Content, id: DimensionId) -> Option<Vec<u8>> {
    let objs = keyed(world, id);
    if objs.is_empty() {
        return None;
    }
    let mut services = OneDimension { content, objs };
    let _scope = voxear_scripting::ServicesScope::enter(&mut services);
    let bytes = voxear_scripting::blocks::capture_edits(world);
    (!nothing(&bytes)).then_some(bytes)
}

/// Puts a dimension's changes `bytes` back into its worlds, each from as it
/// was made; how many worlds took them.
fn apply(world: &mut World, content: &mut Content, id: DimensionId, bytes: &[u8]) -> Result<i32> {
    let objs = keyed(world, id);
    let mut services = OneDimension { content, objs };
    let _scope = voxear_scripting::ServicesScope::enter(&mut services);
    voxear_scripting::blocks::apply_edits(world, bytes)
}

/// Keeps what was changed in a dimension about to be let go, for when it is
/// made again; forgets what was kept of it where nothing is changed now.
pub fn keep(world: &mut World, content: &mut Content, id: DimensionId, level: Option<&str>) {
    let Some(name) = world.resource::<Dimensions>().and_then(|dimensions| Some(dimensions.get(id)?.name())) else {
        return;
    };
    let bytes = capture(world, content, id);
    let mut kept = store(world, level);
    // Changes kept from before it was made are in it now, so what it holds now is all of them.
    kept.set(name, bytes);
    world.insert_resource(kept);
}

/// Puts back what was changed in a dimension just made, where anything was:
/// before it was last let go, or as a save of the scene says. Not on a
/// player's machine in a networked game: the server sends each dimension's
/// changes as it names it, and they are the whole of them.
pub fn put_back(world: &mut World, content: &mut Content, id: DimensionId, level: Option<&str>) {
    use crate::net::{NetRole, NetSession};
    if world.resource::<NetSession>().is_some_and(|session| matches!(session.role(), NetRole::Client)) {
        return;
    }
    let Some(name) = world.resource::<Dimensions>().and_then(|dimensions| Some(dimensions.get(id)?.name())) else {
        return;
    };
    let kept = store(world, level);
    let bytes = kept.of(name).map(<[u8]>::to_vec);
    world.insert_resource(kept);
    let Some(bytes) = bytes else { return };
    let started = std::time::Instant::now();
    match apply(world, content, id, &bytes) {
        Ok(found) => {
            tracing::debug!(found, bytes = bytes.len(), took = ?started.elapsed(), "a dimension's changes put back")
        }
        Err(error) => tracing::error!("a dimension's changes could not be put back: {error:#}"),
    }
}

/// Every dimension's changes for a save of the scene: those of each made
/// now as they are, and those kept for each let go. Nothing where none has
/// any.
pub fn capture_all(world: &mut World, content: &mut Content, level: Option<&str>) -> Vec<u8> {
    let made: Vec<(DimensionId, DimensionName)> = world
        .resource::<Dimensions>()
        .map(|dimensions| dimensions.made().map(|made| (made.id, made.name())).collect())
        .unwrap_or_default();
    let mut kept = store(world, level);
    for (id, name) in made {
        kept.set(name, capture(world, content, id));
    }
    let bytes = kept.to_bytes();
    world.insert_resource(kept);
    bytes
}

/// Puts back every dimension's changes a save of the scene kept: kept for
/// each when it is made, and put into each made now. How many made now
/// took theirs.
pub fn apply_all(world: &mut World, content: &mut Content, level: Option<&str>, bytes: &[u8]) -> Result<i32> {
    let mut kept = DimensionEdits::from_bytes(bytes)?;
    kept.level = level.map(str::to_string);
    let made: Vec<(DimensionId, DimensionName)> = world
        .resource::<Dimensions>()
        .map(|dimensions| dimensions.made().map(|made| (made.id, made.name())).collect())
        .unwrap_or_default();
    let mut found = 0;
    for (id, name) in made {
        if let Some(bytes) = kept.of(name) {
            apply(world, content, id, bytes)?;
            found += 1;
        }
    }
    world.insert_resource(kept);
    Ok(found)
}
