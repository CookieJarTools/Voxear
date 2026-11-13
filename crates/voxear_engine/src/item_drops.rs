//! Items lying in the world: what a tree breaks into where it lands, and
//! whatever a game drops. Each is an Obj of its own ([`ItemDrop`]): an
//! item, by its data asset, and how many of it. It is thrown, falls onto
//! the blocks of any Block World, and rests there, turning slowly; drops
//! of the same item that come to rest near each other become one. It is
//! drawn as its item's **Dropped** prefab where it has one, else as a
//! small block of the block its item places, or a plain one for an item
//! that places none. Scripts take one up (`ItemDrops.Take`,
//! and the Item Pickup behaviour of Voxear.Core, which takes up those in
//! its reach), and it goes by itself after a while ([`LASTS`]).
//!
//! Drops are the moment's: a save of the scene keeps none of them.

use std::collections::HashMap;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{MaterialHandle, MeshData, MeshHandle, MeshRenderer, Renderer};
use voxear_scene::{Destroying, EditorOnly, Transform, WorldTransform};
use voxear_terrain::blocks::{BlockWorld, block::BlockDrop, chunk::AIR, is_ground};

use crate::{Content, MaterialSource};

/// How long a drop lies before it goes by itself, in seconds.
pub const LASTS: f32 = 300.0;

/// How large a drop is drawn, across, in metres.
pub const SIZE: f32 = 0.25;

/// How soon after it is dropped a drop can be taken up, in seconds: it is
/// seen to fall first.
pub const TAKEN_AFTER: f32 = 0.5;

/// How near two resting drops of one item are to become one, in metres.
const MERGE_WITHIN: f32 = 0.6;

const GRAVITY: f32 = 9.81;

/// An item lying in the world: which, by a reference to its data asset,
/// how many, how it moves, and how long it has lain.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemDrop {
    pub item: String,
    pub count: u32,
    pub velocity: Vec3,
    pub resting: bool,
    pub age: f32,
}

impl Component for ItemDrop {}

/// A drop shown as its item's Dropped prefab, by its reference, and
/// whether the prefab is there yet (`Game` puts it there, as a child).
#[derive(Clone, Debug, PartialEq)]
pub struct DroppedAs {
    pub prefab: String,
    pub shown: bool,
}

impl Component for DroppedAs {}

/// What drops are drawn with: one cube, and a material for each item.
#[derive(Default)]
struct DropLooks {
    cube: Option<MeshHandle>,
    materials: HashMap<String, MaterialHandle>,
}

/// A number from three, well mixed.
pub fn mix(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// What `drops` give, `share` of the time each would (a thin log's share
/// of a whole one), with numbers from `seed`: each item and how many.
pub fn rolled(drops: &[BlockDrop], share: f32, seed: u32) -> Vec<(String, u32)> {
    let mut given = Vec::new();
    for (at, drop) in drops.iter().enumerate() {
        let Some(item) = drop.item.clone() else { continue };
        let roll = |salt: u32| (mix(seed, at as u32, salt) >> 8) as f32 / 16_777_216.0;
        if roll(1) >= drop.chance.clamp(0.0, 1.0) * share.clamp(0.0, 1.0) {
            continue;
        }
        let [least, most] = [drop.count[0].min(drop.count[1]), drop.count[0].max(drop.count[1])];
        let count = least + ((roll(2) * (most - least + 1) as f32) as u32).min(most - least);
        if count > 0 {
            given.push((item, count));
        }
    }
    given
}

/// Drops `count` of `item` at `at`, in metres, thrown at `velocity`: the
/// Obj it lies on.
pub fn drop_item(world: &mut World, item: &str, count: u32, at: Vec3, velocity: Vec3) -> Entity {
    let drop = ItemDrop { item: item.to_string(), count: count.max(1), velocity, resting: false, age: 0.0 };
    world.spawn((Transform::new(at, Quat::IDENTITY, Vec3::splat(SIZE)), drop, EditorOnly))
}

/// Every drop lying within `radius` metres of `at` that can be taken up,
/// nearest first.
pub fn near(world: &World, at: Vec3, radius: f32) -> Vec<Entity> {
    let mut found: Vec<(f32, Entity)> = world
        .query_ref::<(Entity, &ItemDrop, &Transform)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, drop, _)| drop.age >= TAKEN_AFTER)
        .map(|(entity, _, transform)| (transform.position.distance(at), entity))
        .filter(|(distance, _)| *distance <= radius)
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, entity)| entity).collect()
}

