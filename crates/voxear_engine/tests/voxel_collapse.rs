//! What was built in an endless Voxel Volume giving way while a game runs:
//! a house with its corner blown out falls in, its pieces coming to rest
//! and stamped back into the volume, the rest of it standing; what a dig
//! cuts off falls; and no more pieces fall at once than the volume says
//! (headless: no GPU, no scripts).

use voxear_core::math::{IVec3, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    voxel_collapse::{BlastOptions, all_settled, blast, settle_after},
    voxel_felling::{CutPieces, PieceFall, VoxelPieceBody},
    voxel_volumes::{VoxelEditData, VoxelEditShape, VoxelVolumeSource, set_voxels, shape_edit},
};
use voxear_render::{VoxelMaterial, VoxelSolids, VoxelVolume};
use voxear_scene::Name;

/// Flat ground of its own a metre up, its corner at the world's middle, a camera over it.
const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.voxel_volume]\nsize = [24.0, 32.0, 24.0]\n\
                     endless = true\nreach = 32.0\n[obj.voxel_volume.ground]\nheight = 1.0\nhills = 0.0\ngrass = 0.0\n\n\
                     [[obj]]\nid = 2\nname = \"Eye\"\nposition = [8.0, 12.0, -6.0]\n[obj.camera]\n";

fn game() -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("voxear.toml", "[project]\nname = \"Street\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Street.scene\"\n".to_string()),
        ("Scenes/Street.scene", SCENE.to_string()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    (dir, game)
}

fn ground(game: &Game) -> Entity {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Ground").unwrap().0
}

fn volume(game: &Game) -> VoxelVolume {
    game.engine.world.get::<VoxelVolume>(ground(game)).unwrap().clone()
}

fn boxed(at: [f32; 3], size: [f32; 3], fill: Option<VoxelMaterial>) -> VoxelEditData {
    VoxelEditData { shape: VoxelEditShape::Box, at, size, fill, ..VoxelEditData::default() }
}

/// A house of bricks, 5 by 4 m and 2.5 m to its roof of planks, its corner at (4, 4): walls a quarter metre
/// thick and a door on its front.
fn build_house(game: &mut Game) {
    let world = &mut game.engine.world;
    let edits = [
        boxed([6.5, 1.0, 6.0], [5.0, 2.5, 4.0], Some(VoxelMaterial::Bricks)),
        boxed([6.5, 1.0, 6.0], [4.5, 2.5, 3.5], None),
        boxed([7.5, 1.0, 4.0], [1.0, 2.0, 0.6], None),
        boxed([6.5, 3.5, 6.0], [5.5, 0.25, 4.5], Some(VoxelMaterial::Planks)),
    ];
    for edit in edits {
        assert_eq!(shape_edit(world, &edit, None), 1);
    }
}

/// How many voxels of the box from `low` to `high` (in metres from the volume's corner) are solid, as collision
/// finds them.
fn solid_in(volume: &VoxelVolume, low: Vec3, high: Vec3) -> usize {
    let (low, high) = ((low * 16.0).as_ivec3(), (high * 16.0).as_ivec3());
    let size = high - low;
    let mut solid = vec![false; (size.x * size.y * size.z) as usize];
    VoxelSolids::of(volume).unwrap().fill(low.to_array(), size.to_array(), &mut solid);
    solid.iter().filter(|s| **s).count()
}

/// Ticks once; the game runs faster than the clock here, so while the volume's thread works out what gives way, the
/// clock is let catch up.
fn tick(game: &mut Game) {
    if !all_settled(&game.engine.world) {
        std::thread::sleep(std::time::Duration::from_millis(1000 / 60));
    }
    game.tick(1.0 / 60.0, None);
}

/// Plays until the volume has settled and no piece of what was built falls, `most` seconds at most: how long it
/// took, and the most pieces that fell at once.
fn play_until_settled(game: &mut Game, most: f32) -> (f32, usize) {
    let (mut time, mut most_live) = (0.0, 0);
    while time < most {
        tick(game);
        time += 1.0 / 60.0;
        let live = game
            .engine
            .world
            .query_ref::<&VoxelPieceBody>()
            .into_iter()
            .filter(|body| body.built && body.fall != PieceFall::Resting)
            .count();
        most_live = most_live.max(live);
        if live == 0 && all_settled(&game.engine.world) && time > 0.5 {
            break;
        }
    }
    (time, most_live)
}

