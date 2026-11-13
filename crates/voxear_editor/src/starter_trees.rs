//! The starter trees of blocks: an oak, a birch and a pine a generated
//! world of blocks builds by itself, made by **Starter Trees of Blocks**
//! on a world's **Add plant** row.
//!
//! Each is a `.voxplant` in the project's Plants folder, grown afresh at
//! every place so no two are alike, and a `.structure` in its Structures
//! folder that says where it stands, listed under the world's generator.
//! Their logs and leaves are blocks made in the Blocks folder beside the
//! world, each with a small picture drawn here from whole-number hashes
//! and a material for it: the leaves cut out, bushy and swaying, the logs
//! (`BlockShape::Log`) of bark with rings at their ends, so a trunk
//! thins to posts towards its top and its branches are thin logs. The oak and the birch have vines on some of their trees, of the
//! Vine block the starter biomes hang too.
//!
//! Each has a sapling too, a block that grows into it, drawn as a small
//! tree of a stem and leaves, and its plant scatters one to three of them
//! where it lands as it is felled. Its log and its sapling each have an
//! item, in the project's Items folder, that places it: a log felled drops
//! its item, and a sapling item dropped on the ground takes root.
//!
//! Where the world has the starter biomes, each tree keeps to the ones
//! that suit it: oaks in the Forest and the Rainforest, birches in the
//! Forest, the Grassland and the Taiga, pines in the Taiga. A file of a
//! starter's name already there is used as it is, a block of its name in
//! the world's Blocks folder too: nothing a project made is written over.

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_ecs::Entity;
use voxear_engine::Game;
use voxear_render::Renderer;
use voxear_terrain::{
    blocks::{
        block::{BlockDef, BlockShape},
        plant::{PlantDef, PlantKind},
        structure::{GroundFit, StructureDef},
    },
    generate::structures::Placement,
};

use crate::{block_props, objs::PLANTS_FOLDER, project_files, starter_biomes};

/// The folder a project keeps its starter trees' structures in.
const STRUCTURES_FOLDER: &str = "Structures";

