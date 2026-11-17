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
