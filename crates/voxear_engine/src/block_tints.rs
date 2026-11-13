//! The tints of each generated world of blocks round where it is seen
//! from: the colours its tinted blocks take from its biomes (see
//! `voxear_terrain::blocks::generate::tints`), kept as a square of its tint
//! lattice's points in a `BlockTints` the renderer reads.
//!
//! The square is made of tiles, [`REACH`] each way round the tile the
//! world is seen from, so it reaches a little over 500 blocks each way.
//! Tiles are worked out on the worker threads, the nearest first and a few
//! a frame, and kept while they stay in reach. One not worked out yet is
//! white, as an untinted world is, until it is. A world whose biomes have
//! no tints, or none at all, has no `BlockTints`.

use std::{collections::HashMap, sync::Arc};

use voxear_ecs::{Entity, World};
use voxear_render::BlockTints;
use voxear_scene::{Destroying, Transform, WorldTransform};
use voxear_terrain::blocks::{
    BlockWorld,
    generate::tints::{TINT_STEP, TINT_TILE, tint_tile},
};

use crate::{blocks::BlockWorldAsset, content::Content};

/// How many tiles each way round the tile a world is seen from its tints
/// reach.
const REACH: i32 = 8;

/// How many tiles are worked out in a frame at most.
const TILES_A_FRAME: usize = 24;

/// A white tint, as the lattice packs one: each part 1, out of 2.
const WHITE: u32 = 0x0080_8080;

/// A world's tiles worked out, and what they were worked out from: its
/// biomes and generator as read, where it lies in the generator's world
/// and how large its blocks are.
struct Kept {
    from: (usize, usize, [u32; 2], u32),
    tiles: HashMap<[i32; 2], Arc<Vec<u32>>>,
    /// The tile it was seen from and how many tiles it had when its tints
    /// were last put together.
    shown: Option<([i32; 2], usize)>,
}

/// Each tinted world's tiles, by its Obj.
#[derive(Default)]
struct TintTiles(HashMap<Entity, Kept>);

/// Brings each generated world of blocks' tints round to where it is seen
/// from: the editor's view where there is one, or else the game's camera;
/// with neither, round its corner.
pub fn update(world: &mut World, content: &mut Content) {
    voxear_core::profile_scope!("Block Tints");
    let tinted = |asset: &BlockWorldAsset| {
        let placed = asset.file.generate.as_ref().and_then(|generate| generate.placed.as_deref());
        placed.is_some_and(|placed| placed.biomes.list().iter().any(|biome| !biome.tints.is_none()))
    };
    let worlds: Vec<(Entity, BlockWorldAsset, voxear_core::math::Vec3)> = world
        .query_ref::<(Entity, &BlockWorld, &BlockWorldAsset, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(_, _, asset, ..)| tinted(asset))
        .map(|(entity, _, asset, local, placed)| (entity, asset.clone(), voxear_terrain::corner(local, placed)))
        .collect();
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
    let mut left = TILES_A_FRAME;
    for (entity, asset, corner) in worlds {
        let Some(generator) = crate::blocks::generator(content, &asset.file) else { continue };
        let Some(generate) = asset.file.generate.as_ref() else { continue };
        let size = asset.file.block_size.max(0.01);
        let placed = generate.placed.as_ref().map_or(0, |placed| Arc::as_ptr(placed) as usize);
        let from = (placed, Arc::as_ptr(&generator) as usize, generate.position.map(f32::to_bits), size.to_bits());
        let kept = kept.0.entry(entity).or_insert_with(|| Kept { from, tiles: HashMap::new(), shown: None });
        if kept.from != from {
            *kept = Kept { from, tiles: HashMap::new(), shown: None };
        }
        // The tile it is seen from, and those in reach, the nearest first.
        let local = viewer.map_or(voxear_core::math::Vec3::ZERO, |viewer| viewer - corner);
        let tile_side = (TINT_TILE * TINT_STEP) as f32 * size;
        let seen = [(local.x / tile_side).floor() as i32, (local.z / tile_side).floor() as i32];
        let within = |tile: &[i32; 2]| (tile[0] - seen[0]).abs() <= REACH && (tile[1] - seen[1]).abs() <= REACH;
        kept.tiles.retain(|tile, _| within(tile));
        let mut missing: Vec<[i32; 2]> = (-REACH..REACH)
            .flat_map(|dz| (-REACH..REACH).map(move |dx| [seen[0] + dx, seen[1] + dz]))
            .filter(|tile| !kept.tiles.contains_key(tile))
            .collect();
        missing.sort_by_key(|tile| (tile[0] - seen[0]).pow(2) + (tile[1] - seen[1]).pow(2));
        missing.truncate(left);
        left -= missing.len();
        let made = voxear_terrain::workers::each(&missing, |&tile| tint_tile(generate, &generator, size, tile));
        for (tile, tints) in missing.into_iter().zip(made) {
            kept.tiles.insert(tile, Arc::new(tints.unwrap_or_default()));
        }
        if kept.shown == Some((seen, kept.tiles.len())) && world.get::<BlockTints>(entity).is_some() {
            continue;
        }
        kept.shown = Some((seen, kept.tiles.len()));
        // Put together, the tiles' points row by row, white where a tile is not worked out yet.
        let tiles = 2 * REACH;
        let side = (tiles * TINT_TILE) as usize;
        let first = [seen[0] - REACH, seen[1] - REACH];
        let mut points = vec![WHITE; side * side * 2];
        for (tile, tints) in &kept.tiles {
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
        let corner = first.map(|tile| tile * TINT_TILE);
        world.insert(entity, BlockTints { corner, size: side as u32, points: points.into() });
    }
    world.insert_resource(kept);
}
