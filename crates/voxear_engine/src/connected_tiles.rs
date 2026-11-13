//! Pictures for blocks whose faces join those of their kind beside them
//! (see `voxear_terrain::blocks::mesh` and `MaterialData::connected`): the
//! five tiles such a picture is, made from a border and what is inside it,
//! and a glass block to start from, its panes joining into one sheet.
//!
//! The tiles lie side by side, each as wide as it is high: the face alone,
//! its border all round; filled, with no border; joined left and right, its
//! border along its top and bottom; joined up and down, its border down its
//! sides; and joined all round, only its inner corners showing. A face's
//! four quarters are each drawn from that quarter of the tile the faces
//! round it make it, so what is inside the border is the same in all five.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use voxear_assets::{AssetGuid, META_EXTENSION};
use voxear_terrain::blocks::block::BlockDef;

use crate::content::MaterialData;

/// How many tiles a joining face's picture is.
pub const TILES: u32 = 5;

/// The five tiles of a face that joins, as sRGB bytes with alpha, five
/// times `side` pixels wide and `side` high: `frame` wherever a tile has
/// its border, `border` pixels thick, and `inside` everywhere else, by
/// where the pixel is in its tile.
pub fn frame_tiles(side: u32, border: u32, frame: [u8; 4], inside: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((TILES * side * side * 4) as usize);
    for y in 0..side {
        for tile in 0..TILES {
            for x in 0..side {
                let (across, along) =
                    (x < border || x >= side - border.min(side), y < border || y >= side - border.min(side));
                let framed = match tile {
                    0 => across || along,
                    1 => false,
                    2 => along,
                    3 => across,
                    _ => across && along,
                };
                rgba.extend(if framed { frame } else { inside(x, y) });
            }
        }
    }
    rgba
}

/// How many pixels a side each tile of the starter glass is.
pub const GLASS_SIDE: u32 = 16;

/// The starter glass's five tiles: a pale frame a pixel thick round a clear
/// pane with two short glints, sixteen pixels a tile.
pub fn glass_tiles() -> Vec<u8> {
    let glints = [(3, 5), (4, 4), (5, 3), (10, 11), (11, 10)];
    frame_tiles(GLASS_SIDE, 1, [205, 228, 238, 255], |x, y| match glints.contains(&(x, y)) {
        true => [245, 252, 255, 255],
        // Clear, in the frame's own colour, so a smoothed picture shows no dark fringe.
        false => [205, 228, 238, 0],
    })
}

/// Makes a glass block called `name` in `folder`: its picture of five
/// tiles, a material that cuts its panes out of it, draws it sharp and
/// joins, and the block drawn with it. The picture and the material are
/// given their GUIDs here, for the material and the block to name them by.
/// Fails where a file of one of the three is there already.
pub fn write_glass_block(folder: &Path, name: &str) -> Result<()> {
    let file = |extension: &str| folder.join(format!("{name}.{extension}"));
    for taken in ["png", "material", "block"].map(file).iter().filter(|file| file.exists()) {
        ensure!(false, "{} is there already", taken.display());
    }
    std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    let picture =
        image::RgbaImage::from_raw(TILES * GLASS_SIDE, GLASS_SIDE, glass_tiles()).context("the glass's tiles")?;
    picture.save(file("png")).with_context(|| format!("writing {}", file("png").display()))?;
    let known = |extension: &str| -> Result<AssetGuid> {
        let guid = AssetGuid::new_random();
        let meta = folder.join(format!("{name}.{extension}.{META_EXTENSION}"));
        std::fs::write(&meta, format!("guid = \"{guid}\"\n")).with_context(|| format!("writing {}", meta.display()))?;
        Ok(guid)
    };
    let material = MaterialData {
        color: [1.0; 3],
        roughness: 0.1,
        color_map: Some(known("png")?),
        cutout: Some(0.5),
        pixels: true,
        connected: true,
        ..MaterialData::default()
    };
    std::fs::write(file("material"), toml::to_string(&material)?)?;
    let drawn = Some(known("material")?.to_string());
    let block = BlockDef { material: drawn, hardness: 0.3, ..BlockDef::colored([0.8, 0.9, 0.95]) };
    std::fs::write(file("block"), block.to_text())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_tiles_have_their_borders_where_each_joins_nothing_and_the_same_inside() {
        let tiles = frame_tiles(8, 2, [9, 9, 9, 255], |x, y| [x as u8, y as u8, 0, 255]);
        assert_eq!(tiles.len(), 5 * 8 * 8 * 4);
        let at = |tile: u32, x: u32, y: u32| {
            let pixel = ((y * 40 + tile * 8 + x) * 4) as usize;
            [tiles[pixel], tiles[pixel + 1], tiles[pixel + 2], tiles[pixel + 3]]
        };
        let framed = |tile: u32, x: u32, y: u32| at(tile, x, y) == [9, 9, 9, 255];
        // A corner, the middle of an edge along the top, the middle of one down the side, and the middle.
        let has = |tile: u32| [framed(tile, 0, 0), framed(tile, 4, 1), framed(tile, 1, 4), framed(tile, 4, 4)];
        assert_eq!(has(0), [true, true, true, false], "alone: a border all round");
        assert_eq!(has(1), [false; 4], "filled");
        assert_eq!(has(2), [true, true, false, false], "joined left and right: along its top and bottom");
        assert_eq!(has(3), [true, false, true, false], "joined up and down: down its sides");
        assert_eq!(has(4), [true, false, false, false], "joined all round: its corners alone");
        assert!((0..5).all(|tile| at(tile, 4, 4) == [4, 4, 0, 255]), "the same inside, tile to tile");
        // The glass: clear panes, a solid frame, and as wide as five of it are high.
        let glass = glass_tiles();
        assert_eq!(glass.len() as u32, TILES * GLASS_SIDE * GLASS_SIDE * 4);
        assert_eq!((glass[3], glass[(8 * 80 + 8) * 4 + 3]), (255, 0), "its frame's corner and its pane's middle");
    }

    #[test]
    fn a_glass_block_is_made_with_its_picture_and_a_material_that_joins() {
        let dir = tempfile::tempdir().unwrap();
        let blocks = dir.path().join("Blocks");
        write_glass_block(&blocks, "Glass").unwrap();
        let material: MaterialData =
            toml::from_str(&std::fs::read_to_string(blocks.join("Glass.material")).unwrap()).unwrap();
        assert!(material.connected && material.pixels && material.cutout == Some(0.5));
        // The project knows the picture by the GUID the material names it by.
        let mut content = crate::Content::open(dir.path()).unwrap();
        let picture = content.parse("Blocks/Glass.png").unwrap().guid();
        assert_eq!(picture, material.color_map);
        let (_, block) = content.block("Blocks/Glass.block").expect("the block reads");
        let drawn = content.parse("Blocks/Glass.material").unwrap().guid().map(|guid| guid.to_string());
        assert_eq!(block.material, drawn);
        assert_eq!(image::open(blocks.join("Glass.png")).unwrap().to_rgba8().dimensions(), (80, 16));
        assert!(write_glass_block(&blocks, "Glass").is_err(), "one of that name is there");
    }
}