/// Takes up the drop on `entity`: its item and how many; none where it is
/// no drop, or was taken.
pub fn take(world: &mut World, entity: Entity) -> Option<(String, u32)> {
    if world.has::<Destroying>(entity) {
        return None;
    }
    let drop = world.get::<ItemDrop>(entity)?.clone();
    world.insert(entity, Destroying);
    Some((drop.item, drop.count))
}

/// The ground under or at `at`, in metres, in any world of blocks: the top
/// of the block of ground `at` is in, where it is in one.
fn ground_at(worlds: &[(Vec3, &BlockWorld)], at: Vec3) -> Option<f32> {
    worlds.iter().find_map(|(corner, blocks)| {
        let p = (at - *corner) / blocks.block_size;
        let place = p.floor().as_ivec3().to_array();
        let id = blocks.blocks.get(place);
        (id != AIR && is_ground(&blocks.faces, id)).then(|| corner.y + (place[1] + 1) as f32 * blocks.block_size)
    })
}

/// Moves each drop on by `delta` seconds: falling onto the ground and
/// resting there, turning slowly; joins those of one item resting near
/// each other; lets those go that have lain long enough; and gives those
/// not yet drawn their look where there is a renderer.
pub fn update(world: &mut World, content: &mut Content, renderer: Option<&mut Renderer>, delta: f32) {
    let drops: Vec<(Entity, ItemDrop, Transform)> = world
        .query_ref::<(Entity, &ItemDrop, &Transform)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, drop, transform)| (entity, drop.clone(), *transform))
        .collect();
    if drops.is_empty() {
        return;
    }
    let worlds: Vec<(Vec3, BlockWorld)> = world
        .query_ref::<(&BlockWorld, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(blocks, local, placed)| (voxear_terrain::corner(local, placed), blocks.clone()))
        .collect();
    let worlds: Vec<(Vec3, &BlockWorld)> = worlds.iter().map(|(corner, blocks)| (*corner, blocks)).collect();
    let mut moved = Vec::with_capacity(drops.len());
    for (entity, mut drop, mut transform) in drops {
        drop.age += delta;
        if drop.age >= LASTS {
            world.insert(entity, Destroying);
            continue;
        }
        let half = SIZE * 0.5;
        if !drop.resting || ground_at(&worlds, transform.position - Vec3::Y * (half + 0.02)).is_none() {
            drop.resting = false;
            drop.velocity.y -= GRAVITY * delta;
            let next = transform.position + drop.velocity * delta;
            // Meeting ground under it: on it, at rest; beside it: stopped that way.
            match ground_at(&worlds, next - Vec3::Y * half) {
                Some(top) if drop.velocity.y <= 0.0 => {
                    transform.position = Vec3::new(next.x, top + half, next.z);
                    drop.velocity = Vec3::ZERO;
                    drop.resting = true;
                }
                _ if ground_at(&worlds, next).is_some() => {
                    drop.velocity = Vec3::new(0.0, drop.velocity.y.min(0.0), 0.0);
                }
                _ => transform.position = next,
            }
            // Fallen out of every world: gone.
            if transform.position.y < -10_000.0 {
                world.insert(entity, Destroying);
                continue;
            }
        }
        transform.rotation = Quat::from_rotation_y(drop.age * 1.2);
        moved.push((entity, drop, transform));
    }
    // Resting drops of one item near each other become one.
    let mut gone = vec![false; moved.len()];
    for a in 0..moved.len() {
        for b in a + 1..moved.len() {
            let (first, second) = (&moved[a], &moved[b]);
            if gone[a] || gone[b] || !first.1.resting || !second.1.resting || first.1.item != second.1.item {
                continue;
            }
            if first.2.position.distance(second.2.position) <= MERGE_WITHIN {
                let count = second.1.count;
                moved[a].1.count += count;
                gone[b] = true;
            }
        }
    }
    for (at, (entity, drop, transform)) in moved.into_iter().enumerate() {
        if gone[at] {
            world.insert(entity, Destroying);
            continue;
        }
        world.insert(entity, drop);
        world.insert(entity, transform);
    }
    if let Some(renderer) = renderer {
        give_looks(world, content, renderer);
    }
}

