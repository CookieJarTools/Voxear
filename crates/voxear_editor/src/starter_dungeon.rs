//! The starter dungeon: a temple dungeon a generated world of blocks builds
//! by itself, made by **Add Temple Dungeon** (and `dungeon.starter` from
//! the command line).
//!
//! Its blocks are made in a Dungeon folder beside the world's Blocks, each
//! with a small picture drawn here from whole-number hashes: dressed stone
//! in courses, carved trim, flagstones, rubble, cracked and mossy stone, a
//! slab and stairs of the stone, pale statue stone, iron bars, fire and
//! blue crystal that glow, glowing lava that is hot, water and vines. Its
//! structure, in the project's Structures folder, names them by what they
//! are for and is listed under the world's generator. A block or a file of
//! a starter's name already there is used as it is: nothing a project made
//! is written over.

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_ecs::Entity;
use voxear_engine::Game;
use voxear_render::Renderer;
use voxear_terrain::{
    blocks::{
        block::{BlockDef, BlockHeat, BlockShape, Liquid},
        structure::{DungeonBlockNames, DungeonDef, GroundFit, StructureDef},
    },
    generate::structures::Placement,
};

use crate::{
    block_props,
    starter_trees::{block_named, mixed, picture, toned},
};

/// Where the starter dungeon's structure goes.
pub const STRUCTURE: &str = "Structures/Temple Dungeon.structure";

const STONE: [f32; 3] = [0.47, 0.45, 0.41];
const TRIM: [f32; 3] = [0.64, 0.6, 0.52];
const FLAGS: [f32; 3] = [0.52, 0.49, 0.44];
const STATUE: [f32; 3] = [0.6, 0.58, 0.54];
const IRON: [f32; 3] = [0.2, 0.2, 0.22];
const MOSS: [f32; 3] = [0.3, 0.42, 0.2];

fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / 16_777_216.0
}

/// Dressed stone laid in courses four squares high, each stone eight long,
/// every other course set half a stone along, with darker joints; cracks or
/// moss over it where asked.
fn stone_picture(color: [f32; 3], salt: u32, cracks: bool, moss: bool) -> Vec<u8> {
    picture(|x, y| {
        let course = y / 4;
        let shift = if course % 2 == 0 { 0 } else { 4 };
        let stone = (x + shift) / 8;
        let joint = y % 4 == 3 || (x + shift) % 8 == 7;
        if joint {
            return toned(color, 0.62, 255);
        }
        let tone = 0.9 + unit(mixed(stone, course, salt)) * 0.18 + unit(mixed(x, y, salt + 1)) * 0.06;
        if cracks {
            // A crack wandering down across a stone or two.
            let line = (x as i32 - 8 + (mixed(0, y / 3, salt + 7) % 3) as i32 - 1).abs() <= 0 && y % 5 != 0;
            if line {
                return toned(color, 0.5, 255);
            }
        }
        if moss {
            let patch = unit(mixed(x / 2, y / 2, salt + 9)) * 0.6 + unit(mixed(x, y, salt + 10)) * 0.4;
            if patch > 0.55 + y as f32 * 0.015 {
                return toned(MOSS, 0.85 + unit(mixed(x, y, salt + 11)) * 0.3, 255);
            }
        }
        toned(color, tone, 255)
    })
}

/// Carved trim: a raised border round a sunken panel, lit from above.
fn trim_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let edge = x.min(y).min(15 - x).min(15 - y);
        let tone = match edge {
            0 => 0.7,
            1 if y < 8 || x < 8 => 1.12,
            1 => 0.86,
            2 => 0.8,
            _ => 0.98 + unit(mixed(x, y, salt)) * 0.06,
        };
        toned(TRIM, tone, 255)
    })
}

/// Flagstones: large slabs of a few sizes, uneven in tone, gaps between.
fn flags_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let row = y / 8;
        let shift = if row % 2 == 0 { 0 } else { 5 };
        let gap = y % 8 == 7 || (x + shift) % 10 == 9;
        if gap {
            return toned(FLAGS, 0.55, 255);
        }
        let slab = (x + shift) / 10;
        toned(FLAGS, 0.88 + unit(mixed(slab, row, salt)) * 0.2 + unit(mixed(x, y, salt + 2)) * 0.05, 255)
    })
}

/// Rubble: stones of many sizes and tones heaped together, each the
/// squares nearest its own middle, dark where two meet.
fn rubble_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        // The middles: one in each cell of four squares, set somewhere in it.
        let middle = |cx: u32, cy: u32| {
            let h = mixed(cx, cy, salt);
            ((cx * 4 + h % 4) as f32, (cy * 4 + (h >> 8) % 4) as f32)
        };
        let (mut first, mut second, mut nearest) = (f32::MAX, f32::MAX, (0, 0));
        for dy in 0..3u32 {
            for dx in 0..3u32 {
                let (cx, cy) = ((x / 4 + 4 + dx - 1) % 4, (y / 4 + 4 + dy - 1) % 4);
                let (mx, my) = middle(cx, cy);
                // Wrapped, so the picture repeats without a seam.
                let wrap = |d: f32| {
                    let d = d.abs();
                    d.min(16.0 - d)
                };
                let d = wrap(mx - x as f32).powi(2) + wrap(my - y as f32).powi(2);
                if d < first {
                    (second, first, nearest) = (first, d, (cx, cy));
                } else if d < second {
                    second = d;
                }
            }
        }
        if second.sqrt() - first.sqrt() < 0.7 {
            return toned(STONE, 0.5, 255);
        }
        let tone = 0.72 + unit(mixed(nearest.0, nearest.1, salt + 3)) * 0.4 - first.sqrt() * 0.03;
        toned(STONE, tone, 255)
    })
}

