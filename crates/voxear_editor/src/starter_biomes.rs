//! The starter biomes: eight kinds of country a generated world of blocks
//! can start from, each with the blocks of its ground, made by **Add
//! Starter Biomes** on a world's list of biomes.
//!
//! Snow, Tundra, Taiga, Grassland, Forest, Desert, Savanna and Rainforest
//! take the climates of the same names (`surfaces::Biome`), so with a
//! generator's own climate a world has different country wherever it is
//! warmer, colder, wetter or drier. Each has rock on its steep ground and
//! sand or gravel along its shore. The wet ones have vines, of a Vine
//! block made with the rest: thick in the Rainforest, some in the Forest,
//! a few in the Taiga and the Grassland.
//!
//! A generator cools 6.5 °C for every 1000 m up, which alone puts snow only
//! on very high ground: a world 12 °C at the sea freezes at 2000 m. So Snow
//! keeps to a height instead: from [`SNOW_LINE`] metres over the sea, in
//! any place but a warm one, where the line lies higher as the climate
//! says. A biome that keeps to a height comes before those that do not, so
//! the heights are snow whatever grows below, and low ground never is.
//! Tundra keeps its climate alone, so cold dry ground is bare at any
//! height. A world cold at the sea has tundra and taiga on its low ground;
//! for snow there too, switch Height off on its Snow.
//!
//! The biomes are made in a Biomes folder and their blocks in a Blocks
//! folder, both beside the world. A biome of a starter's name already
//! there is listed as it is, and a block of a name the project has
//! anywhere is used as it is: nothing a project made is written over.

use anyhow::{Context, Result};
use voxear_assets::{AssetGuid, AssetKind, Mount};
use voxear_ecs::Entity;
use voxear_engine::Game;
use voxear_render::Renderer;
use voxear_terrain::{
    blocks::block::{BlockDef, BlockShape},
    generate::{BiomeDef, BiomeGround, BiomeVines},
    surfaces::Biome,
};

use crate::{block_props, project_files};

/// How high over the sea the snow starts, in metres.
pub const SNOW_LINE: f32 = 260.0;

/// How warm a place may be, in °C, and still be snow on the heights: its
/// ground's own warmth, which is already less for its height.
const SNOW_UP_TO: f32 = 14.0;

/// How steep ground is rock in place of what lies on it, in degrees: under
/// anything that grows, and under snow.
const STEEP: f32 = 45.0;
const SNOW_STEEP: f32 = 58.0;

/// The folders the starters' biomes and blocks go in, beside the world.
const BIOMES_FOLDER: &str = "Biomes";

/// The blocks the starters' ground is made of, each a colour.
const BLOCKS: [(&str, [f32; 3]); 15] = [
    ("Snow", [0.95, 0.96, 0.98]),
    ("Ice", [0.72, 0.84, 0.92]),
    ("Frozen Dirt", [0.47, 0.44, 0.4]),
    ("Moss", [0.24, 0.4, 0.17]),
    ("Needles Floor", [0.33, 0.36, 0.2]),
    ("Grass", [0.33, 0.52, 0.2]),
    ("Dirt", [0.45, 0.32, 0.2]),
    ("Sand", [0.86, 0.78, 0.55]),
    ("Sandstone", [0.76, 0.62, 0.4]),
    ("Dry Grass", [0.7, 0.64, 0.33]),
    ("Red Earth", [0.6, 0.33, 0.2]),
    ("Jungle Floor", [0.2, 0.36, 0.14]),
    ("Mud", [0.33, 0.25, 0.17]),
    ("Stone", [0.5, 0.5, 0.52]),
    ("Gravel", [0.56, 0.54, 0.51]),
];

/// The starters' vine: its name and its colour. A block whose shape is a
/// vine, that sways.
const VINE: (&str, [f32; 3]) = ("Vine", [0.2, 0.46, 0.14]);