/// The block `item` places, a reference to its `.block`, as its data asset
/// says.
pub(crate) fn places(content: &mut Content, item: &str) -> Option<String> {
    item_field(content, item, "places")
}

/// What `item`'s data asset says of `field`, where it names something.
fn item_field(content: &mut Content, item: &str, field: &str) -> Option<String> {
    let guid = content.parse(item).ok()?.guid()?;
    let path = content.db().get(guid)?.path.clone();
    let text = content.read_text(&path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get("fields")?.get(field)?.as_str().filter(|named| !named.is_empty()).map(str::to_string)
}

/// A look for each drop not yet drawn: a small cube of its item's block.
fn give_looks(world: &mut World, content: &mut Content, renderer: &mut Renderer) {
    let bare: Vec<(Entity, String)> = world
        .query_ref::<(Entity, &ItemDrop)>()
        .without::<MeshRenderer>()
        .without::<DroppedAs>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, drop)| (entity, drop.item.clone()))
        .collect();
    if bare.is_empty() {
        return;
    }
    let mut looks = world.remove_resource::<DropLooks>().unwrap_or_default();
    if looks.cube.is_none() {
        looks.cube =
            renderer.add_mesh(&MeshData::cube()).map_err(|error| tracing::warn!("a drop's cube: {error:#}")).ok();
    }
    for (entity, item) in bare {
        // Its item's own prefab, where it has one, full size.
        if let Some(prefab) = item_field(content, &item, "dropped") {
            world.insert(entity, DroppedAs { prefab, shown: false });
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.scale = Vec3::ONE;
            }
            continue;
        }
        let Some(cube) = looks.cube else { break };
        let material = match looks.materials.get(&item) {
            Some(material) => Some(*material),
            None => {
                let made = drop_material(content, renderer, &item);
                if let Some(material) = made {
                    looks.materials.insert(item.clone(), material);
                }
                made
            }
        };
        let Some(material) = material else { continue };
        world.insert(
            entity,
            MeshRenderer { mesh: cube, material, lod_bias: 0.0, forced_lod: None, lod_transitions: None },
        );
    }
    world.insert_resource(looks);
}

/// The material a drop of `item` is drawn with: its block's sides', or its
/// colour, or grey for an item that places no block.
fn drop_material(content: &mut Content, renderer: &mut Renderer, item: &str) -> Option<MaterialHandle> {
    let block = places(content, item).and_then(|block| content.block(&block)).map(|(_, block)| block);
    let named = block.as_ref().and_then(|block| block.material.clone());
    if let Some(material) = named.and_then(|named| content.parse(&named).ok()) {
        if let Ok(handle) = content.material(renderer, &MaterialSource::Asset(material)) {
            return Some(handle);
        }
    }
    let color = block.map_or([0.6, 0.6, 0.6], |block| block.color);
    let data = crate::MaterialData { color, roughness: 1.0, ..crate::scene_file::default_material() };
    content.material(renderer, &MaterialSource::Inline(data)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_rolls_its_count_and_its_chance_and_a_share_of_it_for_a_thin_log() {
        let log = BlockDrop { item: Some("Items/Log.asset".into()), count: [1, 1], chance: 1.0 };
        assert_eq!(rolled(std::slice::from_ref(&log), 1.0, 7), vec![("Items/Log.asset".to_string(), 1)]);
        assert!(rolled(&[BlockDrop { item: None, ..log.clone() }], 1.0, 7).is_empty(), "no item, nothing");
        // A quarter as thick across as a whole log drops about a sixteenth as often.
        let thin = (0..4000).filter(|&seed| !rolled(std::slice::from_ref(&log), 1.0 / 16.0, seed).is_empty()).count();
        assert!((150..350).contains(&thin), "{thin} of 4000");
        // From and to: every count between, and none past.
        let some = BlockDrop { count: [2, 4], ..log };
        let counts: std::collections::BTreeSet<u32> =
            (0..400).flat_map(|seed| rolled(std::slice::from_ref(&some), 1.0, seed)).map(|(_, n)| n).collect();
        assert_eq!(counts.into_iter().collect::<Vec<_>>(), [2, 3, 4]);
    }
}