/// A starter tree: its name, how it grows, the names and colours of its
/// log and its leaves, the darker marks across its bark where it has any,
/// how many of its trees have vines, how far apart its tries are and how
/// many grow, and the starter biomes it keeps to.
struct StarterTree {
    name: &'static str,
    plant: fn() -> PlantDef,
    log: (&'static str, [f32; 3]),
    leaves: (&'static str, [f32; 3]),
    marks: Option<[f32; 3]>,
    vines: f32,
    spacing: f32,
    chance: f32,
    biomes: &'static [&'static str],
}

const TREES: [StarterTree; 3] = [
    StarterTree {
        name: "Oak",
        plant: || PlantDef {
            height: [7.0, 11.0],
            trunk: 1.2,
            taper: 0.6,
            branch: 0.375,
            bend: 0.25,
            crown_width: [7.0, 10.0],
            crown_height: [5.0, 7.0],
            branches: 0.6,
            leaf_size: 3.6,
            leaf_density: 0.85,
            roots: 3,
            ..PlantDef::default()
        },
        log: ("Oak Log", [0.42, 0.31, 0.2]),
        leaves: ("Oak Leaves", [0.22, 0.46, 0.16]),
        marks: None,
        vines: 0.3,
        spacing: 9.0,
        chance: 0.55,
        biomes: &["Forest", "Rainforest"],
    },
    StarterTree {
        name: "Birch",
        plant: || PlantDef {
            height: [8.0, 12.0],
            trunk: 1.0,
            taper: 0.5,
            branch: 0.25,
            bend: 0.1,
            crown_width: [5.0, 7.0],
            crown_height: [6.0, 8.0],
            branches: 0.5,
            leaf_size: 3.0,
            leaf_density: 0.9,
            roots: 0,
            ..PlantDef::default()
        },
        log: ("Birch Log", [0.86, 0.85, 0.8]),
        leaves: ("Birch Leaves", [0.38, 0.66, 0.2]),
        marks: Some([0.2, 0.2, 0.19]),
        vines: 0.1,
        spacing: 11.0,
        chance: 0.45,
        biomes: &["Forest", "Grassland", "Taiga"],
    },
    StarterTree {
        name: "Pine",
        plant: || PlantDef {
            kind: PlantKind::Conifer,
            height: [12.0, 18.0],
            trunk: 1.2,
            taper: 0.7,
            branch: 0.25,
            bend: 0.0,
            crown_width: [6.0, 8.0],
            crown_height: [9.0, 13.0],
            branches: 0.6,
            leaf_size: 2.4,
            leaf_density: 0.85,
            roots: 2,
            ..PlantDef::default()
        },
        log: ("Pine Log", [0.36, 0.26, 0.18]),
        leaves: ("Pine Needles", [0.14, 0.34, 0.22]),
        marks: None,
        vines: 0.0,
        spacing: 8.0,
        chance: 0.6,
        biomes: &["Taiga"],
    },
];

/// The starters' vine block's name.
const VINE: &str = "Vine";

/// The folder, in the Blocks folder beside the world, the starter trees'
/// pictures and their materials go in.
const PICTURES_FOLDER: &str = "Pictures";

/// The folder of the project its starter trees' items go in.
const ITEMS_FOLDER: &str = "Items";

/// A starter tree's sapling as a block: two crossed sheets that sway,
/// broken at once, growing into its plant.
fn sapling_block(color: [f32; 3], grows: String) -> BlockDef {
    BlockDef { shape: BlockShape::Sapling, sways: true, hardness: 0.0, grows: Some(grows), ..BlockDef::colored(color) }
}

/// A sapling of `leaves` on a stem of `bark`: a stem up the middle of the
/// lower half, and a little crown of leaves over it, clear round them.
fn sapling_picture(bark: [f32; 3], leaves: [f32; 3], salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let (dx, dy) = (x as f32 - 7.5, y as f32 - 5.0);
        let crown = dx * dx * 0.8 + dy * dy < 22.0 && mixed(x, y, salt) % 5 != 0;
        let stem = (7..9).contains(&x) && y >= 6;
        let twig = y == 9 && (5..11).contains(&x) && mixed(x, y, salt + 1) % 2 == 0;
        match () {
            _ if crown => toned(leaves, [0.8, 1.0, 1.2][(mixed(x, y, salt + 2) % 3) as usize], 255),
            _ if stem || twig => toned(bark, 0.9, 255),
            _ => toned(leaves, 0.6, 0),
        }
    })
}

/// The item at `Items/<name>.asset` that places `block`, made where there
/// is none: its path.
fn item_for(game: &mut Game, name: &str, block: &str) -> Result<String> {
    let path = format!("{ITEMS_FOLDER}/{name}.asset");
    if game.content.db().guid_of(&path).is_none() {
        let places = toml::Table::from_iter([("places".to_string(), toml::Value::String(block.to_string()))]);
        write_new(game, &path, &crate::data_assets::text("Voxear.Item", &places)?)?;
        game.content.rescan()?;
    }
    Ok(path)
}

/// A starter tree's log as a block: a log, so its trunk thins and its
/// branches are thin logs.
fn log_block(color: [f32; 3]) -> BlockDef {
    BlockDef { shape: BlockShape::Log, hardness: 2.0, ..BlockDef::colored(color) }
}

/// A starter tree's leaves as a block: bushy, swaying, quick to break.
fn leaves_block(color: [f32; 3]) -> BlockDef {
    BlockDef { shape: BlockShape::Bushy, sways: true, hardness: 0.2, ..BlockDef::colored(color) }
}

/// How many squares a side a starter's picture is, and how many pixels a
/// square is drawn: small squares as a pixel-art picture has, drawn large
/// enough to stay sharp close by.
const SQUARES: u32 = 16;
const SQUARE: u32 = 4;