#[test]
fn a_house_with_its_corner_blown_out_falls_in_its_pieces_stamped_back_where_they_rest() {
    let (_dir, mut game) = game();
    build_house(&mut game);
    let before = volume(&game);
    let roof = |volume: &VoxelVolume| solid_in(volume, Vec3::new(3.7, 3.5, 3.7), Vec3::new(9.3, 3.75, 8.3));
    let roof_before = roof(&before);
    let far_wall = |volume: &VoxelVolume| solid_in(volume, Vec3::new(8.75, 1.1, 6.5), Vec3::new(9.0, 2.0, 8.0));
    let far_before = far_wall(&before);
    assert!(roof_before > 20_000 && far_before > 1000, "{roof_before} {far_before}");
    // The front corner blown out, a ball 3.6 m across high on the wall.
    let started = std::time::Instant::now();
    let done = blast(&mut game.engine.world, Vec3::new(4.0, 2.5, 4.0), 1.8, BlastOptions::default(), None);
    eprintln!("blast dug {} voxels in {:?}", done.dug, started.elapsed());
    assert_eq!(done.reached, 1);
    assert!(done.dug > 3000, "{done:?}");
    let (took, most_live) = play_until_settled(&mut game, 40.0);
    let after = volume(&game);
    let (roof_after, far_after) = (roof(&after), far_wall(&after));
    eprintln!(
        "settled in {took:.1} s, at most {most_live} pieces falling; roof {roof_before} -> {roof_after} voxels, far wall {far_before} -> {far_after}"
    );
    assert!(most_live >= 1 && most_live <= voxear_render::MOST_PIECES as usize, "{most_live}");
    // The roof over the corner came down; the wall across the house stands.
    assert!(roof_after < roof_before * 19 / 20, "{roof_after} of {roof_before}");
    assert!(far_after * 10 >= far_before * 9, "{far_after} of {far_before}");
    // Its pieces rested and were stamped back: more is placed in the house than before, nothing falls now.
    let falling = game.engine.world.query_ref::<&VoxelPieceBody>().into_iter().filter(|body| body.built).count();
    assert_eq!(falling, 0, "every piece stamped back");
    let placed = |volume: &VoxelVolume| -> usize {
        volume.changes.bricks().map(|(_, brick)| brick.codes.iter().filter(|code| **code != 0).count()).sum()
    };
    assert!(placed(&after) > placed(&before) + 500, "rubble placed where it lay: {}", placed(&after));
    // Kept with the volume, saved as its changes.
    let source = game.engine.world.get::<VoxelVolumeSource>(ground(&game)).unwrap();
    assert!(source.0.edits.len() == 4 && after.changes.count() > 1000);
}

#[test]
fn what_a_dig_cuts_off_falls_and_what_still_holds_stays() {
    let (_dir, mut game) = game();
    // What is cut off alone: a ledge held by a voxel bends nothing.
    let entity = ground(&game);
    let mut data = game.engine.world.get::<VoxelVolumeSource>(entity).unwrap().0.clone();
    data.strain = voxear_render::VoxelStrain::Off;
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, entity, data);
    // A ledge out of a wall, two metres up.
    let world = &mut game.engine.world;
    shape_edit(world, &boxed([4.0, 1.0, 4.0], [0.5, 3.0, 2.0], Some(VoxelMaterial::Ashlar)), None);
    shape_edit(world, &boxed([5.0, 3.0, 4.0], [1.5, 0.25, 1.0], Some(VoxelMaterial::Planks)), None);
    // Dug through where it meets the wall, all but one voxel: it still holds.
    let mut points = Vec::new();
    for z in 0..16 {
        for y in 0..4 {
            for x in 0..2 {
                points.push(Vec3::new(
                    4.25 + (x as f32 + 0.5) / 16.0,
                    3.0 + (y as f32 + 0.5) / 16.0,
                    3.5 + (z as f32 + 0.5) / 16.0,
                ));
            }
        }
    }
    // The last row kept, through both voxels of the cut.
    let last = [points.pop().unwrap(), points.pop().unwrap()];
    let was = volume(&game);
    set_voxels(&mut game.engine.world, &points, None, None);
    let (low, high) = (Vec3::new(4.2, 2.9, 3.4), Vec3::new(4.4, 3.4, 4.6));
    settle_after(&mut game.engine.world, entity, was, low, high, CutPieces::Fall);
    play_until_settled(&mut game, 5.0);
    let ledge = |volume: &VoxelVolume| solid_in(volume, Vec3::new(4.5, 3.0, 3.5), Vec3::new(5.75, 3.25, 4.5));
    assert_eq!(ledge(&volume(&game)), 20 * 4 * 16, "held by a row of two voxels");
    // The last one dug: it falls, and lies on the ground under where it was.
    let was = volume(&game);
    set_voxels(&mut game.engine.world, &last, None, None);
    settle_after(&mut game.engine.world, entity, was, low, high, CutPieces::Fall);
    let (took, _) = play_until_settled(&mut game, 20.0);
    let after = volume(&game);
    assert_eq!(ledge(&after), 0, "fell after {took:.1} s");
    let under = solid_in(&after, Vec3::new(4.3, 1.0, 3.0), Vec3::new(7.5, 1.5, 5.0));
    assert!(under > 800, "it lies on the ground: {under}");
    let _ = IVec3::ZERO;
}