/// Pale statue stone, smooth with a faint grain.
fn statue_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| toned(STATUE, 0.95 + unit(mixed(x / 2, y, salt)) * 0.08, 255))
}

/// Iron: dark, a little lighter at its edges.
fn iron_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| toned(IRON, 0.9 + unit(mixed(x, y, salt)) * 0.25, 255))
}

/// Fire: flames rising from a bed of coals, yellow at their hearts.
fn fire_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let height = 15 - y;
        let tongue = unit(mixed(x, 0, salt)) * 10.0 + 4.0;
        if (height as f32) > tongue {
            return [40, 18, 8, 255];
        }
        let heat = 1.0 - height as f32 / tongue;
        let color = [1.0, 0.35 + heat * 0.55, 0.08 + heat * 0.3];
        toned(color, 0.9 + unit(mixed(x, y, salt + 1)) * 0.1, 255)
    })
}

/// Blue crystal: facets in three blues, bright edges.
fn crystal_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let facet = (x + y) / 5 + (x + 16 - y) / 7;
        let tone = [0.75, 1.0, 1.2][(mixed(facet, 0, salt) % 3) as usize];
        toned([0.35, 0.7, 1.0], tone, 255)
    })
}

/// Lava: bright channels in a dark crust.
fn lava_picture(salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let flow = unit(mixed(x / 3, y / 2, salt)) * 0.6 + unit(mixed(x, y, salt + 1)) * 0.4;
        if flow > 0.62 { toned([0.25, 0.1, 0.06], 1.0, 255) } else { toned([1.0, 0.42 + flow * 0.4, 0.08], 1.0, 255) }
    })
}

/// Writes the picture `rgba` as a PNG at `path` and a material beside it
/// that draws it, glowing `glow` times its picture's colour where `glow`
/// is not black; gives the material's path. One already there is left.
fn material(game: &mut Game, path: &str, rgba: &[u8], glow: [f32; 3]) -> Result<String> {
    let made = crate::starter_trees::material_with(game, path, rgba, false)?;
    if glow == [0.0; 3] {
        return Ok(made);
    }
    let root = game.project.root.clone();
    let text = std::fs::read_to_string(root.join(&made)).with_context(|| format!("reading {made}"))?;
    let mut data: voxear_engine::MaterialData = toml::from_str(&text).context("a material reads as TOML")?;
    if data.emissive == [0.0; 3] {
        let picture = game.content.db().guid_of(&format!("{path}.png"));
        data.emissive = glow;
        data.emissive_map = picture;
        std::fs::write(root.join(&made), toml::to_string(&data).context("a material writes as TOML")?)
            .with_context(|| format!("writing {made}"))?;
        game.content.rescan()?;
    }
    Ok(made)
}

/// A dungeon's blocks, each by its name and what makes it: a shape, a
/// colour, a picture, a glow, its heat.
struct Starter {
    name: &'static str,
    shape: BlockShape,
    color: [f32; 3],
    picture: Option<fn(u32) -> Vec<u8>>,
    glow: [f32; 3],
    hot: bool,
}

const fn starter(name: &'static str, shape: BlockShape, color: [f32; 3], picture: fn(u32) -> Vec<u8>) -> Starter {
    Starter { name, shape, color, picture: Some(picture), glow: [0.0; 3], hot: false }
}

fn starters() -> Vec<Starter> {
    use BlockShape::{Cube, Fence, Slab, Stairs};
    vec![
        starter("Temple Stone", Cube, STONE, |salt| stone_picture(STONE, salt, false, false)),
        starter("Temple Trim", Cube, TRIM, trim_picture),
        starter("Flagstones", Cube, FLAGS, flags_picture),
        starter("Rubble", Cube, STONE, rubble_picture),
        starter("Cracked Temple Stone", Cube, STONE, |salt| stone_picture(STONE, salt, true, false)),
        starter("Mossy Temple Stone", Cube, STONE, |salt| stone_picture(STONE, salt, false, true)),
        starter("Flagstone Slab", Slab, FLAGS, flags_picture),
        starter("Temple Stairs", Stairs, FLAGS, flags_picture),
        starter("Statue Stone", Cube, STATUE, statue_picture),
        starter("Iron Bars", Fence, IRON, iron_picture),
        Starter { glow: [6.0, 3.0, 1.0], hot: true, ..starter("Brazier Fire", Cube, [1.0, 0.6, 0.2], fire_picture) },
        Starter { glow: [1.0, 2.2, 4.0], ..starter("Blue Crystal", Cube, [0.4, 0.7, 1.0], crystal_picture) },
        Starter { glow: [5.0, 1.6, 0.3], hot: true, ..starter("Temple Lava", Cube, [1.0, 0.4, 0.1], lava_picture) },
        Starter { picture: None, ..starter("Temple Water", Cube, [0.12, 0.36, 0.3], statue_picture) },
    ]
}

