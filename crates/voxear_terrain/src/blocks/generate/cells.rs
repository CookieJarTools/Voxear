//! One cell of generated ground: the block it is, from how deep under its
//! column's surface it lies, the ground pushed in and out by noise in three
//! dimensions so cliffs lean and overhang, and hollowed below by caves:
//! long tunnels where two noises are both near nothing, and chambers where
//! a third is high.

use super::{BlockGenerate, columns::Column};
use crate::{
    blocks::chunk::{AIR, BlockId},
    noise::gradient3,
};

/// How far apart the bumps of the ground's push in and out are, in metres.
const OVERHANG_SIZE: f32 = 14.0;

/// How wide caves' tunnels wind, and how far apart their chambers are, in
/// metres.
const TUNNEL_SIZE: f32 = 22.0;
const CHAMBER_SIZE: f32 = 40.0;

/// How many blocks under the surface caves keep clear of, so they open onto
/// it only here and there.
const CAVE_ROOF: f32 = 3.0;

/// How deep under its column's surface a block `depth` under it is once the
/// surface is pushed in and out at `place`, as [`cell_at`] pushes it.
pub(super) fn pushed(depth: f32, place: [f32; 3], push: f32, seed: u32) -> f32 {
    if push > 0.0 && depth.abs() < push + 1.0 {
        let s = OVERHANG_SIZE;
        return depth + gradient3(place[0] / s, place[1] / s, place[2] / s, seed.wrapping_add(41)) * push;
    }
    depth
}

/// The block `depth` blocks under its column's surface at `place`, in
/// metres from the world's corner, about how far into the ground its
/// middle is, in blocks (out of it where negative), and whether it is the
/// open air over the ground rather than a cave: ground below the surface
/// pushed in and out, but in caves no deeper than `caves_until` blocks, of
/// the column's layer that deep.
#[allow(clippy::too_many_arguments)]
pub(super) fn cell_at(
    column: &Column,
    depth: f32,
    place: [f32; 3],
    size: f32,
    generate: &BlockGenerate,
    push: f32,
    seed: u32,
    caves_until: f32,
) -> (BlockId, f32, bool) {
    let mut depth = depth;
    if push > 0.0 && depth.abs() < push + 1.0 {
        let s = OVERHANG_SIZE;
        depth += gradient3(place[0] / s, place[1] / s, place[2] / s, seed.wrapping_add(41)) * push;
    }
    if depth <= 0.0 {
        return (AIR, depth, true);
    }
    // Caves: tunnels where two noises are both near nothing, chambers where a third is high. How far out of one
    // each is, in blocks, reads its noise's distance from where the cave starts as about half its size a unit.
    let mut inside = depth;
    if generate.caves > 0.0 && depth > CAVE_ROOF - 1.0 && depth < caves_until {
        let caves = generate.caves.clamp(0.0, 1.0);
        let t = TUNNEL_SIZE;
        let tunnel = gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(53)).abs()
            + gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(67)).abs();
        let c = CHAMBER_SIZE;
        let chamber = gradient3(place[0] / c, place[1] / (c * 0.6), place[2] / c, seed.wrapping_add(79));
        let out_of_tunnel = (tunnel - 0.14 * caves.sqrt()) * t * 0.5 / size;
        let out_of_chamber = (0.62 - 0.22 * caves - chamber) * c * 0.5 / size;
        // Within the roof under the surface, the ground stays whole.
        inside = inside.min((CAVE_ROOF - depth).max(out_of_tunnel.min(out_of_chamber)));
    }
    if inside <= 0.0 {
        return (AIR, inside, false);
    }
    let [(top, top_depth), (under, under_depth)] = column.layers;
    let block = if depth <= top_depth {
        top
    } else if depth <= top_depth + under_depth {
        under
    } else {
        column.bottom
    };
    (block, inside, false)
}