#[test]
fn no_more_pieces_fall_at_once_than_the_volume_says_and_small_ones_are_dust() {
    let (_dir, mut game) = game();
    let entity = ground(&game);
    let mut data = game.engine.world.get::<VoxelVolumeSource>(entity).unwrap().0.clone();
    data.most_pieces = 2;
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, entity, data);
    // A row of slabs on posts: each post blown out drops its slab.
    for i in 0..5 {
        let x = 3.0 + i as f32 * 2.5;
        let world = &mut game.engine.world;
        shape_edit(world, &boxed([x, 1.0, 8.0], [0.5, 1.5, 0.5], Some(VoxelMaterial::Ashlar)), None);
        shape_edit(world, &boxed([x, 2.5, 8.0], [1.5, 0.25, 1.5], Some(VoxelMaterial::Flagstones)), None);
    }
    for i in 0..5 {
        let x = 3.0 + i as f32 * 2.5;
        let options = BlastOptions { throw: 0.0, wait: true, ..BlastOptions::default() };
        let done = blast(&mut game.engine.world, Vec3::new(x, 1.6, 8.0), 0.45, options, None);
        assert!(!done.pieces.is_empty(), "slab {i}: {done:?}");
    }
    for _ in 0..10 {
        game.tick(1.0 / 60.0, None);
    }
    let live = game.engine.world.query_ref::<&VoxelPieceBody>().into_iter().filter(|body| body.built).count();
    assert!(live <= 2, "{live} falling");
    // A sliver cut off is dust, no body.
    let world = &mut game.engine.world;
    shape_edit(world, &boxed([10.0, 1.0, 3.0], [0.25, 1.0, 0.25], Some(VoxelMaterial::Ashlar)), None);
    shape_edit(world, &boxed([10.0, 2.0, 3.0], [0.375, 0.125, 0.375], Some(VoxelMaterial::Ashlar)), None);
    let options = BlastOptions { throw: 0.0, wait: true, ..BlastOptions::default() };
    let done = blast(&mut game.engine.world, Vec3::new(10.0, 1.4, 3.0), 0.45, options, None);
    assert!(!done.pieces.is_empty() && done.pieces.iter().all(|piece| piece.obj.is_none()), "{done:?}");
}

/// A writer of bits, for what a snapshot of pieces would cost.
#[derive(Default)]
struct Bits {
    bits: u64,
}

impl Bits {
    fn put(&mut self, count: u32) {
        self.bits += u64::from(count);
    }

    /// A whole number of any size: four bits for how many bits it takes, then those.
    fn number(&mut self, n: u64) {
        self.put(4 + (64 - n.leading_zeros()).max(1));
    }

    /// A number less or more than none, folded to one that is not.
    fn signed(&mut self, n: i64) {
        self.number(((n << 1) ^ (n >> 63)) as u64);
    }

    fn bytes(&self) -> u64 {
        self.bits.div_ceil(8)
    }
}

