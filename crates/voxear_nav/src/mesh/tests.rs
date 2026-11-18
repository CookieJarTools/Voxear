use std::time::Instant;

use super::*;

/// The triangles of a box from `low` to `high`.
fn boxed(low: Vec3, high: Vec3) -> Vec<[Vec3; 3]> {
    let c = |x: bool, y: bool, z: bool| {
        Vec3::new(if x { high.x } else { low.x }, if y { high.y } else { low.y }, if z { high.z } else { low.z })
    };
    let quads = [
        [c(false, true, false), c(true, true, false), c(true, true, true), c(false, true, true)],
        [c(false, false, false), c(false, false, true), c(true, false, true), c(true, false, false)],
        [c(false, false, false), c(true, false, false), c(true, true, false), c(false, true, false)],
        [c(false, false, true), c(false, true, true), c(true, true, true), c(true, false, true)],
        [c(false, false, false), c(false, true, false), c(false, true, true), c(false, false, true)],
        [c(true, false, false), c(true, false, true), c(true, true, true), c(true, true, false)],
    ];
    quads.iter().flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]]).collect()
}

/// A floor `size` metres square, its top at 0.
fn floor(size: f32) -> Vec<[Vec3; 3]> {
    boxed(Vec3::new(0.0, -0.5, 0.0), Vec3::new(size, 0.0, size))
}

fn length(path: &MeshPath) -> f32 {
    path.corners.windows(2).map(|pair| pair[0].distance(pair[1])).sum()
}

#[test]
fn a_floor_is_one_straight_way_and_a_box_on_it_is_gone_round_off_its_sides() {
    let settings = BakeSettings::default();
    let open = bake(&floor(20.0), settings).unwrap();
    let (from, to) = (Vec3::new(2.0, 0.0, 10.0), Vec3::new(18.0, 0.0, 10.0));
    let straight = open.find_path(from, to, 1.0);
    assert_eq!(straight.status, MeshPathStatus::Complete);
    assert_eq!(straight.corners.len(), 2, "{:?}", straight.corners);
    assert!((length(&straight) - 16.0).abs() < 1e-3);

    let mut walled = floor(20.0);
    walled.extend(boxed(Vec3::new(9.0, 0.0, 4.0), Vec3::new(11.0, 3.0, 16.0)));
    let mesh = bake(&walled, settings).unwrap();
    let round = mesh.find_path(from, to, 1.0);
    assert_eq!(round.status, MeshPathStatus::Complete);
    assert!(round.corners.len() >= 3, "round the box: {:?}", round.corners);
    for corner in &round.corners {
        let inside = (9.0 - settings.radius + 0.01..=11.0 + settings.radius - 0.01).contains(&corner.x)
            && (4.0 - settings.radius + 0.01..=16.0 + settings.radius - 0.01).contains(&corner.z);
        assert!(!inside, "kept off the box by its radius: {corner}");
        assert!(corner.y.abs() < 0.05, "on the floor: {corner}");
    }
    assert!(length(&round) < 16.0 + 2.0 * 6.5, "the short way round: {}", length(&round));
    // Nothing on the box's top is reachable from the floor, but it is a place.
    assert!(mesh.locate(Vec3::new(10.0, 3.0, 10.0), 0.5).is_some());
    assert_eq!(mesh.find_path(from, Vec3::new(10.0, 3.0, 10.0), 0.5).status, MeshPathStatus::Partial);
}

#[test]
fn a_ramp_joins_two_floors_and_one_too_steep_does_not() {
    for (rise, joined) in [(1.5, true), (6.0, false)] {
        let settings = BakeSettings::default();
        // A low floor, a ramp up along x over 4 metres, and a high floor.
        let mut triangles = boxed(Vec3::new(0.0, -0.5, 0.0), Vec3::new(8.0, 0.0, 6.0));
        triangles.extend(boxed(Vec3::new(12.0, rise - 0.5, 0.0), Vec3::new(20.0, rise, 6.0)));
        let (a, b, c, d) = (
            Vec3::new(8.0, 0.0, 0.0),
            Vec3::new(12.0, rise, 0.0),
            Vec3::new(12.0, rise, 6.0),
            Vec3::new(8.0, 0.0, 6.0),
        );
        triangles.extend([[a, b, c], [a, c, d]]);
        let mesh = bake(&triangles, settings).unwrap();
        let way = mesh.find_path(Vec3::new(2.0, 0.0, 3.0), Vec3::new(18.0, rise, 3.0), 1.0);
        let reached = way.status == MeshPathStatus::Complete;
        assert_eq!(reached, joined, "a rise of {rise} over 4 m: {:?}", way.corners);
        if reached {
            let end = *way.corners.last().unwrap();
            assert!((end.y - rise).abs() < 0.1, "up on the high floor: {end}");
        }
    }
}

