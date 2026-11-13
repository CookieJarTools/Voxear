//! The starter blocks: a set of blocks of parts a project can start from,
//! made by **Starter Blocks** in the Blocks panel, so a world can have
//! slabs, stairs, fences, walls, panes, doors, trap doors, gates, levers,
//! buttons, chains, ropes and a lantern without drawing anything.
//!
//! Oak and stone, each a slab, stairs and more; glass in a pane; iron for
//! a chain. Their pictures are drawn here: oak planks in boards, glass clear
//! inside its frame, a chain's links on a clear ground, each a picture of
//! small squares as a pixel-art one is. A block of a starter's name there
//! already is left as it is: nothing a project made is written over.

use anyhow::Result;
use voxear_engine::Game;
use voxear_terrain::blocks::{
    block::{BlockDef, BlockShape},
    shape::BoxDef,
};

use crate::{
    block_props,
    starter_trees::{block_named, material_with, mixed, picture, toned},
};

/// The folder in the project's Blocks folder the starters go in, and the
/// one in it their pictures go in.
const FOLDER: &str = "Starter";
const PICTURES: &str = "Pictures";

const OAK: [f32; 3] = [0.62, 0.45, 0.26];
const STONE: [f32; 3] = [0.52, 0.52, 0.54];
const IRON: [f32; 3] = [0.42, 0.43, 0.46];
const ROPE: [f32; 3] = [0.66, 0.55, 0.36];
const LANTERN: [f32; 3] = [0.95, 0.72, 0.3];

/// Oak planks: boards across, four squares wide, each a shade of its own,
/// with a dark seam between them and a nail here and there.
fn planks_picture() -> Vec<u8> {
    picture(|x, y| {
        let board = y / 4;
        if y % 4 == 3 {
            return toned(OAK, 0.62, 255);
        }
        let shade = [0.9, 1.0, 1.08, 0.95][(mixed(board, 0, 7) % 4) as usize];
        let grain = if mixed(x, y, 11) % 7 == 0 { 0.9 } else { 1.0 };
        let nail = (x == 1 || x == 14) && y % 4 == 1;
        if nail { toned(OAK, 0.5, 255) } else { toned(OAK, shade * grain, 255) }
    })
}

/// A pane's glass: a frame a square wide, and clear inside but for a
/// streak of light across a corner.
fn glass_picture() -> Vec<u8> {
    let glass = [0.82, 0.9, 0.94];
    picture(|x, y| {
        let frame = x == 0 || y == 0 || x == 15 || y == 15;
        let streak = (x + y == 5 || x + y == 6) && x > 1 && y > 1;
        match (frame, streak) {
            (true, _) => toned(glass, 0.85, 255),
            (false, true) => toned(glass, 1.15, 255),
            _ => toned(glass, 1.0, 0),
        }
    })
}

/// A chain's links down the middle of the picture, one upright and one
/// seen edge on in turn, on a clear ground.
fn chain_picture() -> Vec<u8> {
    picture(|x, y| {
        let link = y / 4;
        let (row, column) = (y % 4, x);
        let ring = match link % 2 {
            // Seen whole: a ring two squares round a hole.
            0 => (6..=9).contains(&column) && (row == 0 || row == 3 || column == 6 || column == 9),
            // Edge on: a bar down the middle.
            _ => (7..=8).contains(&column),
        };
        if ring { toned(IRON, if column == 6 || row == 0 { 1.15 } else { 0.9 }, 255) } else { toned(IRON, 1.0, 0) }
    })
}

/// A block of `shape` drawn as `color`, with `material` where it names one.
fn part(shape: BlockShape, color: [f32; 3], material: Option<&str>, hardness: f32) -> BlockDef {
    BlockDef { shape, material: material.map(str::to_string), hardness, ..BlockDef::colored(color) }
}

/// A lantern: a small case under a cap and a loop, hanging and swaying.
fn lantern() -> BlockDef {
    let boxes = vec![
        BoxDef { from: [5, 1, 5], to: [11, 9, 11], moves: false },
        BoxDef { from: [6, 9, 6], to: [10, 11, 10], moves: false },
        BoxDef { from: [7, 11, 7], to: [9, 16, 9], moves: false },
    ];
    BlockDef { shape: BlockShape::Boxes, boxes, hangs: true, swing: Some(4.0), hardness: 0.5, ..BlockDef::colored(LANTERN) }
}