/// A piece's place in whole millimetres, and its turn as its three smallest parts in whole 1/1024ths, and which was
/// left out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Pose {
    at: [i64; 3],
    turn: [i64; 3],
    largest: u8,
}

fn pose_of(position: Vec3, rotation: voxear_core::math::Quat) -> Pose {
    let at = position.to_array().map(|a| (a * 1000.0).round() as i64);
    let q = rotation.to_array();
    let largest = (0..4).max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs())).unwrap();
    let sign = if q[largest] < 0.0 { -1.0 } else { 1.0 };
    let rest: Vec<f32> = (0..4).filter(|&i| i != largest).map(|i| q[i] * sign).collect();
    let turn = [0, 1, 2].map(|i| (rest[i] * 1024.0 / std::f32::consts::FRAC_1_SQRT_2).round() as i64);
    Pose { at, turn, largest: largest as u8 }
}

/// What a snapshot of `now` costs sent against `acked` (what the client last said it had): each piece that moved, its
/// number and how far it moved since, its turn whole where it turned; one that is new, whole.
fn snapshot_bytes(now: &[(u32, Pose)], acked: &std::collections::HashMap<u32, Pose>) -> u64 {
    let mut bits = Bits::default();
    // The snapshot's number, and the one it is sent against.
    bits.put(32);
    let mut changed = 0;
    for (id, pose) in now {
        match acked.get(id) {
            Some(was) if was == pose => continue,
            Some(was) => {
                bits.number(u64::from(*id));
                bits.put(2);
                for a in 0..3 {
                    bits.signed(pose.at[a] - was.at[a]);
                }
                if pose.turn != was.turn || pose.largest != was.largest {
                    bits.put(2 + 3 * 11);
                }
            }
            None => {
                bits.number(u64::from(*id));
                bits.put(2 + 3 * 24 + 2 + 3 * 11);
            }
        }
        changed += 1;
    }
    bits.number(changed);
    bits.bytes()
}

#[test]
fn measure_how_many_bytes_a_house_falling_in_costs_over_the_network() {
    let (_dir, mut game) = game();
    build_house(&mut game);
    blast(&mut game.engine.world, Vec3::new(4.0, 2.5, 4.0), 1.8, BlastOptions::default(), None);
    // Twenty snapshots a second; each sent against the last the client said it had, its answers 100 ms behind.
    let (every, behind) = (3, 2);
    let mut ids: std::collections::HashMap<Entity, u32> = std::collections::HashMap::new();
    let mut sent: Vec<std::collections::HashMap<u32, Pose>> = Vec::new();
    let (mut total, mut most, mut snapshots, mut busy, mut stamped, mut made) = (0u64, 0u64, 0, 0, 0, 0);
    let mut live_before: Vec<u32> = Vec::new();
    for step in 0..60 * 40 {
        self::tick(&mut game);
        if step % every != 0 {
            continue;
        }
        let bodies: Vec<(Entity, Vec3, voxear_core::math::Quat)> = game
            .engine
            .world
            .query_ref::<(Entity, &VoxelPieceBody, &voxear_scene::Transform)>()
            .into_iter()
            .filter(|(_, body, _)| body.built)
            .map(|(e, _, t)| (e, t.position, t.rotation))
            .collect();
        let next = ids.len() as u32;
        let now: Vec<(u32, Pose)> = bodies
            .iter()
            .enumerate()
            .map(|(k, (e, p, r))| (*ids.entry(*e).or_insert(next + k as u32), pose_of(*p, *r)))
            .collect();
        made = ids.len();
        // Those gone since: stamped where they lay, a number and their last pose each.
        let gone = live_before.iter().filter(|id| !now.iter().any(|(n, _)| n == *id)).count();
        stamped += gone;
        let empty = std::collections::HashMap::new();
        let acked = sent.len().checked_sub(behind + 1).map_or(&empty, |at| &sent[at]);
        let bytes = snapshot_bytes(&now, acked) + gone as u64 * 12;
        if !now.is_empty() || gone > 0 {
            busy += 1;
        }
        total += bytes;
        most = most.max(bytes);
        snapshots += 1;
        live_before = now.iter().map(|(id, _)| *id).collect();
        sent.push(now.into_iter().collect());
        if live_before.is_empty() && all_settled(&game.engine.world) && step > 60 {
            break;
        }
    }
    eprintln!(
        "a house falling in: {made} pieces, {stamped} stamped; {busy} of {snapshots} snapshots with pieces, {total} bytes in all, at most {most} in one, {:.0} bytes a second while they fell",
        total as f32 / (busy.max(1) as f32 / 20.0)
    );
    assert!(made > 0 && total > 0);
}