#[test]
fn a_floor_over_a_floor_keeps_its_ways_apart() {
    let settings = BakeSettings::default();
    let mut triangles = floor(20.0);
    // An upper floor 4 metres up over half the lower, with nothing joining them.
    triangles.extend(boxed(Vec3::new(0.0, 3.8, 0.0), Vec3::new(10.0, 4.0, 20.0)));
    let mesh = bake(&triangles, settings).unwrap();
    let (low, _) = mesh.locate(Vec3::new(5.0, 0.0, 10.0), 0.5).unwrap();
    let (high, _) = mesh.locate(Vec3::new(5.0, 4.0, 10.0), 0.5).unwrap();
    assert_ne!(low, high, "two floors at one place");
    let below = mesh.find_path(Vec3::new(2.0, 0.0, 2.0), Vec3::new(18.0, 0.0, 18.0), 0.5);
    assert_eq!(below.status, MeshPathStatus::Complete);
    assert!(below.corners.iter().all(|corner| corner.y.abs() < 0.05), "under the upper floor: {:?}", below.corners);
    let up = mesh.find_path(Vec3::new(2.0, 0.0, 2.0), Vec3::new(5.0, 4.0, 10.0), 0.5);
    assert_eq!(up.status, MeshPathStatus::Partial);
}

#[test]
fn a_way_round_a_corner_turns_at_the_corner_and_not_wide_of_it() {
    // An L of corridor: along x, then along z, walls round it.
    let settings = BakeSettings::default();
    let mut triangles = floor(20.0);
    triangles.extend(boxed(Vec3::new(0.0, 0.0, 4.0), Vec3::new(16.0, 3.0, 20.0)));
    let mesh = bake(&triangles, settings).unwrap();
    let way = mesh.find_path(Vec3::new(1.0, 0.0, 2.0), Vec3::new(18.0, 0.0, 18.0), 0.5);
    assert_eq!(way.status, MeshPathStatus::Complete);
    // Turning only at the inside of the turn, where the corridor's edge is cut a cell or two round the corner.
    assert!(way.corners.len() <= 4, "{:?}", way.corners);
    for corner in &way.corners[1..way.corners.len() - 1] {
        assert!((corner.x - 16.0).abs() < 1.0 && (corner.z - 4.0).abs() < 1.0, "{corner}");
    }
}

#[test]
fn a_nav_mesh_is_kept_as_bytes_and_read_back_the_same() {
    let mut triangles = floor(12.0);
    triangles.extend(boxed(Vec3::new(5.0, 0.0, 5.0), Vec3::new(7.0, 2.0, 7.0)));
    let mesh = bake(&triangles, BakeSettings::default()).unwrap();
    let bytes = mesh.to_bytes();
    assert_eq!(NavMesh::from_bytes(&bytes).as_ref(), Some(&mesh));
    assert!(NavMesh::from_bytes(&bytes[..10]).is_none());
}