/// The starters, by name, each as its block: those of oak and glass and
/// iron drawn with the pictures named, by their materials' references.
fn starters(planks: &str, glass: &str, chain: &str) -> Vec<(&'static str, BlockDef)> {
    let (oak, stone) = (Some(planks), None);
    vec![
        ("Oak Slab", part(BlockShape::Slab, OAK, oak, 1.5)),
        ("Oak Stairs", part(BlockShape::Stairs, OAK, oak, 1.5)),
        ("Oak Fence", part(BlockShape::Fence, OAK, oak, 1.5)),
        ("Oak Fence Gate", part(BlockShape::FenceGate, OAK, oak, 1.5)),
        ("Oak Door", part(BlockShape::Door, OAK, oak, 1.5)),
        ("Oak Trap Door", part(BlockShape::TrapDoor, OAK, oak, 1.5)),
        ("Stone Slab", part(BlockShape::Slab, STONE, stone, 2.0)),
        ("Stone Stairs", part(BlockShape::Stairs, STONE, stone, 2.0)),
        ("Stone Wall", part(BlockShape::Wall, STONE, stone, 2.0)),
        ("Stone Button", part(BlockShape::Button, STONE, stone, 0.5)),
        ("Lever", part(BlockShape::Lever, STONE, stone, 0.5)),
        ("Glass Pane", part(BlockShape::Pane, [0.82, 0.9, 0.94], Some(glass), 0.3)),
        ("Chain", part(BlockShape::Chain, IRON, Some(chain), 1.0)),
        ("Rope", part(BlockShape::Rope, ROPE, None, 0.3)),
        ("Lantern", lantern()),
    ]
}

/// Makes the starter blocks, and their pictures, in the project's Blocks
/// folder; gives each one's path, in the order they are listed.
pub fn add(game: &mut Game) -> Result<Vec<String>> {
    let folder = format!("{}/{FOLDER}", block_props::BLOCKS_FOLDER);
    let pictures = format!("{folder}/{PICTURES}");
    let named = |game: &Game, path: &str| game.content.db().guid_of(path).map_or(path.to_string(), |guid| guid.to_string());
    let planks = material_with(game, &format!("{pictures}/Oak Planks"), &planks_picture(), false)?;
    let glass = material_with(game, &format!("{pictures}/Glass"), &glass_picture(), true)?;
    let chain = material_with(game, &format!("{pictures}/Chain"), &chain_picture(), true)?;
    let (planks, glass, chain) = (named(game, &planks), named(game, &glass), named(game, &chain));
    let mut made = Vec::new();
    for (name, block) in starters(&planks, &glass, &chain) {
        made.push(block_named(game, &folder, name, &block)?);
    }
    game.content.rescan()?;
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starters_are_one_of_each_shape_of_parts_and_their_pictures_are_whole() {
        let starters = starters("a", "b", "c");
        let mut shapes: Vec<BlockShape> = starters.iter().map(|(_, block)| block.shape).collect();
        shapes.sort_by_key(|shape| BlockShape::ALL.iter().position(|s| s == shape));
        shapes.dedup();
        let parts = BlockShape::ALL.iter().filter(|shape| shape.is_part()).count();
        assert_eq!(shapes.len(), parts, "every shape of parts: {shapes:?}");
        for (name, block) in &starters {
            assert_eq!(BlockDef::parse(&block.to_text()).unwrap(), *block, "{name} reads as written");
        }
        let side = (16 * 4 * 16 * 4 * 4) as usize;
        for rgba in [planks_picture(), glass_picture(), chain_picture()] {
            assert_eq!(rgba.len(), side);
        }
        // Glass and a chain are clear in places, for their materials to cut out; planks never.
        let clear = |rgba: &[u8]| rgba.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
        assert!(clear(&glass_picture()) > side / 8 && clear(&chain_picture()) > side / 8 && clear(&planks_picture()) == 0);
    }
}