/// Plays `seconds`, letting the clock catch up while the volume settles, and gives each step's bodies of what was
/// built: each its Obj, place, turn and whether it is hinged or hangs.
fn play_watching(game: &mut Game, seconds: f32, mut each: impl FnMut(&Game)) {
    let mut time = 0.0;
    while time < seconds {
        tick(game);
        time += 1.0 / 60.0;
        each(game);
    }
}

/// A floor of planks `long` metres out from a wall to a post, the post blown out, played 12 seconds: how many pieces
/// hung hinged, how many of those snapped off their hinge, how far one bent at most, in degrees, and how low the
/// middle of one went while hinged.
fn floor_blown_off_its_post(long: f32) -> (usize, usize, f32, f32) {
    use voxear_engine::voxel_collapse::PieceHinge;
    let (_dir, mut game) = game();
    let world = &mut game.engine.world;
    let post = 4.25 + long - 0.25;
    shape_edit(world, &boxed([4.0, 1.0, 8.0], [0.5, 3.5, 3.0], Some(VoxelMaterial::Ashlar)), None);
    shape_edit(world, &boxed([post, 1.0, 8.0], [0.5, 2.5, 0.5], Some(VoxelMaterial::Ashlar)), None);
    shape_edit(world, &boxed([4.25 + long * 0.5, 3.5, 8.0], [long, 0.25, 2.0], Some(VoxelMaterial::Planks)), None);
    let options = BlastOptions { throw: 0.0, ..BlastOptions::default() };
    blast(&mut game.engine.world, Vec3::new(post, 2.0, 8.0), 0.7, options, None);
    let mut seen: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let mut snapped: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    let (mut most_bent, mut lowest) = (0.0f32, f32::MAX);
    play_watching(&mut game, 12.0, |game| {
        let world = &game.engine.world;
        for (e, _, t) in world.query_ref::<(Entity, &PieceHinge, &voxear_scene::Transform)>().into_iter() {
            seen.insert(e);
            most_bent = most_bent.max(2.0 * t.rotation.w.abs().min(1.0).acos().to_degrees());
            lowest = lowest.min(t.position.y);
        }
        for e in &seen {
            if world.get::<PieceHinge>(*e).is_none()
                && world.get::<VoxelPieceBody>(*e).is_some_and(|b| b.fall != PieceFall::Resting)
            {
                snapped.insert(*e);
            }
        }
    });
    (seen.len(), snapped.len(), most_bent, lowest)
}

#[test]
fn a_timber_floor_blown_off_its_post_hangs_from_its_wall_bent_and_a_long_one_snaps() {
    // A metre and a half out: it bends and hangs on, never reaching the ground.
    let (hinged, snapped, bent, lowest) = floor_blown_off_its_post(1.5);
    eprintln!("1.5 m: {hinged} hinged, {snapped} snapped, bent at most {bent:.1} degrees, as low as {lowest:.2} m");
    assert!(hinged >= 1 && snapped == 0 && lowest > 2.5, "{hinged} {snapped} {bent} {lowest}");
    // Six metres out, twice as heavy twice as far as three would be: it bends until it snaps.
    let (hinged, snapped, bent, _) = floor_blown_off_its_post(6.0);
    eprintln!("6 m: {hinged} hinged, {snapped} snapped, bent at most {bent:.1} degrees");
    assert!(hinged >= 1 && snapped >= 1, "it bent through and snapped: {hinged} hinged, {snapped} snapped, {bent}");
}