#[test]
fn a_level_two_hundred_metres_square_bakes_in_seconds_and_a_long_way_over_it_is_found_quickly() {
    let slack = if cfg!(debug_assertions) { 4.0 } else { 1.0 };
    let mut triangles = floor(200.0);
    for x in 0..12 {
        for z in 0..12 {
            let (px, pz) = (8.0 + x as f32 * 16.0, 8.0 + z as f32 * 16.0);
            triangles.extend(boxed(Vec3::new(px, 0.0, pz), Vec3::new(px + 6.0, 4.0, pz + 3.0)));
        }
    }
    let began = Instant::now();
    let mesh = bake(&triangles, BakeSettings::default()).unwrap();
    let baked = began.elapsed().as_secs_f64();
    let (polys, cells, bytes) = mesh.counts();
    println!("baked 200 m square: {polys} polygons, {cells} cells, {} KB, in {baked:.2} s", bytes / 1024);
    assert!(baked < 10.0 * slack, "{baked:.2} s");
    let (from, to) = (Vec3::new(2.0, 0.0, 2.0), Vec3::new(198.0, 0.0, 197.0));
    let mut found = MeshPath::default();
    let least = (0..20)
        .map(|_| {
            let began = Instant::now();
            found = mesh.find_path(from, to, 1.0);
            began.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::INFINITY, f64::min);
    println!(
        "a way of {:.0} m: {} corners, {} polygons looked at, {least:.3} ms",
        length(&found),
        found.corners.len(),
        found.looked
    );
    assert_eq!(found.status, MeshPathStatus::Complete);
    assert!(least < 0.5 * slack * 2.0, "{least:.3} ms");
}

#[test]
fn a_box_cut_out_of_a_floor_is_gone_round_as_one_baked_in_is_and_taking_it_away_leaves_the_floor_as_baked() {
    let settings = BakeSettings::default();
    let open = bake(&floor(20.0), settings).unwrap();
    let (from, to) = (Vec3::new(2.0, 0.0, 10.0), Vec3::new(18.0, 0.0, 10.0));
    let crate_box = [Vec3::new(9.0, 0.0, 4.0), Vec3::new(11.0, 3.0, 16.0)];
    let cut = open.cut(&[crate_box]);
    let round = cut.find_path(from, to, 1.0);
    assert_eq!(round.status, MeshPathStatus::Complete);
    assert!(round.corners.len() >= 3, "round the box: {:?}", round.corners);
    for pair in round.corners.windows(2) {
        // Every point along the way, not only its corners, kept off the box by the radius.
        for step in 0..=40 {
            let at = pair[0].lerp(pair[1], step as f32 / 40.0);
            let inside = (9.0 - settings.radius + 0.01..=11.0 + settings.radius - 0.01).contains(&at.x)
                && (4.0 - settings.radius + 0.01..=16.0 + settings.radius - 0.01).contains(&at.z);
            assert!(!inside, "kept off the box by its radius: {at}");
        }
    }
    let mut walled = floor(20.0);
    walled.extend(boxed(crate_box[0], crate_box[1]));
    let baked = bake(&walled, settings).unwrap().find_path(from, to, 1.0);
    assert!((length(&round) - length(&baked)).abs() < 0.6, "cut {} against baked {}", length(&round), length(&baked));
    // Under the box there is nowhere to stand now.
    assert!(cut.locate(Vec3::new(10.0, 0.0, 10.0), 0.5).is_none());
    assert_eq!(open.cut(&[]), open);
    let straight = open.cut(&[]).find_path(from, to, 1.0);
    assert_eq!(straight.corners.len(), 2);
}

#[test]
fn a_box_across_a_corridor_shuts_it_and_one_over_heads_or_on_another_floor_does_not() {
    let settings = BakeSettings::default();
    // A corridor 3 metres wide along x, walls either side, and an upper floor over its far half.
    let mut triangles = boxed(Vec3::new(0.0, -0.5, 0.0), Vec3::new(30.0, 0.0, 3.0));
    triangles.extend(boxed(Vec3::new(0.0, 0.0, -1.0), Vec3::new(30.0, 3.0, 0.0)));
    triangles.extend(boxed(Vec3::new(0.0, 0.0, 3.0), Vec3::new(30.0, 3.0, 4.0)));
    triangles.extend(boxed(Vec3::new(15.0, 6.0, -1.0), Vec3::new(30.0, 6.2, 4.0)));
    let mesh = bake(&triangles, settings).unwrap();
    let (from, to) = (Vec3::new(1.0, 0.0, 1.5), Vec3::new(28.0, 0.0, 1.5));
    assert_eq!(mesh.find_path(from, to, 0.5).status, MeshPathStatus::Complete);
    let across = [Vec3::new(10.0, 0.0, -0.5), Vec3::new(11.0, 1.0, 3.5)];
    let shut = mesh.cut(&[across]);
    let way = shut.find_path(from, to, 0.5);
    assert_eq!(way.status, MeshPathStatus::Partial);
    assert!(way.corners.last().unwrap().x < 10.0, "stops short of the box: {:?}", way.corners);
    // A beam high over heads, and a box on the upper floor, leave the corridor open.
    let beam = [Vec3::new(10.0, 2.2, -0.5), Vec3::new(11.0, 2.6, 3.5)];
    let upstairs = [Vec3::new(20.0, 6.2, -0.5), Vec3::new(21.0, 7.2, 4.5)];
    let open = mesh.cut(&[beam, upstairs]);
    assert_eq!(open.find_path(from, to, 0.5).status, MeshPathStatus::Complete);
    let up = Vec3::new(20.5, 6.2, 1.5);
    assert!(mesh.locate(up, 0.5).is_some() && open.locate(up, 0.5).is_none(), "the upper floor is cut there");
    // The same box twice, or two overlapping, cut as one does.
    let twice = mesh.cut(&[across, across]);
    assert_eq!(twice.find_path(from, to, 0.5).status, MeshPathStatus::Partial);
}

#[test]
fn obstacles_cut_out_of_a_level_two_hundred_metres_square_take_a_few_milliseconds() {
    let slack = if cfg!(debug_assertions) { 4.0 } else { 1.0 };
    let mut triangles = floor(200.0);
    for x in 0..12 {
        for z in 0..12 {
            let (px, pz) = (8.0 + x as f32 * 16.0, 8.0 + z as f32 * 16.0);
            triangles.extend(boxed(Vec3::new(px, 0.0, pz), Vec3::new(px + 6.0, 4.0, pz + 3.0)));
        }
    }
    let mesh = bake(&triangles, BakeSettings::default()).unwrap();
    // Forty carts left in the streets between the blocks.
    let boxes: Vec<[Vec3; 2]> = (0..40)
        .map(|n| {
            let (x, z) = (4.0 + (n % 8) as f32 * 24.0, 5.0 + (n / 8) as f32 * 37.0);
            [Vec3::new(x, 0.0, z), Vec3::new(x + 1.5, 1.2, z + 2.5)]
        })
        .collect();
    let mut cut = NavMesh::default();
    let least = (0..10)
        .map(|_| {
            let began = Instant::now();
            cut = mesh.cut(&boxes);
            began.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::INFINITY, f64::min);
    println!("cut round {} boxes: {} polygons from {}, {least:.2} ms", boxes.len(), cut.polys.len(), mesh.polys.len());
    let way = cut.find_path(Vec3::new(2.0, 0.0, 2.0), Vec3::new(198.0, 0.0, 197.0), 1.0);
    assert_eq!(way.status, MeshPathStatus::Complete);
    for [low, high] in &boxes {
        for pair in way.corners.windows(2) {
            for step in 0..=50 {
                let at = pair[0].lerp(pair[1], step as f32 / 50.0);
                let inside = at.x > low.x && at.x < high.x && at.z > low.z && at.z < high.z;
                assert!(!inside, "through a cart at {at}");
            }
        }
    }
    assert!(least < 15.0 * slack, "{least:.2} ms");
}

#[test]
fn a_link_joins_two_floors_walking_does_not_and_is_taken_only_where_it_is_shorter() {
    let settings = BakeSettings::default();
    // Two floors side by side with a wall between them, and one far round.
    let mut triangles = floor(30.0);
    triangles.extend(boxed(Vec3::new(14.0, 0.0, 0.0), Vec3::new(16.0, 3.0, 26.0)));
    let mesh = bake(&triangles, settings).unwrap();
    let (from, to) = (Vec3::new(5.0, 0.0, 5.0), Vec3::new(25.0, 0.0, 5.0));
    let round = mesh.find_path(from, to, 0.5);
    assert_eq!(round.status, MeshPathStatus::Complete);
    assert!(round.links.iter().all(Option::is_none));
    // A door through the wall that takes you to its far side at once.
    let door = MeshLink { from: Vec3::new(13.0, 0.0, 6.0), to: Vec3::new(17.0, 0.0, 6.0), cost: 0.5 };
    let through = mesh.find_path_linked(from, to, 0.5, &[door]);
    assert_eq!(through.status, MeshPathStatus::Complete);
    assert!(length(&through) < length(&round) - 20.0, "{} against {}", length(&through), length(&round));
    let taken = through.links.iter().position(|link| *link == Some(0)).expect("the door is taken");
    assert!(through.corners[taken].distance(door.to) < 0.5 && through.corners[taken - 1].distance(door.from) < 0.5);
    assert_eq!(through.links.len(), through.corners.len());
    // One far down the wall's other end costs more than going round: left.
    let far = MeshLink { from: Vec3::new(13.0, 0.0, 25.0), to: Vec3::new(17.0, 0.0, 25.0), cost: 40.0 };
    assert!(mesh.find_path_linked(from, to, 0.5, &[far]).links.iter().all(Option::is_none));
    // A link up onto a box nothing walks up to reaches it.
    let up = MeshLink { from: Vec3::new(10.0, 0.0, 28.0), to: Vec3::new(15.0, 3.0, 20.0), cost: 1.0 };
    let onto = mesh.find_path_linked(from, Vec3::new(15.0, 3.0, 10.0), 0.5, &[up]);
    assert_eq!(onto.status, MeshPathStatus::Complete, "{:?}", onto.corners);
}