/// A number from a square's place and `salt`, well mixed: whole numbers
/// alone, so a picture is the same wherever it is made. Places wrap at the
/// picture's edge, so the picture repeats without a seam.
pub(crate) fn mixed(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = salt.wrapping_mul(0x9E37_79B9) ^ (x % SQUARES).wrapping_mul(0x85EB_CA77);
    h = h.rotate_left(13) ^ (y % SQUARES).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// A colour a share lighter or darker, a byte each with `alpha`.
pub(crate) fn toned(color: [f32; 3], tone: f32, alpha: u8) -> [u8; 4] {
    let [r, g, b] = color.map(|part| ((part * tone).clamp(0.0, 1.0) * 255.0).round() as u8);
    [r, g, b, alpha]
}

/// A picture of `SQUARES` squares a side, each `SQUARE` pixels wide, as
/// RGBA: `square` gives each square's colour.
pub(crate) fn picture(square: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let side = SQUARES * SQUARE;
    (0..side * side).flat_map(|at| square(at % side / SQUARE, at / side / SQUARE)).collect()
}

/// Leaves of `color`: in twos and threes, in three shades, with clear gaps
/// between them over about a quarter of the picture.
fn leaves_picture(color: [f32; 3], salt: u32) -> Vec<u8> {
    picture(|x, y| {
        let unit = |hash: u32| (hash >> 8) as f32 / 16_777_216.0;
        let own = unit(mixed(x, y, salt)) * 0.5 + unit(mixed(x / 2, y / 2, salt + 1)) * 0.5;
        if own < 0.34 {
            return toned(color, 0.6, 0);
        }
        toned(color, [0.78, 1.0, 1.24][(mixed(x, y, salt + 2) % 3) as usize], 255)
    })
}

/// Bark of `color`, in streaks running up it; with `marks`, dark dashes
/// across it here and there, as a birch has.
fn bark_picture(color: [f32; 3], marks: Option<[f32; 3]>, salt: u32) -> Vec<u8> {
    picture(|x, y| {
        // A dash a few squares long, on one row in three.
        let dash = mixed(x / 3, y, salt + 3) % 5 == 0 && y % 3 == 1;
        match marks.filter(|_| dash) {
            Some(mark) => toned(mark, 1.0, 255),
            None => toned(color, [0.82, 0.94, 1.0, 1.1][(mixed(x, y / 4, salt) % 4) as usize], 255),
        }
    })
}

/// The end of a log of `color`: rings round its middle, lighter wood
/// inside its bark, as a cut trunk shows.
fn rings_picture(color: [f32; 3], salt: u32) -> Vec<u8> {
    picture(|x, y| {
        // How far from the middle, in squares, a little uneven.
        let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
        let out = (dx * dx + dy * dy).sqrt() + (mixed(x, y, salt + 5) % 3) as f32 * 0.25;
        if out > 6.6 {
            return toned(color, 0.85, 255);
        }
        let ring = (out as u32) % 2 == 0;
        let wood = [color[0] * 1.6 + 0.1, color[1] * 1.6 + 0.08, color[2] * 1.5 + 0.04];
        toned(wood, if ring { 0.82 } else { 1.0 }, 255)
    })
}

/// Writes the picture `rgba` as a PNG at `path` and a material beside it
/// that draws it, cut out where `cut_out` says its clear parts are; gives
/// the material's path. One of that name already there is left as it is.
pub(crate) fn material_with(game: &mut Game, path: &str, rgba: &[u8], cut_out: bool) -> Result<String> {
    let (picture, material) = (format!("{path}.png"), format!("{path}.material"));
    let root = game.project.root.clone();
    if root.join(&material).is_file() {
        return Ok(material);
    }
    if let Some(folder) = root.join(&picture).parent() {
        std::fs::create_dir_all(folder).with_context(|| format!("making the folder of {picture}"))?;
    }
    let file = std::fs::File::create(root.join(&picture)).with_context(|| format!("making {picture}"))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), SQUARES * SQUARE, SQUARES * SQUARE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header()?.write_image_data(rgba)?;
    game.content.rescan()?;
    let color_map = game.content.db().guid_of(&picture).with_context(|| format!("{picture} was not taken up"))?;
    let data = voxear_engine::MaterialData {
        color: [1.0; 3],
        roughness: 1.0,
        color_map: Some(color_map),
        cutout: cut_out.then_some(0.5),
        ..voxear_engine::scene_file::default_material()
    };
    let text = toml::to_string(&data).context("a material writes as TOML")?;
    std::fs::write(root.join(&material), text).with_context(|| format!("making {material}"))?;
    game.content.rescan()?;
    Ok(material)
}