#[test]
fn a_slab_blown_off_its_posts_hangs_from_a_rope_sways_a_little_and_falls_once_cut() {
    use voxear_engine::voxel_ropes::{RopeHolds, cut, hang};
    let (_dir, mut game) = game();
    let world = &mut game.engine.world;
    // A slab of planks two metres square on four posts, a rope tied three metres over its middle holding it.
    for (x, z) in [(7.25, 7.25), (8.75, 7.25), (7.25, 8.75), (8.75, 8.75)] {
        shape_edit(world, &boxed([x, 1.0, z], [0.5, 1.5, 0.5], Some(VoxelMaterial::Ashlar)), None);
    }
    shape_edit(world, &boxed([8.0, 2.5, 8.0], [2.0, 0.25, 2.0], Some(VoxelMaterial::Planks)), None);
    let rope = hang(&mut game.engine.world, Vec3::new(8.0, 5.75, 8.0), Vec3::new(8.03, 2.6, 8.03), 0.0, 0.0);
    play_watching(&mut game, 1.0, |_| {});
    // Its posts blown out, thrown a little: it hangs, and swings under where it is tied.
    for (x, z) in [(7.25, 7.25), (8.75, 7.25), (7.25, 8.75), (8.75, 8.75)] {
        blast(
            &mut game.engine.world,
            Vec3::new(x, 1.7, z),
            0.45,
            BlastOptions { throw: 1.0, ..BlastOptions::default() },
            None,
        );
    }
    let (mut hanging, mut swung, mut lowest, mut late) = (0, 0.0f32, f32::MAX, 0.0f32);
    let mut last: Option<Vec3> = None;
    let mut step = 0;
    play_watching(&mut game, 10.0, |game| {
        let world = &game.engine.world;
        step += 1;
        if let Some(piece) = world.get::<RopeHolds>(rope).and_then(|holds| holds.piece) {
            if let Some(t) = world.get::<voxear_scene::Transform>(piece) {
                hanging += 1;
                swung = swung.max((t.position.x - 8.0).abs().max((t.position.z - 8.0).abs()));
                lowest = lowest.min(t.position.y);
                // How far it moves in its last second.
                if step == 540 {
                    last = Some(t.position);
                }
                if step == 600
                    && let Some(was) = last
                {
                    late = t.position.distance(was);
                }
            }
        }
    });
    eprintln!(
        "hung for {hanging} steps, swayed {swung:.2} m across, its middle as low as {lowest:.2} m, moving {late:.3} m in its last second"
    );
    assert!(hanging > 100, "the slab hangs from the rope");
    // It sways a little, under where it is tied, and settles: a heavy thing hanging, not a light one on a string.
    assert!(swung > 0.02 && swung < 0.8, "it sways a little: {swung}");
    assert!(late < 0.05, "and settles: {late} m in its last second");
    assert!(lowest > 1.3, "under the rope, over the ground: {lowest}");
    // Cut: it falls and rests on the ground, stamped back.
    assert!(cut(&mut game.engine.world, rope));
    let before = volume(&game).changes.count();
    play_until_settled(&mut game, 15.0);
    let pieces = game.engine.world.query_ref::<&VoxelPieceBody>().into_iter().filter(|body| body.built).count();
    assert_eq!(pieces, 0, "it rested and was stamped back");
    assert!(volume(&game).changes.count() > before, "where it fell");
}

#[test]
fn what_was_built_held_by_nothing_but_a_rope_hangs_once_tied() {
    use voxear_engine::voxel_ropes::{RopeHolds, hang};
    let (_dir, mut game) = game();
    // A box built in the air, a rope tied over it.
    shape_edit(&mut game.engine.world, &boxed([8.0, 3.0, 8.0], [1.0, 1.0, 1.0], Some(VoxelMaterial::Planks)), None);
    let rope = hang(&mut game.engine.world, Vec3::new(9.0, 6.0, 8.0), Vec3::new(8.0, 3.9, 8.0), 0.0, 0.0);
    let mut hung = 0;
    play_watching(&mut game, 4.0, |game| {
        if game.engine.world.get::<RopeHolds>(rope).and_then(|holds| holds.piece).is_some() {
            hung += 1;
        }
    });
    assert!(hung > 60, "it hangs from the rope: {hung}");
    let held = game.engine.world.get::<RopeHolds>(rope).and_then(|holds| holds.piece).unwrap();
    let at = game.engine.world.get::<voxear_scene::Transform>(held).unwrap().position;
    // Tied off to one side, it swung under where it is tied.
    assert!(at.x > 8.2 && at.y > 2.5, "{at}");
}