/// The structure of the starter dungeon, its blocks named by `named` (a
/// block's name to its reference): a temple dungeon, medium, of one to
/// three floors, a site every 900 m or so, on land.
pub fn structure(named: &dyn Fn(&str) -> Option<String>) -> StructureDef {
    let blocks = DungeonBlockNames {
        stone: named("Temple Stone"),
        trim: named("Temple Trim"),
        floor: named("Flagstones"),
        rubble: named("Rubble"),
        cracked: named("Cracked Temple Stone"),
        mossy: named("Mossy Temple Stone"),
        slab: named("Flagstone Slab"),
        stairs: named("Temple Stairs"),
        statue: named("Statue Stone"),
        metal: named("Iron Bars"),
        light: named("Brazier Fire"),
        cold_light: named("Blue Crystal"),
        lava: named("Temple Lava"),
        water: named("Temple Water"),
        vine: named("Vine"),
    };
    StructureDef {
        ground: GroundFit::Leave,
        sink: 0,
        placement: Placement {
            spacing: 900.0,
            separation: 300.0,
            chance: 0.8,
            apart: 120.0,
            slope: 80.0,
            height: Some([4.0, 2000.0]),
            ..Placement::default()
        },
        dungeon: Some(DungeonDef { floors: [1, 3], blocks, ..DungeonDef::default() }),
        ..StructureDef::default()
    }
}

/// Makes the starter dungeon's blocks and structure, and lists the
/// structure first under the generator of the world of blocks on `entity`,
/// its ground made again with it. Whether the world changed, and the
/// structure's path.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<(bool, String)> {
    let world = block_props::world_path(game, entity).context("the world has no asset")?;
    let folder = format!("{}/Dungeon", block_props::blocks_beside(&world));
    let pictures = format!("{folder}/Pictures");
    let reference = |game: &Game, path: &str| game.content.db().guid_of(path).map(|guid| guid.to_string());
    for (at, made) in starters().into_iter().enumerate() {
        let path = format!("{folder}/{}.block", made.name);
        if game.content.db().guid_of(&path).is_some() {
            continue;
        }
        let mut block = BlockDef { shape: made.shape, hardness: 3.0, ..BlockDef::colored(made.color) };
        if made.name == "Temple Water" {
            block.liquid = Liquid::Water;
            block.hardness = 0.0;
        }
        if let Some(draw) = made.picture.filter(|_| block.liquid == Liquid::None) {
            let rgba = draw(300 + at as u32 * 7);
            let material = material(game, &format!("{pictures}/{}", made.name), &rgba, made.glow)?;
            block.material = Some(reference(game, &material).unwrap_or(material));
        }
        if made.hot {
            block.heat = Some(BlockHeat::default());
        }
        block_named(game, &folder, made.name, &block)?;
    }
    let vine_folder = block_props::blocks_beside(&world);
    block_named(game, &vine_folder, "Vine", &crate::starter_biomes::vine_block())?;
    game.content.rescan()?;
    let named = |name: &str| {
        let path = if name == "Vine" { format!("{vine_folder}/Vine.block") } else { format!("{folder}/{name}.block") };
        reference(game, &path)
    };
    let structure = structure(&named);
    let file = game.project.root.join(STRUCTURE);
    if !file.is_file() {
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).context("making the Structures folder")?;
        }
        std::fs::write(&file, structure.to_text()).with_context(|| format!("making {STRUCTURE}"))?;
        game.content.rescan()?;
    }
    let guid: AssetGuid = game.content.db().guid_of(STRUCTURE).context("the dungeon's structure was not taken up")?;
    // First, so the woods and houses listed after it keep clear of its entrance.
    Ok((block_props::list_structure_first(game, renderer, entity, guid), STRUCTURE.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starter_dungeons_pictures_are_whole_and_its_structure_reads_back() {
        let side = (16 * 4 * 16 * 4 * 4) as usize;
        for (at, made) in starters().into_iter().enumerate() {
            if let Some(draw) = made.picture {
                let rgba = draw(at as u32);
                assert_eq!(rgba.len(), side, "{}", made.name);
                assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255), "{} is solid", made.name);
            }
        }
        let named = |name: &str| Some(format!("Blocks/Dungeon/{name}.block"));
        let def = structure(&named);
        assert_eq!(StructureDef::parse(&def.to_text()).unwrap(), def);
        let dungeon = def.dungeon.unwrap();
        assert_eq!(dungeon.blocks.light.as_deref(), Some("Blocks/Dungeon/Brazier Fire.block"));
    }
}