/// The starters' vine as a block.
pub fn vine_block() -> BlockDef {
    BlockDef { shape: BlockShape::Vine, sways: true, hardness: 0.2, ..BlockDef::colored(VINE.1) }
}

/// A starter biome: the climate it takes, its colour on maps, its
/// ground's blocks by name: on top, under it and how deep, deep down, on
/// steep ground and along the shore; and how many vines hang in it.
struct Starter {
    climate: Biome,
    color: [f32; 3],
    top: &'static str,
    under: (&'static str, u32),
    deep: &'static str,
    steep: &'static str,
    shore: &'static str,
    vines: f32,
}

const STARTERS: [Starter; 8] = [
    Starter {
        climate: Biome::Snow,
        color: [0.93, 0.95, 0.98],
        top: "Snow",
        under: ("Ice", 2),
        deep: "Stone",
        steep: "Stone",
        shore: "Gravel",
        vines: 0.0,
    },
    Starter {
        climate: Biome::Tundra,
        color: [0.55, 0.53, 0.47],
        top: "Frozen Dirt",
        under: ("Frozen Dirt", 3),
        deep: "Stone",
        steep: "Stone",
        shore: "Gravel",
        vines: 0.0,
    },
    Starter {
        climate: Biome::Taiga,
        color: [0.27, 0.4, 0.3],
        top: "Needles Floor",
        under: ("Dirt", 3),
        deep: "Stone",
        steep: "Stone",
        shore: "Gravel",
        vines: 0.06,
    },
    Starter {
        climate: Biome::Grassland,
        color: [0.45, 0.62, 0.3],
        top: "Grass",
        under: ("Dirt", 3),
        deep: "Stone",
        steep: "Stone",
        shore: "Sand",
        vines: 0.08,
    },
    Starter {
        climate: Biome::Forest,
        color: [0.24, 0.45, 0.2],
        top: "Moss",
        under: ("Dirt", 4),
        deep: "Stone",
        steep: "Stone",
        shore: "Gravel",
        vines: 0.22,
    },
    Starter {
        climate: Biome::Desert,
        color: [0.9, 0.8, 0.55],
        top: "Sand",
        under: ("Sandstone", 5),
        deep: "Sandstone",
        steep: "Sandstone",
        shore: "Sand",
        vines: 0.0,
    },
    Starter {
        climate: Biome::Savanna,
        color: [0.75, 0.68, 0.36],
        top: "Dry Grass",
        under: ("Red Earth", 3),
        deep: "Stone",
        steep: "Stone",
        shore: "Sand",
        vines: 0.0,
    },
    Starter {
        climate: Biome::Rainforest,
        color: [0.13, 0.4, 0.16],
        top: "Jungle Floor",
        under: ("Mud", 3),
        deep: "Stone",
        steep: "Stone",
        shore: "Sand",
        vines: 0.5,
    },
];

/// The starters as biomes, each by its name, the blocks of its ground
/// named as `block` names them: by reference, for a file, or as they are,
/// for a look at where each lies.
pub fn biomes(mut block: impl FnMut(&str) -> String) -> Vec<(&'static str, BiomeDef)> {
    let made = |starter: &Starter, block: &mut dyn FnMut(&str) -> String| {
        let (temperature, humidity) = starter.climate.ranges();
        let mut biome = BiomeDef {
            color: starter.color,
            temperature: temperature.unwrap_or([-60.0, 60.0]),
            humidity: humidity.unwrap_or([0.0, 1.0]),
            height: None,
            sunk: None,
            ground: BiomeGround {
                top: Some(block(starter.top)),
                under: Some(block(starter.under.0)),
                under_depth: starter.under.1,
                deep: Some(block(starter.deep)),
                steep: Some(block(starter.steep)),
                steep_slope: STEEP,
                shore: Some(block(starter.shore)),
                shore_height: 2.0,
            },
            vines: match starter.vines > 0.0 {
                true => BiomeVines { block: Some(block(VINE.0)), amount: starter.vines, ..BiomeVines::default() },
                false => BiomeVines::default(),
            },
        };
        // The heights are snow in any place but a warm one, and low ground never. Snow lies on steeper ground than
        // anything grows on, so a mountain's top is white with rock only in its faces.
        if starter.climate == Biome::Snow {
            biome.temperature[1] = SNOW_UP_TO;
            biome.height = Some([SNOW_LINE, 10_000.0]);
            biome.ground.steep_slope = SNOW_STEEP;
        }
        biome
    };
    STARTERS.iter().map(|starter| (starter.climate.name(), made(starter, &mut block))).collect()
}

/// Makes the starter biomes and the blocks they need beside the world of
/// blocks on `entity`, and lists them on it, its ground made again with
/// them. Whether the world changed.
pub fn add(game: &mut Game, renderer: Option<&mut Renderer>, entity: Entity) -> Result<bool> {
    let world = block_props::world_path(game, entity).context("the world has no asset")?;
    let blocks_folder = block_props::blocks_beside(&world);
    let biomes_folder = match blocks_folder.rsplit_once('/') {
        Some((beside, _)) => format!("{beside}/{BIOMES_FOLDER}"),
        None => BIOMES_FOLDER.to_string(),
    };
    let root = game.project.root.clone();
    // Each block by its name: the project's own where it has one of that name, or made in the Blocks folder.
    let mut made_blocks = std::collections::HashMap::new();
    let ground = BLOCKS.into_iter().map(|(name, color)| (name, BlockDef::colored(color)));
    for (name, block) in ground.chain([(VINE.0, vine_block())]) {
        let path = match block_named(game, &blocks_folder, name) {
            Some(path) => path,
            None => project_files::create_block(&root, &blocks_folder, name, &block)?,
        };
        made_blocks.insert(name, path);
    }
    game.content.rescan()?;
    let reference = |game: &Game, path: &str| game.content.db().guid_of(path).map(|guid| guid.to_string());
    // Each biome naming its blocks by their GUIDs: one of its name there already is listed as it is.
    let mut paths = Vec::new();
    let starters = biomes(|name| {
        let path = &made_blocks[name];
        reference(game, path).unwrap_or_else(|| path.clone())
    });
    for (name, biome) in starters {
        let path = format!("{biomes_folder}/{name}.biome");
        if !root.join(&path).is_file() {
            std::fs::create_dir_all(root.join(&biomes_folder)).with_context(|| format!("making {biomes_folder}"))?;
            std::fs::write(root.join(&path), project_files::biome_text(&biome))
                .with_context(|| format!("making {path}"))?;
        }
        paths.push(path);
    }
    game.content.rescan()?;
    let listed: Vec<AssetGuid> = paths.iter().filter_map(|path| game.content.db().guid_of(path)).collect();
    Ok(block_props::list_biomes(game, renderer, entity, &listed))
}

/// The block of the project called `name`: the one in `folder` where there
/// is one, or else any of the project's own, the first by path.
fn block_named(game: &Game, folder: &str, name: &str) -> Option<String> {
    let here = format!("{folder}/{name}.block");
    let db = game.content.db();
    if db.guid_of(&here).is_some() {
        return Some(here);
    }
    let is = |path: &str| {
        let file = path.rsplit('/').next().unwrap_or(path);
        file.strip_suffix(".block").is_some_and(|stem| stem.eq_ignore_ascii_case(name))
    };
    let mut found: Vec<&String> = db
        .entries()
        .filter(|entry| entry.kind == AssetKind::Block && entry.mount == Mount::Project && is(&entry.path))
        .map(|entry| &entry.path)
        .collect();
    found.sort();
    found.first().map(|path| path.to_string())
}

#[cfg(test)]
mod tests {
    use voxear_terrain::generate::{Biomes, Preset, biomes::Place};

