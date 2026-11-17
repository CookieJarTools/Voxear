//! The tints of each generated world of blocks round where it is seen
//! from: the colours its tinted blocks take from its biomes (see
//! `voxear_terrain::blocks::generate::tints`), kept as a square of its tint
//! lattice's points in a `BlockTints` the renderer reads.
//!
//! The square is made of tiles, [`REACH`] each way round the tile the
//! world is seen from, so it reaches a little over 500 blocks each way;
//! and a second square as many tiles of points `TINT_FAR` times further
//! apart, for the far view, reaching some 8 km each way.
//! Tiles are worked out on the terrain's background threads, the nearest
//! first and a few at once, taken in as they are done and kept while they
//! stay in reach; the frame never waits for them. One not worked out yet is
//! white, as an untinted world is, until it is. A world whose biomes have
//! no tints, or none at all, has no `BlockTints`, unless plants grow on it
//! that colour its grass far off (see `grown_plants::covers`).

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use voxear_ecs::{Entity, World};
use voxear_render::BlockTints;
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld,
    generate::tints::{TINT_FAR, TINT_STEP, TINT_TILE, tint_tile},
};

use crate::{blocks::BlockWorldAsset, content::Content};

/// How many tiles each way round the tile a world is seen from its tints
/// reach.
const REACH: i32 = 8;

/// How many tiles are worked out at once at most, each world's.
const TILES_AT_ONCE: usize = 24;

/// A white tint, as the lattice packs one: each part 1, out of 2.
const WHITE: u32 = 0x0080_8080;

/// A world's tiles worked out, and what they were worked out from: its
/// biomes and generator as read, where it lies in the generator's world,
/// how large its blocks are, and what covers its grass. Its near tints'
/// tiles and its far ones'.
struct Kept {
    from: (usize, usize, [u32; 2], u32, u64),
    windows: [Window; 2],
    /// The tiles worked out in the background and not yet taken in: by
    /// window, tile and its tints.
    done: Arc<Mutex<Vec<(usize, [i32; 2], Vec<u32>)>>>,
}

/// A square of tint tiles round where a world is seen from: the tiles
/// worked out, and the tile it was seen from and how many tiles it had
/// when they were last put together.
#[derive(Default)]
struct Window {
    tiles: HashMap<[i32; 2], Arc<Vec<u32>>>,
    /// The tiles being worked out.
    asked: HashSet<[i32; 2]>,
    shown: Option<([i32; 2], usize)>,
}

/// Each tinted world's tiles, by its Obj.
#[derive(Default)]
struct TintTiles(HashMap<Entity, Kept>);