/// The names of the starter trees' leaf blocks: what grows near trees keeps
/// near to these.
pub fn leaf_blocks() -> Vec<&'static str> {
    TREES.iter().map(|tree| tree.leaves.0).collect()
}

/// The block called `name` in `folder`: made there as `block` where there
/// is none. Only that folder is looked in: a starter plant's far block, in
/// the Plants folder, may have a tree's leaves' name and is another block.
pub(crate) fn block_named(game: &mut Game, folder: &str, name: &str, block: &BlockDef) -> Result<String> {
    let here = format!("{folder}/{name}.block");
    if game.content.db().guid_of(&here).is_some() {
        return Ok(here);
    }
    let root = game.project.root.clone();
    let made = project_files::create_block(&root, folder, name, block)?;
    game.content.rescan()?;
    Ok(made)
}

/// Writes `text` at `path` in the project where no file is there.
fn write_new(game: &Game, path: &str, text: &str) -> Result<()> {
    let file = game.project.root.join(path);
    if file.is_file() {
        return Ok(());
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).with_context(|| format!("making the folder of {path}"))?;
    }
    std::fs::write(&file, text).with_context(|| format!("making {path}"))
}

/// Makes the starter trees, their blocks and their structures, and lists
/// the structures under the generator of the world of blocks on `entity`,
/// its ground made again with them. Whether the world changed.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<bool> {
    let world = block_props::world_path(game, entity).context("the world has no asset")?;
    let blocks_folder = block_props::blocks_beside(&world);
    let biomes_folder =
        blocks_folder.rsplit_once('/').map_or("Biomes".to_string(), |(beside, _)| format!("{beside}/Biomes"));
    let reference = |game: &Game, path: &str| game.content.db().guid_of(path).map(|guid| guid.to_string());
    let vine = block_named(game, &blocks_folder, VINE, &starter_biomes::vine_block())?;
    let mut structures = Vec::new();
    for (at, tree) in TREES.into_iter().enumerate() {
        let named = |game: &Game, path: &str| reference(game, path).unwrap_or_else(|| path.to_string());
        // Each block with its picture, where the project has no block of its name: bark for the log, cut-out
        // leaves for the leaves.
        let salt = 100 + at as u32 * 10;
        let has =
            |game: &Game, name: &str| game.content.db().guid_of(&format!("{blocks_folder}/{name}.block")).is_some();
        let mut log_block = log_block(tree.log.1);
        if !has(game, tree.log.0) {
            let pictures = format!("{blocks_folder}/{PICTURES_FOLDER}");
            let bark = bark_picture(tree.log.1, tree.marks, salt);
            let bark = material_with(game, &format!("{pictures}/{}", tree.log.0), &bark, false)?;
            log_block.material = Some(named(game, &bark));
            let rings = rings_picture(tree.log.1, salt);
            let rings = material_with(game, &format!("{pictures}/{} Rings", tree.log.0), &rings, false)?;
            log_block.top = Some(named(game, &rings));
            log_block.bottom = Some(named(game, &rings));
        }
        let mut leaf_block = leaves_block(tree.leaves.1);
        if !has(game, tree.leaves.0) {
            let pictures = format!("{blocks_folder}/{PICTURES_FOLDER}");
            let leaves = leaves_picture(tree.leaves.1, salt + 2);
            let leaves = material_with(game, &format!("{pictures}/{}", tree.leaves.0), &leaves, true)?;
            leaf_block.material = Some(named(game, &leaves));
        }
        // A felled log drops its item.
        let log_path = format!("{blocks_folder}/{}.block", tree.log.0);
        if !has(game, tree.log.0) {
            let item = item_for(game, tree.log.0, &log_path)?;
            log_block.drops = vec![voxear_terrain::blocks::block::BlockDrop { item: Some(named(game, &item)), ..Default::default() }];
        }
        let log = block_named(game, &blocks_folder, tree.log.0, &log_block)?;
        let leaves = block_named(game, &blocks_folder, tree.leaves.0, &leaf_block)?;
        let sapling_name = format!("{} Sapling", tree.name);
        let sapling_path = format!("{blocks_folder}/{sapling_name}.block");
        let plant_path = format!("{PLANTS_FOLDER}/{}.voxplant", tree.name);
        let plant = PlantDef {
            vines: tree.vines,
            wood: Some(named(game, &log)),
            leaves: Some(named(game, &leaves)),
            vine: (tree.vines > 0.0).then(|| named(game, &vine)),
            sapling: Some(sapling_path.clone()),
            saplings: [1, 3],
            ..(tree.plant)()
        };
        write_new(game, &plant_path, &plant.to_text())?;
        game.content.rescan()?;
        // Its sapling, drawn as a small tree, which grows into it; and an item that plants it.
        if !has(game, &sapling_name) {
            let pictures = format!("{blocks_folder}/{PICTURES_FOLDER}");
            let picture = sapling_picture(tree.log.1, tree.leaves.1, salt + 4);
            let material = material_with(game, &format!("{pictures}/{sapling_name}"), &picture, true)?;
            let mut block = sapling_block(tree.leaves.1, named(game, &plant_path));
            block.material = Some(named(game, &material));
            block_named(game, &blocks_folder, &sapling_name, &block)?;
            item_for(game, &sapling_name, &sapling_path)?;
        }
        // Only in the starter biomes that suit it, where the world's project has them.
        let biomes: Vec<String> =
            tree.biomes.iter().filter_map(|biome| reference(game, &format!("{biomes_folder}/{biome}.biome"))).collect();
        let structure = StructureDef {
            ground: GroundFit::Leave,
            sink: 0,
            placement: Placement {
                spacing: tree.spacing,
                separation: tree.spacing * 0.5,
                chance: tree.chance,
                slope: 28.0,
                biomes,
                ..Placement::default()
            },
            plant: Some(named(game, &plant_path)),
            ..StructureDef::default()
        };
        let structure_path = format!("{STRUCTURES_FOLDER}/{}.structure", tree.name);
        write_new(game, &structure_path, &structure.to_text())?;
        structures.push(structure_path);
    }
    game.content.rescan()?;
    let listed: Vec<AssetGuid> = structures.iter().filter_map(|path| game.content.db().guid_of(path)).collect();
    Ok(block_props::list_structures(game, renderer, entity, &listed))
}