    use super::*;

    /// How much of the ground of `preset`'s world, by height over the sea in bands of `band` metres, is each
    /// starter: the bands from the lowest, each with how many places it has and each starter's share of them.
    fn shares(preset: Preset, band: f32) -> Vec<(f32, usize, [f32; 8])> {
        let world = preset.world();
        let starters: Vec<BiomeDef> = biomes(|name| name.to_string()).into_iter().map(|(_, biome)| biome).collect();
        let placed = Biomes::new(starters, 15.0, 0.3, world.seed());
        let mut bands: std::collections::BTreeMap<i32, (usize, [usize; 8])> = Default::default();
        // Every 40 m over 16 km a side.
        for z in 0..400 {
            for x in 0..400 {
                let (x, z) = (x as f32 * 40.0, z as f32 * 40.0);
                let height = world.height_at(x, z);
                let above_sea = height - world.sea_level();
                if above_sea < 0.0 {
                    continue;
                }
                let (temperature, humidity) = world.climate(x, z, height);
                let biome = placed.at(x, z, Place { temperature, humidity, above_sea, sunk: 0.0 }).unwrap();
                let entry = bands.entry((above_sea / band).floor() as i32).or_default();
                entry.0 += 1;
                entry.1[biome] += 1;
            }
        }
        bands
            .into_iter()
            .map(|(at, (count, each))| (at as f32 * band, count, each.map(|n| n as f32 / count as f32)))
            .collect()
    }

