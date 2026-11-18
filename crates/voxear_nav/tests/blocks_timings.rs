//! How long navigation in a world of blocks takes, on generated land: each
//! chunk's places, a world's first build, an edit made into it, and ways
//! of 100 and 500 blocks. Each is the least of many runs, so a machine
//! busy with something else measures what it can do, not what it was let
//! do; each prints what it measured.
//!
//! The targets are for optimised builds. A test build runs the engine's
//! own crates barely optimised, so here each is allowed four times over.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use voxear_nav::{
    BlockNavBuilder, PathAsk, PathStatus,
    blocks::{
        places::ColumnBuilder,
        search::find_path,
        walking::{WALKERS, WalkTable, Walker},
    },
};
use voxear_terrain::{
    blocks::{
        BlockMap,
        file::{BlockFile, Ground},
        generate::{BlockGenerate, column_chunks},
    },
    generate::Preset,
};

/// How much slower a test build may be than the targets.
const SLACK: f64 = if cfg!(debug_assertions) { 4.0 } else { 1.0 };

/// Rolling land `columns` chunk columns a side, blocks a metre a side, with
/// caves or without.
fn land(columns: i32, caves: bool) -> (BlockMap, Vec<voxear_terrain::blocks::BlockFaces>) {
    let file = BlockFile {
        block_size: 1.0,
        ground: Ground { size: [0, 0], ..Ground::default() },
        generate: Some(BlockGenerate { caves: if caves { 1.0 } else { 0.0 }, bottom: -16, ..BlockGenerate::default() }),
        ..BlockFile::default()
    };
    let (_, faces) = file.faces();
    let generate = file.generate.clone().unwrap();
    let world = Preset::RollingHills.world();
    let at: Vec<[i32; 2]> = (0..columns).flat_map(|x| (0..columns).map(move |z| [x, z])).collect();
    let made = voxear_terrain::workers::each(&at, |&at| column_chunks(&file, &generate, &world, 0, at, None));
    let mut blocks = BlockMap::default();
    for (key, chunk) in made.into_iter().flatten() {
        blocks.put_chunk(key, chunk);
    }
    (blocks, faces)
}

/// The least time `run` takes over `times` runs.
fn least(times: usize, mut run: impl FnMut()) -> Duration {
    (0..times)
        .map(|_| {
            let began = Instant::now();
            run();
            began.elapsed()
        })
        .min()
        .unwrap()
}

fn ms(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

#[test]
fn a_chunk_of_land_has_its_places_made_in_well_under_a_millisecond() {
    for (caves, target) in [(false, 0.5), (true, 1.0)] {
        let (blocks, faces) = land(9, caves);
        let table = WalkTable::of(&faces);
        // The chunks with ground in them and the one over each.
        let mut keys: Vec<[i32; 3]> = blocks.chunks().map(|(key, _)| *key).collect();
        keys.sort_unstable();
        let mut each = Vec::new();
        let mut bytes = Vec::new();
        for key in &keys {
            let mut made = None;
            each.push(least(20, || made = Some(ColumnBuilder::new(&blocks, &table).build(*key))));
            let made = made.unwrap();
            if !made.is_empty() {
                bytes.push(made.bytes());
            }
        }
        let mean = each.iter().map(|time| ms(*time)).sum::<f64>() / each.len() as f64;
        let worst = each.iter().map(|time| ms(*time)).fold(0.0, f64::max);
        let most_bytes = bytes.iter().copied().max().unwrap_or(0);
        println!(
            "places, caves {caves}: {} chunks, {mean:.3} ms each on average, {worst:.3} ms at worst, {most_bytes} bytes at most",
            keys.len()
        );
        assert!(mean < target * SLACK, "{mean:.3} ms a chunk on average");
        assert!(most_bytes < 8 * 1024 * if caves { 2 } else { 1 }, "{most_bytes} bytes");
    }
}

#[test]
fn a_world_of_land_is_built_and_an_edit_made_into_it_quickly() {
    let (blocks, faces) = land(9, false);
    let faces: Arc<[voxear_terrain::blocks::BlockFaces]> = faces.into();
    let mut built = None;
    let whole = least(3, || {
        let mut builder = BlockNavBuilder::new(&WALKERS);
        builder.settle(&Arc::new(blocks.clone()), &faces);
        built = Some(builder);
    });
    let mut builder = built.unwrap();
    let (chunks, places) = builder.nav().counts();
    println!("a world of 9 × 9 columns: {chunks} chunks, {places} places, built in {:.1} ms", ms(whole));
    // With regions for a walker two blocks wide as well, as a game's worlds have them.
    let mut walkers = WALKERS.to_vec();
    walkers.push(voxear_nav::Walker::blocks(3).wide(2).class());
    let wide = least(3, || {
        let mut builder = BlockNavBuilder::new(&walkers);
        builder.settle(&Arc::new(blocks.clone()), &faces);
    });
    println!("and with regions for a walker two blocks wide: built in {:.1} ms", ms(wide));
    assert!(ms(wide) < ms(whole) * 2.0 + 5.0, "{:.1} ms against {:.1} ms", ms(wide), ms(whole));

    // A block set on the ground in the middle, again and again.
    let mut edited = blocks.clone();
    let mut height = 0;
    let edit = least(20, || {
        let top = (0..120).rev().find(|y| edited.get([140, *y, 140]) != 0).unwrap_or(0);
        edited.set([140, top + 1, 140], 1);
        height = top + 1;
        let shared = Arc::new(edited.clone());
        let began = Instant::now();
        builder.settle(&shared, &faces);
        let _ = began;
    });
    println!("a block set: made into the navigation in {:.3} ms", ms(edit));
    assert!(ms(edit) < 1.0 * SLACK * 2.0, "{:.3} ms, cloning the blocks too", ms(edit));
    assert!(height > 0);
}

#[test]
fn ways_of_a_hundred_and_five_hundred_blocks_are_found_within_a_millisecond() {
    let (blocks, faces) = land(18, false);
    let mut builder = BlockNavBuilder::new(&WALKERS);
    builder.settle(&Arc::new(blocks), &faces.into());
    let nav = builder.nav().clone();
    let walker = Walker::blocks(2);
    let ground = |x: f32, z: f32| {
        let at = nav.nearest([x, 200.0, z], walker, 6.0, 400.0).expect("ground there");
        nav.point(at).unwrap()
    };
    for (from, to, target) in [((20.0, 20.0), (90.0, 90.0), 0.2), ((10.0, 10.0), (366.0, 366.0), 1.0)] {
        let (from, to) = (ground(from.0, from.1), ground(to.0, to.1));
        let ask = PathAsk::new(from, to, walker);
        let mut found = None;
        let time = least(20, || found = Some(find_path(&nav, &ask)));
        let found = found.unwrap();
        let far = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
        println!(
            "a way of {far:.0} blocks: {:?}, {} corners, {} looked at, {:.3} ms",
            found.status,
            found.corners.len(),
            found.looked,
            ms(time)
        );
        assert_eq!(found.status, PathStatus::Complete);
        assert!(ms(time) < target * SLACK, "{:.3} ms", ms(time));
    }
}