#[cfg(test)]
mod tests {
    use voxear_terrain::blocks::plant::PlantPart;

    use super::*;

    #[test]
    fn the_starter_trees_are_an_oak_a_birch_and_a_pine_each_its_own_shape_with_bushy_leaves() {
        let names: Vec<&str> = TREES.iter().map(|tree| tree.name).collect();
        assert_eq!(names, ["Oak", "Birch", "Pine"]);
        assert_eq!(leaf_blocks(), ["Oak Leaves", "Birch Leaves", "Pine Needles"]);
        for tree in &TREES {
            let block = leaves_block(tree.leaves.1);
            assert!(block.shape == BlockShape::Bushy && block.sways, "{}", tree.name);
            assert_eq!(log_block(tree.log.1).shape, BlockShape::Log, "{}", tree.name);
            let sapling = sapling_block(tree.leaves.1, "Plants/Oak.voxplant".into());
            assert_eq!((sapling.shape, sapling.grows.as_deref()), (BlockShape::Sapling, Some("Plants/Oak.voxplant")));
            // A sapling's picture: clear round a crown of leaves on a stem.
            let picture = sapling_picture(tree.log.1, tree.leaves.1, 9);
            let clear = picture.chunks_exact(4).filter(|pixel| pixel[3] == 0).count();
            assert!(clear > picture.len() / 8 && clear < picture.len() / 4 * 3, "{}: {clear} clear", tree.name);
            // Grown from a few seeds: a trunk, and leaves over it; taller than a person, no wider than a house.
            for seed in 0..4 {
                let model = (tree.plant)().grow(seed);
                let (low, high) = model.bounds().unwrap();
                assert!(model.count(PlantPart::Trunk) >= 6 && model.count(PlantPart::Leaf) > 40, "{}", tree.name);
                assert!((7..=22).contains(&high[1]), "{} stands {} blocks", tree.name, high[1]);
                assert!(high[0] - low[0] <= 16 && high[2] - low[2] <= 16, "{}", tree.name);
            }
        }
        // The pine is the tallest and narrows upwards; the birch's trunk is the palest.
        let tall = |at: usize| (TREES[at].plant)().height[1];
        assert!(tall(2) > tall(0) && tall(2) > tall(1));
        assert_eq!((TREES[2].plant)().kind, PlantKind::Conifer);
        let pale = |at: usize| TREES[at].log.1.iter().sum::<f32>();
        assert!(pale(1) > pale(0) * 2.0 && pale(1) > pale(2) * 2.0);
        // Vines on some oaks, on fewer birches, on no pine.
        assert!(TREES[0].vines > TREES[1].vines && TREES[2].vines == 0.0);
    }

