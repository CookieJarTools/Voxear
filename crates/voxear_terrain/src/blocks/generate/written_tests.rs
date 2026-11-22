//! A world's land written into its generator makes the ground it made.

use std::sync::Arc;

use super::*;
use crate::{
    blocks::{
        chunk::AIR,
        file::{BlockFile, BlockKind, Ground},
    },
    generate::{BiomeBlocks, BiomeDef, BiomeLand, Biomes, Graph, Landform, Regions, WorldBiomes, nodes::Levels},
};

/// A world of 2 m blocks on mountains 300 m high, its land in levels 90 m
/// across, of mountains and of pillars, with rivers and caves, kept clear
/// round its start: everything a world lays into its generator.
fn levelled() -> (BlockFile, WorldGen, Levels, Vec<BiomeDef>) {
    let mut land = crate::generate::Preset::Alpine.landmass();
    land.height = 300.0;
    let world = WorldGen::from(land);
    let levels = Levels { size: 90.0, levels: 3, low: 4.0, high: 64.0, ..Levels::default() };
    let mountains = BiomeLand { height: 60.0, size: 120.0, ..BiomeLand::of(Landform::Mountains) };
    let pillars = BiomeLand { pillar_height: 40.0, ..BiomeLand::of(Landform::Pillars) };
    let list = vec![
        BiomeDef { temperature: [-60.0, 2.0], humidity: [0.0, 1.0], land: mountains, ..BiomeDef::default() },
        BiomeDef { temperature: [2.0, 60.0], humidity: [0.0, 1.0], land: pillars, ..BiomeDef::default() },
    ];
    let mut file = BlockFile {
        block_size: 2.0,
        ground: Ground { size: [64, 64], ..Ground::default() },
        generate: Some(BlockGenerate {
            world: Some("alpine".into()),
            position: [500.0, 800.0],
            bottom: -40,
            overhangs: 3.0,
            caves: 0.4,
            rivers: true,
            land_from: 20.0,
            sea: AIR,
            levels: Some(levels.clone()),
            ..BlockGenerate::default()
        }),
        ..BlockFile::default()
    };
    file.blocks.push(BlockKind::colored("Moss", [0.4, 0.5, 0.3]));
    file.blocks.push(BlockKind::colored("Sand", [0.8, 0.7, 0.5]));
    (file, world, levels, list)
}

/// `file` with its biomes placed as a world places them: in regions read
/// from `raw`, the generator as its file has it, laid in levels.
fn placed(mut file: BlockFile, raw: &Arc<WorldGen>, levels: &Levels, list: &[BiomeDef]) -> BlockFile {
    let biomes = Biomes::new(list.to_vec(), 15.0, 0.3, raw.seed());
    let generate = file.generate.as_mut().unwrap();
    let keep = Some(generate.position);
    let regions = Regions::new(levels.size, 16.0, biomes.clone(), raw.clone());
    let regions = Arc::new(regions.with_levels(crate::generate::LevelPlan { keep, ..levels.plan() }));
    let blocks =
        vec![BiomeBlocks { top: 4, ..BiomeBlocks::default() }, BiomeBlocks { top: 5, ..BiomeBlocks::default() }];
    generate.placed = Some(Arc::new(WorldBiomes { painted: None, regions: Some(regions), biomes, blocks }));
    file
}