/// Brings each generated world of blocks' tints round to where it is seen
/// from: the editor's view where there is one, or else the game's camera;
/// with neither, round its corner. Its near tints first, then its far ones.
pub fn update(world: &mut World, content: &mut Content) {
    voxear_core::profile_scope!("Block Tints");
    let tinted = |asset: &BlockWorldAsset| {
        let placed = asset.file.generate.as_ref().and_then(|generate| generate.placed.as_deref());
        placed.is_some_and(|placed| placed.biomes.list().iter().any(|biome| !biome.tints.is_none()))
    };
    let generated: Vec<(Entity, bool, voxear_core::math::Vec3)> = world
        .query_ref::<(Entity, &BlockWorld, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, _, asset, ..)| asset.file.generate.is_some())
        .map(|(entity, _, asset, local, placed)| (entity, tinted(asset), voxear_terrain::corner(local, placed)))
        .collect();
    // Those tinted, or whose grass plants colour far off.
    let covering = voxear_core::profile_marker!("Covers").scope();
    let worlds: Vec<(Entity, voxear_core::math::Vec3, crate::grown_plants::Covers)> = generated
        .into_iter()
        .filter_map(|(entity, tinted, corner)| {
            let covers = crate::grown_plants::covers(world, content, entity);
            (tinted || !covers.0.is_empty()).then_some((entity, corner, covers))
        })
        .collect();
    drop(covering);
    // Those no longer tinted are drawn untinted.
    let stale: Vec<Entity> = world
        .query_ref::<(Entity, &BlockTints)>()
        .into_iter()
        .map(|(entity, _)| entity)
        .filter(|entity| !worlds.iter().any(|(tinted, ..)| tinted == entity))
        .collect();
    for entity in stale {
        world.remove::<BlockTints>(entity);
    }
    if worlds.is_empty() {
        world.remove_resource::<TintTiles>();
        return;
    }
    let viewer = crate::terrain::viewer(world);
    let mut kept = world.remove_resource::<TintTiles>().unwrap_or_default();
    kept.0.retain(|entity, _| worlds.iter().any(|(tinted, ..)| tinted == entity));
    let seen = crate::dimensions::seen_from(world);
    for (entity, corner, (covers, cover, covered)) in worlds {
        // A world of a dimension not seen keeps its tints as they are.
        if viewer.is_some() && voxear_scene::dimension_of(world, entity) != seen {
            continue;
        }
        let Some(asset) = world.get::<BlockWorldAsset>(entity) else { continue };
        let Some(generator) = crate::blocks::generator(content, &asset.file) else { continue };
        let Some(generate) = asset.file.generate.as_ref() else { continue };
        let size = asset.file.block_size.max(0.01);
        let placed = generate.placed.as_ref().map_or(0, |placed| Arc::as_ptr(placed) as usize);
        let stamp = voxear_terrain::layers::stable_hash(format!("{covers:?} {cover:?} {covered:?}").as_bytes());
        let from =
            (placed, Arc::as_ptr(&generator) as usize, generate.position.map(f32::to_bits), size.to_bits(), stamp);
        let made = || Kept { from, windows: Default::default(), done: Default::default() };
        let kept = kept.0.entry(entity).or_insert_with(made);
        if kept.from != from {
            // What is still being worked out for the old is let go when it is done.
            *kept = made();
        }
        let done = std::mem::take(&mut *kept.done.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        let mut came = false;
        for (window, tile, tints) in done {
            let window = &mut kept.windows[window];
            window.asked.remove(&tile);
            window.tiles.insert(tile, Arc::new(tints));
            came = true;
        }
        // Handed to the background threads as they are asked for tiles, the world's way of generating copied once.
        let (mut shared, covers) = (None, Arc::new(covers));
        let mut left = TILES_AT_ONCE.saturating_sub(kept.windows.iter().map(|window| window.asked.len()).sum());
        let local = viewer.map_or(voxear_core::math::Vec3::ZERO, |viewer| viewer - corner);
        let mut changed = came || world.get::<BlockTints>(entity).is_none();
        let mut seen_from = [[0; 2]; 2];
        for (number, ((window, scale), seen_from)) in
            kept.windows.iter_mut().zip([1, TINT_FAR]).zip(&mut seen_from).enumerate()
        {
            // As the near tints' tiles are, of blocks as many times larger: points as many times further apart.
            let size = size * scale as f32;
            let tile_side = (TINT_TILE * TINT_STEP) as f32 * size;
            let seen = [(local.x / tile_side).floor() as i32, (local.z / tile_side).floor() as i32];
            let within = |tile: &[i32; 2]| (tile[0] - seen[0]).abs() <= REACH && (tile[1] - seen[1]).abs() <= REACH;
            window.tiles.retain(|tile, _| within(tile));
            let mut missing: Vec<[i32; 2]> = (-REACH..REACH)
                .flat_map(|dz| (-REACH..REACH).map(move |dx| [seen[0] + dx, seen[1] + dz]))
                .filter(|tile| !window.tiles.contains_key(tile) && !window.asked.contains(tile))
                .collect();
            missing.sort_by_key(|tile| (tile[0] - seen[0]).pow(2) + (tile[1] - seen[1]).pow(2));
            missing.truncate(left);
            left -= missing.len();
            for tile in missing {
                window.asked.insert(tile);
                let generate = shared.get_or_insert_with(|| Arc::new(generate.clone())).clone();
                let (generator, covers, done) = (generator.clone(), covers.clone(), kept.done.clone());
                voxear_terrain::workers::spawn(move || {
                    let tints = tint_tile(&generate, &generator, size, tile, &covers).unwrap_or_default();
                    done.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push((number, tile, tints));
                });
            }
            changed |= window.shown != Some((seen, window.tiles.len()));
            window.shown = Some((seen, window.tiles.len()));
            *seen_from = seen;
        }
        // Put together only where a tile came or the tile it is seen from changed: half a megabyte each.
        let _putting = voxear_core::profile_marker!("Put Together").scope();
        if changed {
            let [(corner, points), (far_corner, far_points)] =
                [0, 1].map(|window| put_together(&kept.windows[window], seen_from[window]));
            let side = (2 * REACH * TINT_TILE) as u32;
            world.insert(entity, BlockTints { corner, size: side, points, far_corner, far_points, cover, covered });
        }
    }
    world.insert_resource(kept);
}

/// A window's tiles put together round the tile `seen`, their points row
/// by row, white where a tile is not worked out yet: the point they start
/// from, and the points.
fn put_together(window: &Window, seen: [i32; 2]) -> ([i32; 2], Arc<[u32]>) {
    let tiles = 2 * REACH;
    let side = (tiles * TINT_TILE) as usize;
    let first = [seen[0] - REACH, seen[1] - REACH];
    let mut points = vec![WHITE; side * side * 2];
    for (tile, tints) in &window.tiles {
        let at = [tile[0] - first[0], tile[1] - first[1]];
        if !(0..tiles).contains(&at[0]) || !(0..tiles).contains(&at[1]) || tints.is_empty() {
            continue;
        }
        let each = TINT_TILE as usize;
        for z in 0..each {
            let row = (at[1] as usize * each + z) * side + at[0] as usize * each;
            points[row * 2..(row + each) * 2].copy_from_slice(&tints[z * each * 2..(z + 1) * each * 2]);
        }
    }
    (first.map(|tile| tile * TINT_TILE), points.into())
}