    #[test]
    fn the_starter_trees_pictures_are_sixteen_squares_a_side_leaves_with_gaps_and_a_birchs_bark_with_marks() {
        let side = (SQUARES * SQUARE) as usize;
        let at = |rgba: &[u8], x: usize, y: usize| -> [u8; 4] { rgba[(y * side + x) * 4..][..4].try_into().unwrap() };
        for (tree, salt) in TREES.iter().zip([100u32, 110, 120]) {
            let leaves = leaves_picture(tree.leaves.1, salt + 2);
            assert_eq!(leaves.len(), side * side * 4);
            // Each square one colour; an eighth to two fifths of them clear; the rest in three greens.
            for (x, y) in (0..side).flat_map(|y| (0..side).map(move |x| (x, y))) {
                assert_eq!(at(&leaves, x, y), at(&leaves, x / 4 * 4, y / 4 * 4), "{}", tree.name);
            }
            let clear = leaves.chunks_exact(4).filter(|pixel| pixel[3] == 0).count() as f32 / (side * side) as f32;
            assert!((0.12..0.4).contains(&clear), "{}: {clear} of its leaves' picture is clear", tree.name);
            let shades: std::collections::HashSet<[u8; 4]> =
                leaves.chunks_exact(4).filter(|pixel| pixel[3] == 255).map(|pixel| pixel.try_into().unwrap()).collect();
            assert_eq!(shades.len(), 3, "{}", tree.name);
            assert!(shades.iter().all(|shade| shade[1] > shade[0] && shade[1] > shade[2]), "green: {shades:?}");
            // The same again; bark is solid all over.
            assert_eq!(leaves, leaves_picture(tree.leaves.1, salt + 2));
            let bark = bark_picture(tree.log.1, tree.marks, salt);
            assert!(bark.chunks_exact(4).all(|pixel| pixel[3] == 255));
            let dark = bark.chunks_exact(4).filter(|pixel| pixel[0] < 60).count();
            assert_eq!(dark > 0, tree.marks.is_some(), "{}: {dark} dark pixels of bark", tree.name);
        }
    }
}