    #[test]
    fn the_starters_put_snow_on_the_heights_and_none_on_low_ground_that_is_not_freezing() {
        let named: Vec<&str> = biomes(|name| name.to_string()).into_iter().map(|(name, _)| name).collect();
        assert_eq!(named, ["Snow", "Tundra", "Taiga", "Grassland", "Forest", "Desert", "Savanna", "Rainforest"]);
        for preset in [Preset::Alpine, Preset::RollingHills] {
            let bands = shares(preset, 50.0);
            for (from, count, each) in &bands {
                let shown: Vec<String> = each.iter().map(|share| format!("{:>3.0}", share * 100.0)).collect();
                println!("{:>14} {from:>5} m {count:>6}: {}", preset.name(), shown.join(" "));
            }
            // Well under the snow line no place is snow.
            let low_snow: f32 = bands.iter().filter(|(from, ..)| *from < 150.0).map(|(_, _, each)| each[0]).sum();
            assert_eq!(low_snow, 0.0, "{}: snow on low ground", preset.name());
            // Over the snow line, all of it is.
            for (from, _, each) in bands.iter().filter(|(from, ..)| *from >= SNOW_LINE + 50.0) {
                assert!(each[0] > 0.97, "{}: at {from} m, {:.2} of it is snow", preset.name(), each[0]);
            }
        }
        // The mountains reach the snow, and the hills do not.
        let top = |preset: Preset| shares(preset, 50.0).last().map(|(from, ..)| *from).unwrap();
        assert!(top(Preset::Alpine) > SNOW_LINE && top(Preset::RollingHills) < SNOW_LINE - 100.0);
    }

    #[test]
    fn every_block_a_starter_names_is_one_of_the_starters_blocks() {
        let blocks: Vec<&str> = BLOCKS.iter().map(|(name, _)| *name).chain([VINE.0]).collect();
        let mut named = std::collections::BTreeSet::new();
        biomes(|name| {
            named.insert(name.to_string());
            name.to_string()
        });
        assert!(named.iter().all(|name| blocks.contains(&name.as_str())), "{named:?}");
        assert_eq!(named.len(), blocks.len(), "and every block is some starter's: {named:?}");
        // Vines hang where it is wet: the most in the Rainforest, none in the Desert or on the Snow.
        let vines = |name: &str| {
            let (_, biome) = biomes(|name| name.to_string()).into_iter().find(|(named, _)| *named == name).unwrap();
            biome.vines.block.map_or(0.0, |_| biome.vines.amount)
        };
        assert!(vines("Rainforest") > vines("Forest") && vines("Forest") > vines("Grassland"));
        assert_eq!((vines("Desert"), vines("Snow")), (0.0, 0.0));
        assert_eq!(vine_block().shape, BlockShape::Vine);
    }
}