#[test]
fn a_worlds_land_written_into_its_generator_makes_the_same_ground_framed_part_by_part() {
    let (file, raw, levels, list) = levelled();
    let raw = Arc::new(raw);
    let before_file = placed(file.clone(), &raw, &levels, &list);
    let generate = before_file.generate.as_ref().unwrap();
    assert!(lays_land(generate));
    let before_world = landed(generate, &raw);
    let before = before_file.make_ground(Some(&before_world));

    // Written into the generator, saved and read back, with the world laying none in itself.
    let names = ["Crags".to_string(), "Pillared".to_string()];
    let graph = write_land(generate, &raw, before_file.block_size, &names).expect("land to write");
    let text = raw.with_graph(graph.clone()).to_text();
    let written = Arc::new(WorldGen::parse(&text).expect("the written generator reads"));
    assert_eq!(written.graph(), &graph, "kept by its file as it was written");
    let mut after_file = file.clone();
    let after_generate = after_file.generate.as_mut().unwrap();
    after_generate.land_written = true;
    let after_file = placed(after_file, &written, &levels, &list);
    let after_generate = after_file.generate.as_ref().unwrap();
    assert!(!lays_land(after_generate), "nothing laid in twice");
    let after_world = landed(after_generate, &written);

    // The same height and climate everywhere over the world and round it, and the same blocks.
    let (mut differ, mut most) = (0, 0.0f32);
    for k in 0..48 * 48 {
        let (x, z) = (440.0 + (k % 48) as f32 * 5.0, 740.0 + (k / 48) as f32 * 5.0);
        let (a, b) = (before_world.height_at(x, z), after_world.height_at(x, z));
        if a.to_bits() != b.to_bits() {
            differ += 1;
            most = most.max((a - b).abs());
        }
        let (a, b) = (before_world.climate(x, z, a), after_world.climate(x, z, b));
        assert_eq!(a, b, "climate at ({x}, {z})");
    }
    assert_eq!(differ, 0, "heights differ at {differ} places, by up to {most} m");
    let tops = |map: &crate::blocks::BlockMap| -> Vec<i32> {
        (0..64 * 64).map(|k| (-40..400).rev().find(|&y| map.get([k % 64, y, k / 64]) != AIR).unwrap_or(-41)).collect()
    };
    let (high, low) = (tops(&before), tops(&before));
    assert!(high.iter().max() > low.iter().min(), "ground that is not all one level");
    assert_eq!(tops(&after_file.make_ground(Some(&after_world))), tops(&before), "the tops of the columns");
    assert!(after_file.make_ground(Some(&after_world)) == before, "the same blocks throughout");

    // The world's own step undone, its settings back but the generator as written: no land laid in again, and
    // none written in twice.
    let undone = placed(file.clone(), &written, &levels, &list);
    let undone_generate = undone.generate.as_ref().unwrap();
    let undone_world = landed(undone_generate, &written);
    let same = (0..48 * 48).all(|k| {
        let (x, z) = (440.0 + (k % 48) as f32 * 5.0, 740.0 + (k / 48) as f32 * 5.0);
        undone_world.height_at(x, z).to_bits() == before_world.height_at(x, z).to_bits()
    });
    assert!(same, "the undone world's heights");
    assert!(undone.make_ground(Some(&undone_world)) == before, "the undone world's blocks");
    assert!(write_land(undone_generate, &written, undone.block_size, &names).is_none());

    // Each part framed in a comment that holds its nodes.
    let titles: Vec<&str> = graph.comments.iter().map(|comment| comment.title.as_str()).collect();
    for title in [
        "Where the Land Rises",
        "Levels",
        "Crags Land",
        "Mountains Cut to Massifs",
        "Pillared Land",
        "Floors and Walls",
        "Climate Counted from the Floor",
        "Rivers",
        "Overhangs and Caves",
        "Spires and Arches",
        "Pillars, Bridges and Overhangs",
    ] {
        assert!(titles.contains(&title), "no {title} among {titles:?}");
    }
    let raw_ids: Vec<u32> = raw.graph().nodes.iter().map(|node| node.id.0).collect();
    for node in graph.nodes.iter().filter(|node| !raw_ids.contains(&node.id.0)) {
        let inside = |comment: &crate::generate::Comment| {
            let [x, y] = node.at;
            x >= comment.at[0]
                && y >= comment.at[1]
                && x < comment.at[0] + comment.size[0]
                && y < comment.at[1] + comment.size[1]
        };
        assert!(graph.comments.iter().any(inside), "{} at {:?} is in no comment", node.kind.title(), node.at);
    }
    // The frames side by side, none over another but those within.
    let massifs = graph.comments.iter().find(|comment| comment.title == "Mountains Cut to Massifs").unwrap();
    let crags = graph.comments.iter().find(|comment| comment.title == "Crags Land").unwrap();
    assert!(massifs.at[0] >= crags.at[0] && massifs.at[0] + massifs.size[0] <= crags.at[0] + crags.size[0] + 1.0);
    // The world's own regions are given to the nodes that keep to them as the world loads, and kept by number.
    assert!(graph.nodes.iter().any(|node| matches!(&node.kind, NodeKind::Levels(levels) if levels.world_areas)));
    assert!(graph.nodes.iter().any(|node| matches!(&node.kind, NodeKind::ClimateMask(mask) if mask.world_biome == 2)));
    let _ = Graph::default();
}
