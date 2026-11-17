use std::sync::Arc;

use voxear_terrain::blocks::BlockMap;

use super::*;
use crate::blocks::{
    search::{PathAsk, PathStatus, find_path},
    tests::{FACES, STONE, floor},
    walking::WALKERS,
};

fn on(at: [i32; 3]) -> [f32; 3] {
    [at[0] as f32 + 0.5, at[1] as f32, at[2] as f32 + 0.5]
}

#[test]
fn a_block_set_in_the_way_is_gone_round_from_the_next_frame() {
    let faces: Arc<[BlockFaces]> = Arc::from(&FACES[..]);
    let mut blocks = floor();
    let mut builder = BlockNavBuilder::new(&WALKERS);
    let shared = Arc::new(blocks.clone());
    assert!(builder.update(&shared, &faces), "a small world's places at once");
    let ask = PathAsk::new(on([2, 1, 5]), on([12, 1, 5]), Walker::blocks(2));
    let before = find_path(builder.nav(), &ask);
    assert_eq!(before.corners.len(), 2);

    // A wall across the way, with a gap at one end.
    for z in 0..15 {
        blocks.set([7, 1, z], STONE);
        blocks.set([7, 2, z], STONE);
    }
    let changed = Arc::new(blocks.clone());
    builder.update(&changed, &faces);
    // The next frame: a small build is waited for.
    builder.update(&changed, &faces);
    assert!(builder.is_settled(), "taken the frame after");
    let after = find_path(builder.nav(), &ask);
    assert!(before.is_stale(builder.nav()), "the way went through a chunk that changed");
    assert_eq!(after.status, PathStatus::Complete);
    assert!(after.corners.len() > 2, "round the wall: {:?}", after.corners);
    assert!(builder.is_settled());
}

#[test]
fn chunks_that_go_take_their_places_and_those_that_come_back_bring_them() {
    let faces: Arc<[BlockFaces]> = Arc::from(&FACES[..]);
    let mut blocks = BlockMap::default();
    for x in 0..96 {
        for z in 0..32 {
            blocks.set([x, 0, z], STONE);
        }
    }
    let mut builder = BlockNavBuilder::new(&WALKERS);
    builder.settle(&Arc::new(blocks.clone()), &faces);
    let ask = PathAsk::new(on([2, 1, 5]), on([90, 1, 5]), Walker::blocks(2));
    assert_eq!(find_path(builder.nav(), &ask).status, PathStatus::Complete);
    let (chunks, places) = builder.nav().counts();
    assert_eq!((chunks, places), (3, 96 * 32));

    // The middle chunk streams out: the way stops at its edge.
    let mut gone = blocks.clone();
    gone.put_chunk([1, 0, 0], voxear_terrain::blocks::chunk::Chunk::filled(0));
    builder.settle(&Arc::new(gone), &faces);
    assert_eq!(builder.nav().counts().0, 2);
    let partial = find_path(builder.nav(), &ask);
    assert_eq!(partial.status, PathStatus::Partial);
    assert!(partial.corners.last().unwrap()[0] < 32.0);

    // And streams back in.
    builder.settle(&Arc::new(blocks), &faces);
    assert_eq!(builder.nav().counts(), (3, 96 * 32));
    assert_eq!(find_path(builder.nav(), &ask).status, PathStatus::Complete);
}

#[test]
fn a_block_numbered_for_the_first_time_changes_only_the_chunks_it_is_in() {
    let mut faces = FACES.to_vec();
    let mut blocks = BlockMap::default();
    for x in 0..64 {
        for z in 0..32 {
            blocks.set([x, 0, z], STONE);
        }
    }
    let mut builder = BlockNavBuilder::new(&WALKERS);
    builder.settle(&Arc::new(blocks.clone()), &Arc::from(&faces[..]));
    let before = builder.nav().clone();
    faces.push(voxear_terrain::blocks::BlockFaces::new(0, 0, 0));
    blocks.set([40, 1, 4], 3);
    builder.settle(&Arc::new(blocks), &Arc::from(&faces[..]));
    let after = builder.nav();
    let changed: Vec<[i32; 3]> =
        after.slots().filter(|(_, slot)| slot.changed > before.version()).map(|(_, slot)| slot.key).collect();
    assert_eq!(changed, vec![[1, 0, 0]], "only the chunk with the new block");
}
